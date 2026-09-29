/**
 * Agent Hub 使用统计与会话日志域编排（票据 06 + 看板改版）
 *
 * - 状态流：挂载时拉取持久化状态（host-storage `usage` 键），guest 每次扫描
 *   完成 emit `plugin:agent-hub:usage` 覆盖本地状态；auth-required 呈现授权
 *   入口。授权口径：扫描命令发起时 guest 先对**本次扫描的实际来源根目录**
 *   批量 request-auth（一次弹窗列全部，已授权路径静默跳过）——同一业务预见
 *   多个文件访问用批量授权代替逐个弹窗；拒绝才整体降级 auth-required
 * - 看板：stats 一次拉回「汇总 + 全部维度分组 + 7×24 节奏矩阵」，指标 /
 *   维度切换纯前端；时间窗（`statsDays`）走 guest 参数，服务端切片
 * - **会话列表只有一份**（日志分区独有）：此前统计分区挂了一份同源的
 *   「会话明细」列表（追加加载语义），与日志表格（按页替换）重复且游标
 *   语义相反，两个 tab 还要共享一套筛选条件互相干扰。现已删除统计侧列表，
 *   `listFilter/searchText/rangeFrom/rangeTo` 只服务日志表格
 * - 日志来源：内置只读 + 自定义增删（list/add/remove-usage-source），
 *   扫描计数合并进 sources 列表。opencode 是 SQLite 源（`kind: 'sqlite'`）：
 *   guest 侧经 host-process 同步查库，不在 JSONL 枚举里，也不提供增删
 * - 日志视图：openSession 按会话 id 读源文件解析为归一事件流 + 原始行
 *   （单会话一次读盘双消费）；opencode 会话改为现查 message/part 联表
 *   （无「原始行」视图）
 * - 数据清空（票 07）：clearData 清会话聚合 + 解析水位（guest 侧同事务），
 *   保留策略为全量保留不自动过期，清空只由用户显式触发
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { resolvePluginErrorText } from '../utils/pluginError'
import type {
  AdapterErrorCode,
  CliId,
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

/**
 * 扫描在途对账间隔（2026-09-28 实机 bug）
 *
 * 实机：扫描发起后回调丢失，状态永远停在 `syncing` → 按钮卡「扫描中…」且禁用，
 * 用户看不到失败也点不动重试（guest 侧重入闸门还会拒绝再次发起）。
 * 真源仍在 guest；本看门狗只做**对账**：在途期间定期回拉状态，覆盖
 * 「事件丢失 / guest 卡死」两种情形，guest 侧的失活守卫（无进展超 2 分钟落
 * error）则把回拉到的真实终态带回来。
 */
export const SCAN_WATCHDOG_INTERVAL_MS = 15_000
/**
 * 对账次数上限（12 × 15s ≈ 3 分钟）
 *
 * **必须长于 guest 侧失活窗口**（`usage::SCAN_STALE_MS` = 2×枚举超时 = 2 分钟）：
 * 停表早了，guest 就算已把失活扫描落 error，前端也永远收不到那一帧，界面就
 * 又变回“永不退出的扫描中”。停表只防面板长开时的无限轮询。
 */
export const SCAN_WATCHDOG_MAX_TICKS = 12

/**
 * 看板时间窗（天）：0 = 全部
 *
 * 前端只提供这四档（与看板 pills 一一对应）；guest 侧另有清洗
 * （负数 / 超上限回落全量），前端不做静默夹取。
 */
export type StatsDays = 0 | 7 | 30 | 90

/**
 * CLI 的会话数据状态（票 07：codex「已装 · 未初始化」的信号源）
 *
 * - `scanned`：已扫描且采到会话 → 徽章显示常规「已装」
 * - `empty`：已扫描但**零会话** → 「已装 · 未初始化」（本机 codex 正是此态：
 *   `~/.codex/` 存在但从未跑过会话）
 * - `unknown`：未扫描 / 未授权 / 扫描中 / 出错 → **不下结论**。这三态必须与
 *   `empty` 严格区分：把「没数据可看」说成「装了没初始化」是误报。
 */
export type CliSessionState = 'scanned' | 'empty' | 'unknown'

