/**
 * Agent Hub 使用统计与会话日志域编排（票据 06 + 日志页改版）
 *
 * - 状态流：挂载时拉取持久化状态（host-storage `usage` 键），guest 每次扫描
 *   完成 emit `plugin:agent-hub:usage` 覆盖本地状态；auth-required 呈现授权
 *   入口（fs_auth 第三层按路径弹窗，guest 侧已闸门拦截，不再触发弹窗）
 * - 看板：stats 一次拉全部分组（按天/CLI/项目/模型），维度切换纯前端；
 *   扫描完成后自动重拉
 * - 会话列表：**共享查询条件 + 两份独立列表**。统计明细是「追加加载」、
 *   日志表格是「按页替换」，两者游标语义相反，因此
 *   `statSessions/statTotal/statLoaded` 与 `logSessions/logTotal/logPage`
 *   各自持有；`listFilter/searchText/rangeFrom/rangeTo` 由两个 tab 共享，
 *   任一变更都让两份列表同时回到第一页（此前单一 `sessions/page/loadedOffset`
 *   会在两个 tab 间串味：统计加载更多后翻日志页再回来即出现重复行，
 *   日志页则显示 45 行却写着「第 1 页」）
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
  /** 列表加载中（防重入；两份列表各自一个，互不阻塞） */
  const statLoading = ref(false)
  const logLoading = ref(false)
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
      // 票 04（ADR 0030）：详情只进日志，返回值仅供界面触发友好 i18n（不携带原文）
      console.error('[Agent Hub] add-usage-source failed:', e)
      return { ok: false }
    }
  }

  async function removeSource(name: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.remove-usage-source', { name })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      console.error('[Agent Hub] remove-usage-source failed:', e)
      return { ok: false }
    }
  }

  // ==================== 会话列表（共享查询条件 + 两份独立列表） ====================
  /** 适配器过滤（'' = 全部）——两个 tab 共享 */
  const listFilter = ref('')
  /** 关键词（标题 / 项目 / 会话 id 模糊匹配，空 = 不限定）——两个 tab 共享 */
  const searchText = ref('')
  /** 时间范围（epoch ms，null = 不限定）——两个 tab 共享 */
  const rangeFrom = ref<number | null>(null)
  const rangeTo = ref<number | null>(null)

  // ---- 统计分区私有：追加加载语义 ----
  const statSessions = ref<UsageSessionRow[]>([])
  const statTotal = ref(0)
  /** 已追加加载的条数（= 下一页的 offset） */
  const statLoaded = ref(0)

  // ---- 日志分区私有：按页替换语义 ----
  const logSessions = ref<UsageSessionRow[]>([])
  const logTotal = ref(0)
  const logPage = ref(1)

  const logTotalPages = computed(() => Math.max(1, Math.ceil(logTotal.value / PAGE_SIZE)))

  function buildQuery() {
    return {
      adapter: listFilter.value,
      q: searchText.value.trim(),
      from: rangeFrom.value,
      to: rangeTo.value,
    }
  }

  /** 拉取一页原始数据（offset/limit 固定由调用方给） */
  async function fetchPage(offset: number): Promise<UsageSessionPage | null> {
    const data = await context.commands.execute('agent-hub.list-usage-sessions', {
      offset,
      limit: PAGE_SIZE,
      ...buildQuery(),
    })
    return data as UsageSessionPage
  }

  /** 统计明细：把一页追加到已有列表之后（游标只在本列表内推进） */
  async function loadStatPage(replace: boolean) {
    if (statLoading.value) return
    statLoading.value = true
    try {
      const offset = replace ? 0 : statLoaded.value
      const p = await fetchPage(offset)
      if (p === null) return
      const rows = p.sessions ?? []
      statSessions.value = replace ? rows : [...statSessions.value, ...rows]
      statTotal.value = p.total
      statLoaded.value = replace ? rows.length : statLoaded.value + rows.length
    } catch (e) {
      console.error('[Agent Hub] list-usage-sessions failed (stats)', e)
    } finally {
      statLoading.value = false
    }
  }

  /** 日志表格：把某页替换进列表（游标只在本列表内推进） */
  async function loadLogPage(page: number) {
    if (logLoading.value) return
    logLoading.value = true
    try {
      const p = await fetchPage((page - 1) * PAGE_SIZE)
      if (p === null) return
      logSessions.value = p.sessions ?? []
      logTotal.value = p.total
      logPage.value = page
    } catch (e) {
      console.error('[Agent Hub] list-usage-sessions failed (logs)', e)
    } finally {
      logLoading.value = false
    }
  }

  /** 共享条件或扫描结果变化 → 两份列表同时回第一页 */
  async function reloadSessions() {
    await Promise.all([loadStatPage(true), loadLogPage(1)])
  }

  /** 日志页页导航（只动日志列表） */
  async function goPage(p: number) {
    const target = Math.min(Math.max(1, p), logTotalPages.value)
    if (target === logPage.value && logSessions.value.length > 0) return
    await loadLogPage(target)
  }

  /** 列表适配器过滤切换：重设共享条件并让两份列表都回第 1 页 */
  async function setListFilter(adapter: string) {
    if (listFilter.value === adapter) return
    listFilter.value = adapter
    await reloadSessions()
  }

  /** 统计明细追加加载（保持既有「加载更多」语义，游标不与日志页串味） */
  async function loadMoreSessions() {
    if (statLoading.value || statLoaded.value >= statTotal.value) return
    await loadStatPage(false)
  }

  /** 重置共享查询条件并让两份列表都回第 1 页 */
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

  /**
   * 自动扫描是否已**成功发起**过：只锁「已发起」，发起失败即回滚，
   * 使下次 idle 回流（面板重开 / refresh）能重试。
   * 标志只由 scan() 持有，applyState 只读取。
   */
  let autoScanDone = false

  function applyState(next: UsageDomainState | null) {
    const syncing = state.value?.status === 'syncing'
    state.value = next
    // spec §4.5「应用打开面板时增量扫描」：从未扫描（idle）时自动触发一次
    // （auth-required 不自动触发——由用户经授权入口先授权）
    if (next?.status === 'idle' && !autoScanDone) {
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
    // 手动/重试扫描允许再次自动触发：autoScanDone 只锁「已成功发起的自动扫描」
    autoScanDone = true
    try {
      const data = await context.commands.execute('agent-hub.scan-usage', {})
      applyState((data?.state ?? null) as UsageDomainState | null)
    } catch (e) {
      // 发起失败 → 释放自动扫描锁，下次 idle 回流可重试
      autoScanDone = false
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
    // 共享查询条件
    listFilter,
    searchText,
    rangeFrom,
    rangeTo,
    // 统计分区（追加加载）
    statSessions,
    statTotal,
    statLoaded,
    statLoading,
    // 日志分区（按页替换）
    logSessions,
    logTotal,
    logPage,
    logTotalPages,
    logLoading,
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