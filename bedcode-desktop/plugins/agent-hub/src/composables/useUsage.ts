/**
 * Agent Hub 使用统计与会话日志域编排（票据 06）
 *
 * - 状态流：挂载时拉取持久化状态（host-storage `usage` 键），guest 每次扫描
 *   完成 emit `plugin:agent-hub:usage` 覆盖本地状态；auth-required 呈现授权
 *   入口（fs_auth 第三层按路径弹窗，guest 侧已闸门拦截，不再触发弹窗）
 * - 看板：stats 一次拉全部分组（按天/CLI/项目/模型），维度切换纯前端；
 *   扫描完成后自动重拉
 * - 会话列表：分页增量加载（offset 递增追加），供统计明细与日志主从共用
 * - 日志视图：openSession 按会话 id 读源文件解析为归一事件流 + 原始行
 *   （与统计共用同一适配器层，单会话一次读盘双消费）
 */
import { onMounted, onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type {
  UsageDomainState,
  UsageSessionDetail,
  UsageSessionPage,
  UsageSessionRow,
  UsageStats,
} from '../types'

export type UseUsageReturn = ReturnType<typeof useUsage>

/** 列表分页步长（与 guest PAGE_SIZE 一致） */
const PAGE_LIMIT = 50

export function useUsage(context: PluginContext) {
  const state = ref<UsageDomainState | null>(null)
  const stats = ref<UsageStats | null>(null)
  const sessions = ref<UsageSessionRow[]>([])
  const sessionsTotal = ref(0)
  /** 列表分页游标（offset 已加载数） */
  let loadedOffset = 0
  /** 列表加载中（防分页重入） */
  const loadingSessions = ref(false)
  /** 当前打开的日志会话（null = 未选中） */
  const openedSession = ref<UsageSessionDetail | null>(null)
  const openingSession = ref(false)

  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-usage-state', {})
      applyState((data?.state ?? null) as UsageDomainState | null)
    } catch (e) {
      console.error('[Agent Hub] get-usage-state failed', e)
    }
  }

  function applyState(next: UsageDomainState | null) {
    const syncing = state.value?.status === 'syncing'
    state.value = next
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

  /** 首屏重置加载 */
  async function reloadSessions(adapterFilter = listFilter.value) {
    loadingSessions.value = true
    try {
      const data = await context.commands.execute('agent-hub.list-usage-sessions', {
        offset: 0,
        limit: PAGE_LIMIT,
        adapter: adapterFilter,
      })
      const page = data as UsageSessionPage
      sessions.value = page.sessions
      sessionsTotal.value = page.total
      loadedOffset = page.sessions.length
    } catch (e) {
      console.error('[Agent Hub] list-usage-sessions failed', e)
    } finally {
      loadingSessions.value = false
    }
  }

  /** 列表适配器过滤（'' = 全部）；切换即重拉 */
  const listFilter = ref('')

  /** 加载更多（分页追加） */
  async function loadMoreSessions() {
    if (loadingSessions.value || loadedOffset >= sessionsTotal.value) return
    loadingSessions.value = true
    try {
      const data = await context.commands.execute('agent-hub.list-usage-sessions', {
        offset: loadedOffset,
        limit: PAGE_LIMIT,
        adapter: listFilter.value,
      })
      const page = data as UsageSessionPage
      sessions.value = [...sessions.value, ...page.sessions]
      loadedOffset += page.sessions.length
      sessionsTotal.value = page.total
    } catch (e) {
      console.error('[Agent Hub] list-usage-sessions loadMore failed', e)
    } finally {
      loadingSessions.value = false
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

  /** 列表适配器过滤切换（'' = 全部）；切换即重拉 */
  async function setListFilter(adapter: string) {
    if (listFilter.value === adapter) return
    listFilter.value = adapter
    await reloadSessions(adapter)
  }

  const subscription = context.events.on('plugin:agent-hub:usage', (payload: UsageDomainState) => {
    applyState(payload)
  })

  onMounted(() => {
    void refresh()
    void reloadStats()
    void reloadSessions()
  })

  onUnmounted(() => {
    subscription.dispose()
  })

  return {
    state,
    stats,
    sessions,
    sessionsTotal,
    loadingSessions,
    listFilter,
    openedSession,
    openingSession,
    refresh,
    scan,
    reloadStats,
    setListFilter,
    loadMoreSessions,
    openSession,
    closeSession,
  }
}