export function useUsage(context: PluginContext) {
  const state = ref<UsageDomainState | null>(null)
  const stats = ref<UsageStats | null>(null)
  /** 看板时间窗（默认 30 天：近况是看板的主要读数诉求，「全部」在 pills 里显式选） */
  const statsDays = ref<StatsDays>(30)
  /** 看板加载中（任一请求在飞即为 true；响应按 statsSeq 丢弃过期） */
  const statsLoading = ref(false)
  /** 看板响应序号（只认最后一次，防旧窗响应覆盖新窗） */
  let statsSeq = 0
  /** 日志列表加载中（防重入） */
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

  /** 添加目录：返回 { ok, error? }（guest 校验失败经命令拒绝 → 友好文案，不携带原文） */
  async function addSource(name: string, path: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.add-usage-source', { name, path })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      // ADR 0030：详情只进日志，返回值仅供界面显示友好 i18n（guest 业务码优先）
      console.error('[Agent Hub] add-usage-source failed:', e)
      return { ok: false, error: resolvePluginErrorText(context, e, 'hub.lg.sources.addFailed') }
    }
  }

  /**
   * 系统文件夹选择器选日志目录（fs:pick 权限门与选中路径授权校验在宿主）。
   * 返回 { ok, picked, path, error? }：picked=false 表示用户取消；
   * 授权拒绝等失败 ok=false（error 为友好 i18n 文案）。
   */
  async function pickSourceDir(): Promise<{
    ok: boolean
    picked: boolean
    path: string
    error?: string
  }> {
    try {
      const data = (await context.commands.execute('agent-hub.pick-source-dir', {})) as {
        picked?: boolean
        path?: string
      } | null
      return { ok: true, picked: data?.picked === true, path: data?.path ?? '' }
    } catch (e) {
      // ADR 0030：详情只进日志，界面触发友好 i18n（guest 业务码优先）
      console.error('[Agent Hub] pick-source-dir failed:', e)
      return { ok: false, picked: false, path: '', error: resolvePluginErrorText(context, e, 'hub.lg.sources.pickFailed') }
    }
  }

  async function removeSource(name: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.remove-usage-source', { name })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      // ADR 0030：详情只进日志，界面触发友好 i18n（guest 业务码优先）
      console.error('[Agent Hub] remove-usage-source failed:', e)
      return { ok: false, error: resolvePluginErrorText(context, e, 'hub.lg.sources.removeFailed') }
    }
  }

  /** 给既有来源追加一个目录（内置来源也可追加；sqlite 源拒绝） */
  async function addSourcePath(
    name: string,
    path: string,
  ): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.add-usage-source-path', { name, path })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      // ADR 0030：详情只进日志，界面触发友好 i18n（guest 业务码优先）
      console.error('[Agent Hub] add-usage-source-path failed:', e)
      return {
        ok: false,
        error: resolvePluginErrorText(context, e, 'hub.lg.sources.addPathFailed'),
      }
    }
  }

  /** 从来源移除一个目录（内置默认路径 / 自定义来源最后一条路径由 guest 拒绝） */
  async function removeSourcePath(
    name: string,
    path: string,
  ): Promise<{ ok: boolean; error?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.remove-usage-source-path', {
        name,
        path,
      })
      applyState((data as { state: UsageDomainState })?.state ?? null)
      await reloadSources()
      return { ok: true }
    } catch (e) {
      // ADR 0030：详情只进日志，界面触发友好 i18n（guest 业务码优先）
      console.error('[Agent Hub] remove-usage-source-path failed:', e)
      return {
        ok: false,
        error: resolvePluginErrorText(context, e, 'hub.lg.sources.removePathFailed'),
      }
    }
  }

  // ==================== 会话列表（日志分区独有） ====================
  /** 适配器过滤（'' = 全部） */
  const listFilter = ref('')
  /** 关键词（标题 / 项目 / 会话 id 模糊匹配，空 = 不限定） */
  const searchText = ref('')
  /** 时间范围（epoch ms，null = 不限定） */
  const rangeFrom = ref<number | null>(null)
  const rangeTo = ref<number | null>(null)

  // ---- 日志分区：按页替换语义 ----
  const logSessions = ref<UsageSessionRow[]>([])
  const logTotal = ref(0)
  const logPage = ref(1)

  const logTotalPages = computed(() => Math.max(1, Math.ceil(logTotal.value / PAGE_SIZE)))

  // ==================== 适配器降级（票 07）+ 会话数据状态 ====================

  /**
   * 处于降级态的适配器（code → 需 i18n 呈现，不透出 guest 原文）
   *
   * 只收 `adapters` 里的条目；`error` 为 null / 未识别的 code 均不收
   * （未知 code 宁可少显示一条，也不用错误文案去渲染它）。
   */
  const adapterErrors = computed<{ adapter: string; code: AdapterErrorCode }[]>(() => {
    const adapters = state.value?.adapters
    if (!adapters) return []
    const known: AdapterErrorCode[] = ['sqlite3-missing', 'db-missing', 'query-failed']
    return Object.entries(adapters)
      .filter((e): e is [string, (typeof adapters)[string]] => !!e[1])
      .map(([adapter, stat]) => ({ adapter, code: stat.error }))
      .filter(
        (e): e is { adapter: string; code: AdapterErrorCode } =>
          e.code !== null && known.includes(e.code),
      )
  })

  /**
   * CLI 会话数据状态（CliCard「已装 · 未初始化」的判据）
   *
   * 只有在**确实扫描完成**（status==='ok' 且已授权）时才给结论——
   * 未授权 / 扫描中 / 扫描失败都返回 `unknown`，避免把「看不到数据」误报成
   * 「装了没初始化」。
   */
  function cliSessionState(cli: CliId): CliSessionState {
    const s = state.value
    if (!s || s.status !== 'ok' || !s.authGranted) return 'unknown'
    const stat = s.adapters?.[cli]
    if (!stat) return 'unknown'
    const has = (stat.files ?? 0) + (stat.parsed ?? 0) + (stat.sessions ?? 0)
    return has > 0 ? 'scanned' : 'empty'
  }

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

  /** 查询条件或扫描结果变化 → 日志列表回第一页 */
  async function reloadSessions() {
    await loadLogPage(1)
  }

  /** 日志页页导航（只动日志列表） */
  async function goPage(p: number) {
    const target = Math.min(Math.max(1, p), logTotalPages.value)
    if (target === logPage.value && logSessions.value.length > 0) return
    await loadLogPage(target)
  }

  /** 重置查询条件并让日志列表回第 1 页 */
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

  // ==================== 扫描在途对账（watchdog） ====================
  // 节奏常量见文件顶部导出（与测试同源，避免两处各写一份数值）。
  let watchdogTimer: ReturnType<typeof setTimeout> | null = null
  let watchdogTicks = 0

  function stopScanWatchdog() {
    if (watchdogTimer !== null) {
      clearTimeout(watchdogTimer)
      watchdogTimer = null
    }
    watchdogTicks = 0
  }

  /** 在途则排下一次对账（幂等：已在表则不重排）；否则停表归零 */
  function ensureScanWatchdog() {
    if (state.value?.status !== 'syncing') {
      stopScanWatchdog()
      return
    }
    if (watchdogTimer !== null) return
    watchdogTimer = setTimeout(() => {
      watchdogTimer = null
      watchdogTicks += 1
      if (state.value?.status !== 'syncing') {
        watchdogTicks = 0
        return
      }
      void refresh().finally(() => {
        if (watchdogTicks < SCAN_WATCHDOG_MAX_TICKS) ensureScanWatchdog()
        else watchdogTicks = 0
      })
    }, SCAN_WATCHDOG_INTERVAL_MS)
  }

  function applyState(next: UsageDomainState | null) {
    const syncing = state.value?.status === 'syncing'
    state.value = next
    // 在途开表、终态停表：让「扫描中…」不可能成为永不退出的假状态
    ensureScanWatchdog()
    // spec §4.5「应用打开面板时增量扫描」：从未扫描（idle）时自动触发一次
    // （auth-required 不自动触发——用户点扫描时 guest 侧会对来源根目录
    // 批量弹窗，拒绝才落 auth-required；也可先经概览授权入口批量授权）
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

  /**
   * 拉取看板聚合（带时间窗）
   *
   * 窗由 `statsDays` 决定（guest 侧再清洗一次非法值）；`window.days` 回显
   * 服务端实际生效的窗，界面据此判定「服务端真的切片了」。
   *
   * **按序号丢弃过期响应**（与探测域 `AgentHubState.seq` 同一手法）：切窗
   * 与扫描回流可能同时在飞，若用「在飞就跳过」防重入，扫描那次旧窗的响应
   * 仍会落地，界面就成了「新 pill 选中 + 旧窗数据」。序号只认最后一次。
   */
  async function reloadStats() {
    const seq = ++statsSeq
    statsLoading.value = true
    try {
      const data = await context.commands.execute('agent-hub.get-usage-stats', {
        days: statsDays.value,
      })
      if (seq !== statsSeq) return
      stats.value = (data ?? null) as UsageStats | null
    } catch (e) {
      if (seq !== statsSeq) return
      console.error('[Agent Hub] get-usage-stats failed', e)
    } finally {
      if (seq === statsSeq) statsLoading.value = false
    }
  }

  /** 切换时间窗并重拉看板（同窗连点不重打 guest） */
  async function setStatsDays(days: StatsDays) {
    if (statsDays.value === days) return
    statsDays.value = days
    await reloadStats()
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

  // ==================== 数据清空（票 07） ====================
  /** 清空中（防重入；清空后列表与看板均需重拉） */
  const clearing = ref(false)

  /**
   * 清空已采集的统计数据（会话聚合 + 解析水位，guest 侧同事务）
   *
   * 成功后一并重拉看板与两份列表首屏——不清会看到「空看板 + 旧列表」的
   * 中间态。保留策略为**全量保留不自动过期**，故这是唯一的清理入口。
   */
  async function clearData(): Promise<{ ok: boolean }> {
    if (clearing.value) return { ok: false }
    clearing.value = true
    try {
      const data = await context.commands.execute('agent-hub.clear-usage-data', {})
      applyState((data?.state ?? null) as UsageDomainState | null)
      await Promise.all([reloadStats(), reloadSessions()])
      return { ok: true }
    } catch (e) {
      // 票 04（ADR 0030）：详情只进日志，界面用 i18n 提示
      console.error('[Agent Hub] clear-usage-data failed:', e)
      return { ok: false }
    } finally {
      clearing.value = false
    }
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
    stopScanWatchdog()
  })

  return {
    state,
    stats,
    statsDays,
    statsLoading,
    sources,
    adapterErrors,
    // 日志分区的查询条件
    listFilter,
    searchText,
    rangeFrom,
    rangeTo,
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
    setStatsDays,
    reloadSessions,
    goPage,
    resetQuery,
    reloadSources,
    addSource,
    addSourcePath,
    removeSourcePath,
    pickSourceDir,
    removeSource,
    openSession,
    closeSession,
    clearData,
    clearing,
    cliSessionState,
  }
}