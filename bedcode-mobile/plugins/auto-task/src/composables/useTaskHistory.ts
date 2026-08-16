/**
 * 任务历史核心逻辑（移动端工具箱「任务记录」页）
 *
 * 数据源为对端桌面端 AutoTask 插件 HTTP 端点 task-history/list（分页 + 状态筛选）。
 * 实时性来自三路 WS 事件（scheduled/queue/status 变更）共用 500ms 去抖重拉：
 * 事件只作「有变化」信号，不依赖 payload 做局部更新（批量 missed/failed 广播
 * job_id 为空串，v1 简化全量重拉，数据量小）。下拉刷新与断线重连兜底。
 */
import { ref, computed } from 'vue'
import type { Disposable, PluginContext, MobileHostApi } from '@bedcode/plugin-sdk-mobile'
import { getMobileApi } from '@bedcode/plugin-sdk-mobile'

/** 任务历史条目（与桌面端 task_history 表字段一一对应） */
export interface TaskHistoryItem {
  id: string
  description: string | null
  status: string
  agent: string | null
  source: string | null
  session_id: string
  claude_sid: string | null
  working_dir: string | null
  auto_approve: number
  exit_reason: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
  input_tokens: number | null
  output_tokens: number | null
}

/** 状态筛选选项：'all' 表示不筛选（HTTP 层不传 status） */
export type HistoryStatusFilter = 'all' | 'in_progress' | 'completed' | 'interrupted' | 'failed'

/** 每页条数（桌面端 limit 上限 500，移动端固定小页） */
const PAGE_SIZE = 20

/** UTC "YYYY-MM-DD HH:MM:SS" → 本地时区展示文本（无日期库，手工补 T+Z 交给 Date） */
export function utcToLocalDisplay(utc: string | null | undefined): string {
  if (!utc) return '-'
  const d = new Date(utc.replace(' ', 'T') + 'Z')
  if (isNaN(d.getTime())) return utc
  return d.toLocaleString()
}

/** 耗时展示：completed_at − started_at（秒 → 自适应单位） */
export function formatDuration(start: string | null | undefined, end: string | null | undefined): string {
  if (!start || !end) return '-'
  const s = new Date(start.replace(' ', 'T') + 'Z').getTime()
  const e = new Date(end.replace(' ', 'T') + 'Z').getTime()
  if (isNaN(s) || isNaN(e)) return '-'
  const sec = Math.max(0, Math.round((e - s) / 1000))
  if (sec < 60) return `${sec}s`
  if (sec < 3600) return `${Math.floor(sec / 60)}m ${sec % 60}s`
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ${Math.floor((sec % 3600) / 60)}m`
  return `${Math.floor(sec / 86400)}d ${Math.floor((sec % 86400) / 3600)}h`
}

export function useTaskHistory(context: PluginContext) {
  const mobileApi = getMobileApi() as MobileHostApi

  const tasks = ref<TaskHistoryItem[]>([])
  const total = ref(0)
  const offset = ref(0)
  const limit = ref(PAGE_SIZE)
  const statusFilter = ref<HistoryStatusFilter>('all')
  const loading = ref(false)
  const loadingMore = ref(false)
  /** 未连接（HTTP 调用命中宿主 "No base URL set"）或请求失败 */
  const offline = ref(false)

  /** 是否还有下一页（offset 恒为“下一页起点”，与 total 直接比较即可） */
  const hasMore = computed(() => offset.value < total.value)

  // ==================== 数据加载 ====================

  /** 拉取一页；reset 为 true 时从第一页开始并整体替换列表 */
  async function load(reset: boolean): Promise<void> {
    if (reset) {
      loading.value = true
      offset.value = 0
    } else {
      loadingMore.value = true
    }
    try {
      const result = await mobileApi.httpTaskHistoryList({
        status: statusFilter.value === 'all' ? undefined : statusFilter.value,
        limit: PAGE_SIZE,
        offset: offset.value,
      })
      if (result.code === 0 && result.data) {
        offline.value = false
        const page = result.data.tasks || []
        tasks.value = reset ? page : [...tasks.value, ...page]
        total.value = result.data.total ?? 0
        // offset 前进为本次起点 + 返回条数（与后端 offset 语义一致）
        offset.value = reset ? page.length : offset.value + page.length
      } else {
        // 未连接 / 后端错误：保留旧数据，标记离线空态
        offline.value = true
      }
    } catch (e) {
      console.error('[AutoTask] task history load failed:', e)
      offline.value = true
    } finally {
      loading.value = false
      loadingMore.value = false
    }
  }

  /** 加载更多（底部按钮） */
  function loadMore(): void {
    if (loading.value || loadingMore.value || !hasMore.value) return
    void load(false)
  }

  /** 切换状态筛选：回到第一页重拉 */
  function setStatusFilter(status: HistoryStatusFilter): void {
    if (statusFilter.value === status) return
    statusFilter.value = status
    void load(true)
  }

  /** 下拉刷新：回到第一页重拉（保留当前筛选） */
  function refresh(): Promise<void> {
    return load(true)
  }

  // ==================== 事件订阅（500ms 去抖重拉） ====================

  /** 去抖计时器：任一相关事件都重置计时，到点才重拉，合并高频广播 */
  let debounceTimer: ReturnType<typeof setTimeout> | null = null
  /** 去抖触达时除历史页外还需一并重拉的页面（定时任务页由视图接线） */
  const debouncedReloadCallbacks: Array<() => void> = []
  let disposables: Disposable[] = []

  function onAnyTaskEvent(): void {
    if (debounceTimer) clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      debounceTimer = null
      // 事件到达时连接必然存在：直接重拉，避免高频广播下的重复请求
      void load(true)
      for (const cb of debouncedReloadCallbacks) cb()
    }, 500)
  }

  /** 注册去抖重拉的联动页面（定时任务页列表） */
  function onDebouncedReload(cb: () => void): void {
    debouncedReloadCallbacks.push(cb)
  }

  /** 建立事件订阅 + 首次加载（组件 onMounted 调用） */
  function start(): void {
    stop()
    disposables = [
      // 定时任务变更（创建/触发/失败/错过，job_id 可为空串的批量广播）
      context.events.on('ws_sync_task_scheduled_changed', onAnyTaskEvent),
      // 队列变更（自动队列任务入队/完成/移除，任务历史来源之一）
      context.events.on('ws_sync_task_queue_changed', onAnyTaskEvent),
      // 任务状态变更（执行中/完成/中断等，驱动历史页状态刷新）
      context.events.on('ws_sync_task_status_changed', onAnyTaskEvent),
    ]
    void load(true)
  }

  /** 摘除事件订阅（组件 onUnmounted 调用） */
  function stop(): void {
    for (const d of disposables) d.dispose()
    disposables = []
    if (debounceTimer) {
      clearTimeout(debounceTimer)
      debounceTimer = null
    }
  }

  return {
    tasks,
    total,
    hasMore,
    statusFilter,
    loading,
    loadingMore,
    offline,
    load,
    loadMore,
    setStatusFilter,
    refresh,
    onDebouncedReload,
    start,
    stop,
  }
}

export type TaskHistoryComposable = ReturnType<typeof useTaskHistory>
