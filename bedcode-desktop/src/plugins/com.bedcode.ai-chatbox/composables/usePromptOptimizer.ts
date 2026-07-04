/**
 * 终端提示词优化
 *
 * 获取终端当前输入 → 调用 Rust 后端 AI 优化 → 弹窗确认 → 填入终端
 * 通过 PluginContext.commands 调用 Rust 后端命令，不再直接调用 openaiClient
 */
import { ref } from 'vue'
import type { CurrentInputEvent } from '../types'
import type { PluginContext } from '../../../plugin/types'

export function usePromptOptimizer(context: PluginContext) {
  const optimizing = ref(false)
  const showDialog = ref(false)
  const originalText = ref('')
  const optimizedText = ref('')
  const errorMessage = ref('')
  let currentSessionId = ''

  /** 获取终端当前输入内容 */
  function getCurrentInput(): Promise<CurrentInputEvent> {
    return new Promise((resolve) => {
      const disposable = context.events.on('ai-chatbox:currentInput', (data: any) => {
        disposable.dispose()
        resolve(data as CurrentInputEvent)
      })
      // 请求宿主组件返回当前输入
      context.events.emit('ai-chatbox:getCurrentInput')
      // 超时保护
      setTimeout(() => {
        disposable.dispose()
        resolve({ sessionId: '', text: '' })
      }, 3000)
    })
  }

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
      errorMessage.value = '请先配置 AI 模型'
      showDialog.value = true
      return
    }

    // 获取当前终端输入
    const input = await getCurrentInput()
    if (!input.text) {
      errorMessage.value = '终端无输入内容'
      showDialog.value = true
      return
    }

    currentSessionId = input.sessionId
    originalText.value = input.text
    errorMessage.value = ''
    optimizing.value = true
    showDialog.value = true
    optimizedText.value = ''

    try {
      // 调用 Rust 后端优化命令
      const result = await context.commands.execute('ai-chatbox.optimize-prompt', {
        provider,
        prompt: input.text,
      })
      optimizedText.value = result
    } catch (e: any) {
      errorMessage.value = e.message || '优化失败'
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
