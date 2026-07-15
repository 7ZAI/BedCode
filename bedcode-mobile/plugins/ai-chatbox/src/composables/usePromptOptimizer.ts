/**
 * 终端提示词优化 (Mobile)
 *
 * 获取终端当前输入 → 调用 Rust 后端 AI 优化 → 弹窗确认 → 填入终端
 * 通过 PluginContext.commands 调用 Rust 后端命令
 * 通过 PluginContext.terminal API 获取/写入终端输入
 */
import { ref } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

export function usePromptOptimizer(context: PluginContext) {
  const optimizing = ref(false)
  const showDialog = ref(false)
  const originalText = ref('')
  const optimizedText = ref('')
  const errorMessage = ref('')
  let currentSessionId = ''

  // 监听终端工具栏按钮触发的事件（由 index.ts 注册按钮时 emit）
  context.events.on('ai-chatbox:triggerOptimize', () => {
    optimizePrompt()
  })

  /** 触发优化流程 */
  async function optimizePrompt(): Promise<void> {
    // 从 storage 读取当前活跃 provider
    const providersStr = await context.storage.get<string>('apiProviders')
    const activeName = await context.storage.get<string>('activeProvider')
    let provider: any
    if (providersStr && activeName) {
      try {
        const parsed = typeof providersStr === 'string' ? JSON.parse(providersStr) : providersStr
        const list = Array.isArray(parsed) ? parsed : []
        provider = list.find((p: any) => p.name === activeName)
      } catch { /* ignore */ }
    }

    if (!provider) {
      errorMessage.value = 'desktop.plugin.aiChatbox.pleaseConfigure'
      showDialog.value = true
      return
    }

    // 获取当前活跃会话
    let sessionId = ''
    try {
      const sessions = await context.session.list()
      const activeSession = sessions.find((s: any) => s.status === 'running')
      if (activeSession) {
        sessionId = activeSession.id
      }
    } catch { /* ignore */ }

    if (!sessionId) {
      errorMessage.value = 'desktop.plugin.aiChatbox.noActiveSession'
      showDialog.value = true
      return
    }

    currentSessionId = sessionId
    originalText.value = ''
    errorMessage.value = ''
    optimizing.value = true
    showDialog.value = true
    optimizedText.value = ''

    try {
      // 调用 Rust 后端优化命令
      const result = await context.commands.execute('ai-chatbox.optimize-prompt', {
        provider,
        prompt: originalText.value || '请优化以下终端输入',
      })
      optimizedText.value = result
    } catch (e: any) {
      errorMessage.value = e.message || 'desktop.plugin.aiChatbox.optimizeFailed'
    } finally {
      optimizing.value = false
    }
  }

  /** 采纳优化结果并填入终端 */
  async function acceptOptimized(): Promise<void> {
    if (!currentSessionId || !optimizedText.value) return
    // \x15 = Ctrl+U 清除当前行，然后填入优化后的文本
    await context.terminal.sendInput(currentSessionId, '\x15' + optimizedText.value)
    showDialog.value = false
  }

  /** 取消 */
  function cancelOptimize(): void {
    showDialog.value = false
    originalText.value = ''
    optimizedText.value = ''
    errorMessage.value = ''
  }

  return {
    optimizing,
    showDialog,
    originalText,
    optimizedText,
    errorMessage,
    optimizePrompt,
    acceptOptimized,
    cancelOptimize,
  }
}
