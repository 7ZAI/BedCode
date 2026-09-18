/**
 * 终端输入域（TerminalView 拆分产物，移动端特有）
 *
 * 一块职责：**把「用户想送进终端的东西」送达 PTY**，含三条来源与它们的数据准备：
 * - 输入栏：命令文本（submit）/ 带回车执行（execute）/ 特殊键（specialKey）
 * - 预设任务：任务选择器的发送（send）与执行（execute）
 * - 命令面板预设识别：按会话 config_id 反查启动命令，决定面板展示哪套预设
 *
 * 为什么预设识别也在这里：它服务的唯一消费者就是命令面板，与「送命令进终端」
 * 同源；独立成文件只会让调用方多一次跳转。
 *
 * 关键取舍：**不可送达时必须给用户可见反馈**。此前「未连接 / 会话非活跃」是静默
 * no-op，用户感知为「输入没反应」且无线索；这里按原因分别提示并留 warn 日志。
 */
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import { useToast } from '@/composables/useToast'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { isMockSession } from '@/composables/useMockTerminal'
import { executeTask, sendTask } from '@/composables/usePresetTasks'
import type { PresetTask } from '@/composables/model'
import type { TerminalKernelContext } from './terminalKernel'

export interface TerminalInputDeps {
  /**
   * 输入回传（useTerminalBuffer.sendInput）。返回 false = 链路不可送达
   * （未连接 / 会话未运行），由本域统一提示
   */
  sendInput: (sessionId: string, data: string, specialKey?: string) => boolean
  /** 当前会话的 config_id（WS 推送为 configId，HTTP 响应为 config_id，两端兼容） */
  getConfigId: () => string | undefined
}

export function useTerminalInput(ctx: TerminalKernelContext, deps: TerminalInputDeps) {
  const { t } = useI18n()
  const toast = useToast()
  const connection = useMobileConnection()
  const assistStore = useInputAssistantStore()
  const { sendInput } = deps

  /** 侧栏「插入引用」待填入路径：TerminalInputBar 消费后置回 null */
  const pendingRefPath = ref<string | null>(null)

  // ==================== 输入回传 ====================

  /** 输入不可送达的用户可见反馈（区分「未连接」与「会话未运行」） */
  function notifyInputUnavailable() {
    logger.warn(
      `[TerminalView] input dropped (${ctx.getSessionId()}): connected=${ctx.isConnected()}, ` +
        `active=${ctx.isSessionActive()}`,
    )
    toast.error(t(ctx.isConnected() ? 'mobile.connection.connectFailed' : 'mobile.input.disconnected'))
  }

  /** 输入栏提交（不含回车执行） */
  function handleInputSubmit(text: string) {
    if (!ctx.terminalRef.value) return
    const sid = ctx.getSessionId()
    if (isMockSession(sid)) return
    if (ctx.isConnected() && ctx.isSessionActive()) {
      if (!sendInput(sid, text)) toast.error(t('mobile.connection.connectFailed'))
    } else {
      notifyInputUnavailable()
    }
  }

  /** 输入栏执行（命令 + enter） */
  async function handleInputExecute(text: string) {
    if (!ctx.terminalRef.value) return
    const sid = ctx.getSessionId()
    if (isMockSession(sid)) return
    if (ctx.isConnected() && ctx.isSessionActive()) {
      if (!sendInput(sid, text, 'enter')) toast.error(t('mobile.connection.connectFailed'))
    } else {
      notifyInputUnavailable()
    }
  }

  /** 特殊键（按键组合名，由 Rust 侧映射为字节序列） */
  function handleSpecialKey(key: string) {
    const sid = ctx.getSessionId()
    if (isMockSession(sid)) return
    if (ctx.isConnected() && ctx.isSessionActive()) {
      if (!sendInput(sid, '', key)) toast.error(t('mobile.connection.connectFailed'))
    } else {
      notifyInputUnavailable()
    }
  }

  // ==================== 预设任务 ====================

  async function onTaskSend(task: PresetTask) {
    if (!ctx.isConnected() || !ctx.isSessionActive()) {
      toast.error(t('mobile.connection.connectFailed'))
      return
    }
    try {
      await sendTask(task, ctx.getSessionId())
    } catch {
      toast.error(t('mobile.toolbox.sendFailed'))
    }
  }

  async function onTaskExecute(task: PresetTask) {
    if (!ctx.isConnected() || !ctx.isSessionActive()) {
      toast.error(t('mobile.connection.connectFailed'))
      return
    }
    try {
      await executeTask(task, ctx.getSessionId())
    } catch {
      toast.error(t('mobile.toolbox.sendFailed'))
    }
  }

  // ==================== 命令面板预设识别 ====================
  //
  // 预设需要两条数据：会话的 config_id（activeSessions）与配置的启动命令
  // （sessionConfigs）。通知跳转 / 路由恢复等「直接进入终端页」的路径上两者都
  // 可能未就绪（loadActiveSessions 仅 DevicesView/SessionsView 调用），故识别时
  // 按需补齐：会话缺失按 id 拉列表反查，配置缺失现场拉取。
  // 识别为 generic（未识别）时面板仅保留用户自定义命令——这是可接受的降级；
  // 「已发起」标记保证并发触发（watch immediate + 数据到位）只拉一次。

  /** Agent 类型覆盖表是否已加载（全页只需一次） */
  let overridesLoaded = false
  let sessionsFetchStarted = false
  let configsFetchStarted = false

  async function applyAgentPreset() {
    const sessionId = ctx.getSessionId()
    if (isMockSession(sessionId)) return // mock 会话无配置，不加载预设
    if (!overridesLoaded) {
      await assistStore.loadAgentTypeOverrides()
      overridesLoaded = true
    }
    if (!deps.getConfigId() && !sessionsFetchStarted) {
      sessionsFetchStarted = true
      await connection.loadActiveSessions()
    }
    const configId = deps.getConfigId()
    if (!configId) {
      const found = connection.activeSessions.value.find((s) => s.id === sessionId)
      logger.warn(
        '[TerminalView] applyAgentPreset: 会话未就绪（无 config_id）',
        JSON.stringify({
          sessionId,
          activeSessionsCount: connection.activeSessions.value.length,
          foundSession: found,
        }),
      )
      return // 列表拉取失败或会话确实无配置，保留用户自定义命令
    }
    let config = connection.sessionConfigs.value.find((c) => c.id === configId)
    if (!config && !configsFetchStarted) {
      configsFetchStarted = true
      await connection.loadSessionConfigs().catch(() => {})
      config = connection.sessionConfigs.value.find((c) => c.id === configId)
    }
    if (!config) {
      logger.warn('[TerminalView] applyAgentPreset: 配置列表无匹配 config_id，预设不加载', { configId })
      return
    }
    assistStore.setAgentPreset(assistStore.getEffectiveAgentType(configId, config.command))
  }

  // ==================== 侧栏引用填充 ====================

  /** 侧栏「插入引用」：把 @路径 传给输入条填充，并收起侧栏露出输入区 */
  function handleInsertRef(path: string) {
    pendingRefPath.value = path
    // 侧栏收起由调用方（功能栏域）负责：此处只管输入内容
    return path
  }

  return {
    pendingRefPath,
    handleInputSubmit,
    handleInputExecute,
    handleSpecialKey,
    onTaskSend,
    onTaskExecute,
    applyAgentPreset,
    handleInsertRef,
  }
}
