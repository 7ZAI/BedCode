/**
 * Agent Hub 使用统计与会话日志域编排（票据 06 + 日志页改版）
 *
 * - 状态流：挂载时拉取持久化状态（host-storage `usage` 键），guest 每次扫描
 *   完成 emit `plugin:agent-hub:usage` 覆盖本地状态；auth-required 呈现授权
 *   入口（fs_auth 第三层按路径弹窗，guest 侧已闸门拦截，不再触发弹窗）
 * - 看板：stats 一次拉全部分组（按天/CLI/项目/模型），维度切换纯前端；
 *   扫描完成后自动重拉
 * - 会话列表：多条件查询（adapter / 关键词 / 时间范围）+ 分页（页大小
 *   PAGE_SIZE），供统计明细（追加加载）与日志页（页导航）共用同一查询域
 * - 日志来源：内置只读 + 自定义增删（list/add/remove-usage-source），
 *   扫描计数合并进 sources 列表
 * - 日志视图：openSession 按会话 id 读源文件解析为归一事件流 + 原始行
 *   （与统计共用同一适配器层，单会话一次读盘双消费）
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type {
  UsageDomainState,
  UsageSessionDetail,
  UsageSessionPage,
  UsageSessionRow,
  UsageSource,
  UsageStats,
} from '../types'

export type UseUsageReturn = ReturnType<typeof useUsage>

/** 日志页分页步长（guest 上限 200 内的合理页大小） */
export const PAGE_SIZE = 15

