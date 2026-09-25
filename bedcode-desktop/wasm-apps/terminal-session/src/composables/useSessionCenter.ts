/**
 * 会话中心数据与编排（票 13）
 *
 * 取数红线（spec D2）：只经 PluginContext 与插件命令通道——
 * - 会话运行时状态：命令通道 `session.list`（本插件登记域视图；票 08 起宿主
 *   不再有会话数据命令面，**不**经 `context.session.list()`）
 * - 会话配置 CRUD：插件命令通道 `session.config.*`（真源在本插件私有库）
 * - 创建 / 停止：`session.create` / `session.close`（插件 WASM 编排 → host-session 原语）
 * - 终端窗口：`context.session.openTerminal / closeTerminal / isTerminalOpen /
 *   predictTerminalSize`（窗口本体与渲染管线留宿主，spec D3）
 *
 * **禁止**直调宿主领域命令（`list_sessions` / `start_session` 等）——那层门面是
 * 宿主 UI 的兼容接缝，出现即评审退回（插件工程契约测试 C4 强校验）。
 *
 * 本模块只做「取数 + 编排 + 终端窗口生命周期跟随」，不含文案与弹窗：
 * 错误上抛由视图层决定用户可见提示（错误文案一律 i18n，禁硬编码）。
 */
import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

/** 会话配置（插件私有库真源行，camelCase 与宿主 DTO 逐字同形） */
export interface SessionConfigDto {
  id: string
  name: string
  environment: string
  wslDistro?: string | null
  workingDir: string
  command: string
  autoStart: boolean
}

/** 会话运行时记录（宿主 `SessionInfo` 对外视图子集，camelCase） */
export interface SessionDto {
  id: string
  configId: string
  name: string
  /** 'idle' | 'starting' | 'running' | 'waitingInput' | 'stopping' | 'stopped' | 'error' */
  status: string
  createdAt?: string
  startedAt?: string | null
  stoppedAt?: string | null
}

/** 配置写入草稿（`session.config.upsert` 入参；`id` 缺省 = 新建） */
export interface ConfigDraft {
  id?: string
  name: string
  environment: string
  wslDistro?: string
  workingDir: string
  command: string
  autoStart?: boolean
}

/** 运行中判定：只有 stopped / error 不算运行中（与宿主 `SessionsFallbackView` 同口径） */
export function isRunningStatus(status: string): boolean {
  return status !== 'stopped' && status !== 'error'
}

/** 终端窗口可查看的判定（宿主会话页原语义：仅 running / waitingInput 可看） */
export function isViewableStatus(status: string): boolean {
  return status === 'running' || status === 'waitingInput'
}