export function useUsage(context: PluginContext) {
  const state = ref<UsageDomainState | null>(null)
  const stats = ref<UsageStats | null>(null)
  const sessions = ref<UsageSessionRow[]>([])
  const sessionsTotal = ref(0)
  /** 追加加载游标（统计明细 load-more 用；页导航后重置） */
  let loadedOffset = 0
  /** 列表加载中（防分页重入） */
  const loadingSessions = ref(false)
  /** 当前打开的日志会话（null = 未选中） */
  const openedSession = ref<UsageSessionDetail | null>(null)
  const openingSession = ref(false)

  // ==================== 日志来源（内置只读 + 自定义增删） ====================
  const sources = ref<UsageSource[]>([])

  async function reloadSources() {
    try {
      const data = await context.commands.execute('agent-hub.list-usage-sources', {})
      sources.value = ((data as { sources: UsageSource[] })?.sources ?? []) as UsageSource[]
    } catch (e) {
      console.error('[Agent Hub] list-usage-sources failed', e)
    }
  }

  /** 添加目录：返回 { ok, error? }（guest 校验失败经命令拒绝透传消息） */
  async function addSource(name: string, path: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.add-usage-source', { name, path })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) }
    }
  }

  async function removeSource(name: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.remove-usage-source', { name })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) }
    }
  }

  // ==================== 会话列表（多条件查询 + 分页） ====================
  /** 适配器过滤（'' = 全部） */
  const listFilter = ref('')
  /** 关键词（标题 / 项目 / 会话 id 模糊匹配，空 = 不限定） */
  const searchText = ref('')
  /** 时间范围（epoch ms，null = 不限定） */
  const rangeFrom = ref<number | null>(null)
  const rangeTo = ref<number | null>(null)
  /** 日志页当前页码（1 基） */
  const page = ref(1)

  const totalPages = computed(() =>
    Math.max(1, Math.ceil(sessionsTotal.value / PAGE_SIZE)),
  )

  function buildQuery() {
    return {
      adapter: listFilter.value,
      q: searchText.value.trim(),
      from: rangeFrom.value,
      to: rangeTo.value,
    }
  }

  /** 拉取一页（replace=true 替换列表；false 追加——统计明细 load-more） */
  async function fetchSessionsPage(offset: number, replace: boolean) {
    loadingSessions.value = true
    try {
      const data = await context.commands.execute('agent-hub.list-usage-sessions', {
        offset,
        limit: PAGE_SIZE,
        ...buildQuery(),
      })
      const p = data as UsageSessionPage
      sessions.value = replace
        ? p.sessions
        : [...sessions.value, ...p.sessions]
      sessionsTotal.value = p.total
      loadedOffset = replace ? p.sessions.length : loadedOffset + p.sessions.length
    } catch (e) {
      console.error('[Agent Hub] list-usage-sessions failed', e)
    } finally {
      loadingSessions.value = false
    }
  }

  /** 按当前查询条件重载第 1 页（扫描完成回流 / 查询按钮 / 重置） */
  async function reloadSessions() {
    page.value = 1
    loadedOffset = 0
    await fetchSessionsPage(0, true)
  }

  /** 日志页页导航（替换列表为该页切片） */
  async function goPage(p: number) {
    const target = Math.min(Math.max(1, p), totalPages.value)
    if (target === page.value && sessions.value.length > 0) return
    page.value = target
    await fetchSessionsPage((target - 1) * PAGE_SIZE, true)
  }

  /** 列表适配器过滤切换：重设条件并回到第 1 页 */
  async function setListFilter(adapter: string) {
    if (listFilter.value === adapter) return
    listFilter.value = adapter
    await reloadSessions()
  }

  /** 统计明细追加加载（保持既有「加载更多」语义） */
  async function loadMoreSessions() {
    if (loadingSessions.value || loadedOffset >= sessionsTotal.value) return
    await fetchSessionsPage(loadedOffset, false)
  }

  /** 重置查询条件并回到第 1 页 */
  async function resetQuery() {
    listFilter.value = ''
    searchText.value = ''
    rangeFrom.value = null
    rangeTo.value = null
    await reloadSessions()
  }

  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-usage-state', {})
      applyState((data?.state ?? null) as UsageDomainState | null)
    } catch (e) {
      console.error('[Agent Hub] get-usage-state failed', e)
    }
  }

  /** 面板打开后自动扫描已触发过（idle 态只扫一次，防事件风暴下重复发起） */
  let autoScanDone = false

  function applyState(next: UsageDomainState | null) {
    const syncing = state.value?.status === 'syncing'
    state.value = next
    // spec §4.5「应用打开面板时增量扫描」：从未扫描（idle）时自动触发一次
    // （auth-required 不自动触发——由用户经授权入口先授权）
    if (next?.status === 'idle' && !autoScanDone) {
      autoScanDone = true
      void scan()
      return
    }
    // 扫描由本会话发起 → 完成后重拉看板与列表首屏（auth-required/error 无需）
    if (syncing && next?.status === 'ok') {
      void Promise.all([reloadStats(), reloadSessions()])
    }
  }

  /** 触发增量扫描（guest 闸门：未授权 → auth-required，同步返回状态） */
  async function scan() {
    try {
      const data = await context.commands.execute('agent-hub.scan-usage', {})
      applyState((data?.state ?? null) as UsageDomainState | null)
    } catch (e) {
      console.error('[Agent Hub] scan-usage failed', e)
    }
  }

  async function reloadStats() {
    try {
      const data = await context.commands.execute('agent-hub.get-usage-stats', {})
      stats.value = (data ?? null) as UsageStats | null
    } catch (e) {
      console.error('[Agent Hub] get-usage-stats failed', e)
    }
  }

  /** 打开会话日志（事件流 + 原始行，单会话一次读盘双消费） */
  async function openSession(id: number) {
    openingSession.value = true
    try {
      const data = await context.commands.execute('agent-hub.read-usage-session', { id })
      openedSession.value = (data ?? null) as UsageSessionDetail | null
    } catch (e) {
      console.error('[Agent Hub] read-usage-session failed', id, e)
      openedSession.value = null
    } finally {
      openingSession.value = false
    }
  }

  function closeSession() {
    openedSession.value = null
  }

  const subscription = context.events.on('plugin:agent-hub:usage', (payload: UsageDomainState) => {
    applyState(payload)
  })

  onMounted(() => {
    void refresh()
    void reloadStats()
    void reloadSessions()
    void reloadSources()
  })

  onUnmounted(() => {
    subscription.dispose()
  })

  return {
    state,
    stats,
    sources,
    sessions,
    sessionsTotal,
    totalPages,
    loadingSessions,
    listFilter,
    searchText,
    rangeFrom,
    rangeTo,
    page,
    openedSession,
    openingSession,
    refresh,
    scan,
    reloadStats,
    reloadSessions,
    setListFilter,
    goPage,
    resetQuery,
    loadMoreSessions,
    reloadSources,
    addSource,
    removeSource,
    openSession,
    closeSession,
  }
}