export function useSessionCenter(context: PluginContext) {
  const configs = ref<SessionConfigDto[]>([])
  const sessions = ref<SessionDto[]>([])
  const isLoading = ref(true)
  /** 每秒刷新一次，用于运行时长显示 */
  const now = ref(Date.now())

  const runningSessions = computed<SessionDto[]>(() =>
    sessions.value.filter((s) => isRunningStatus(s.status)),
  )

  // ==================== 取数 ====================

  /** 配置列表：插件私有库真源（命令通道） */
  async function loadConfigs(): Promise<void> {
    const raw = await context.commands.execute('session.config.list', {})
    configs.value = Array.isArray(raw) ? (raw as SessionConfigDto[]) : []
  }

  /** 会话列表：本插件登记域真源（命令通道 → `session-list` 视图面） */
  async function loadSessions(): Promise<void> {
    const raw = (await context.commands.execute('session.list', {})) as
      | { sessions?: SessionDto[] }
      | null
    sessions.value = Array.isArray(raw?.sessions) ? raw!.sessions! : []
  }

  /** 全量加载（配置 + 会话）：任一失败上抛，由调用方提示 */
  async function load(): Promise<void> {
    await Promise.all([loadConfigs(), loadSessions()])
  }

  // ==================== 会话编排 ====================

  /**
   * 启动会话：先预测宿主终端窗口网格（PTY 以正确行列 openpty，避免开窗后重排），
   * 再经插件命令通道 `session.create`（命名唯一化 / config→launch 映射 / 两阶段
   * 决策在本插件 WASM 完成）创建并启动，最后刷新列表。
   */
  async function startConfig(configId: string): Promise<void> {
    const size = await context.session.predictTerminalSize()
    await context.commands.execute('session.create', {
      configId,
      cols: size?.cols,
      rows: size?.rows,
      start: true,
    })
    await loadSessions()
  }

  /**
   * 打开（或聚焦）终端窗口。
   *
   * @returns `true` = 新建窗口（调用方显示就绪 loading）；`false` = 未运行（调用方提示）
   */
  async function viewTerminal(session: SessionDto): Promise<boolean> {
    if (!isViewableStatus(session.status)) return false
    await context.session.openTerminal({ id: session.id, name: session.name })
    return true
  }

  /** 停止会话（保留记录）：`session.close` 由插件 WASM 转 host-session 原语 */
  async function stopSession(sessionId: string): Promise<void> {
    await context.commands.execute('session.close', { sessionId })
    await context.session.closeTerminal(sessionId)
    await loadSessions()
  }

  /** 移除会话（连同记录）：`session.action.remove` */
  async function removeSession(sessionId: string): Promise<void> {
    await context.commands.execute('session.action.remove', { sessionId })
    await context.session.closeTerminal(sessionId)
    await loadSessions()
  }

  /** 重启会话（同一 id 重建并启动） */
  async function restartSession(sessionId: string): Promise<void> {
    await context.commands.execute('session.action.restart', { sessionId })
    await loadSessions()
  }

  // ==================== 配置编排 ====================

  /** 写入配置（`id` 命中即覆盖，未命中显性报错；缺省 id 新建） */
  async function saveConfig(draft: ConfigDraft): Promise<void> {
    await context.commands.execute('session.config.upsert', draft)
    await loadConfigs()
  }

  /** 删除配置 */
  async function removeConfig(id: string): Promise<void> {
    await context.commands.execute('session.config.delete', { id })
    await loadConfigs()
  }

  // ==================== 终端窗口生命周期跟随 ====================

  /** 该会话是否已有打开的终端窗口（宿主登记事实；调用方据此决定是否显示 loading） */
  function isTerminalOpen(sessionId: string): boolean {
    return context.session.isTerminalOpen(sessionId)
  }

  // 会话停止 / 出错 / 被移除 → 关闭其终端窗口（宿主会话页原语义）：
  // 窗口本体留宿主，但「何时该关」是会话编排的一部分，故由本模块跟随
  watch(
    sessions,
    (next, prev) => {
      if (!prev) return
      for (const oldSession of prev) {
        const current = next.find((s) => s.id === oldSession.id)
        const wasRunning = isViewableStatus(oldSession.status)
        const nowGone = !current
        const nowDead = current !== undefined && !isRunningStatus(current.status)
        if ((wasRunning && nowDead) || nowGone) {
          void context.session.closeTerminal(oldSession.id)
        }
      }
    },
    { deep: true },
  )

  // ==================== 运行时长计时 ====================

  let timer: ReturnType<typeof setInterval> | null = null

  function startTicker(): void {
    if (timer !== null) return
    timer = setInterval(() => {
      now.value = Date.now()
    }, 1000)
  }

  function stopTicker(): void {
    if (timer !== null) {
      clearInterval(timer)
      timer = null
    }
  }

  return {
    configs: configs as Ref<SessionConfigDto[]>,
    sessions: sessions as Ref<SessionDto[]>,
    runningSessions: runningSessions as ComputedRef<SessionDto[]>,
    isLoading,
    now,
    load,
    loadConfigs,
    loadSessions,
    startConfig,
    viewTerminal,
    stopSession,
    removeSession,
    restartSession,
    saveConfig,
    removeConfig,
    isTerminalOpen,
    startTicker,
    stopTicker,
  }
}

export type SessionCenter = ReturnType<typeof useSessionCenter>
