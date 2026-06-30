/**
 * 终端提示词优化
 *
 * 获取终端当前输入 → 调用 AI 优化 → 弹窗确认 → 填入终端
 */
import { ref } from 'vue'
import type { ApiProvider, CurrentInputEvent } from '../types'
import { chat } from '../services/openaiClient'

const OPTIMIZE_SYSTEM_PROMPT = `你是一个提示词优化专家。请优化以下用户输入的提示词，使其更清晰、更具体、更容易让 AI 理解和执行。
要求：
1. 保持原始意图不变
2. 添加必要的上下文和约束条件
3. 使用更精确的表达方式
4. 只输出优化后的提示词，不要添加任何解释、前缀或引号`

export function usePromptOptimizer(
  getActiveProvider: () => ApiProvider | undefined | Promise<ApiProvider | undefined>,
  sendInput: (sessionId: string, text: string) => Promise<void>,
  eventEmit: (event: string, ...args: any[]) => void,
  eventOn: (pluginId: string, event: string, handler: (...args: any[]) => void) => { dispose(): void },
) {
  const optimizing = ref(false)
  const showDialog = ref(false)
  const originalText = ref('')
  const optimizedText = ref('')
  const errorMessage = ref('')
  let currentSessionId = ''

  /** 获取终端当前输入内容 */
  function getCurrentInput(): Promise<CurrentInputEvent> {
    return new Promise((resolve) => {
      const disposable = eventOn('com.bedcode.ai-chatbox', 'ai-chatbox:currentInput', (data: any) => {
        disposable.dispose()
        resolve(data as CurrentInputEvent)
      })
      // 请求宿主组件返回当前输入
      eventEmit('ai-chatbox:getCurrentInput')
      // 超时保护
      setTimeout(() => {
        disposable.dispose()
        resolve({ sessionId: '', text: '' })
      }, 3000)
    })
  }

  /** 触发优化流程 */
  async function optimizePrompt(): Promise<void> {
    const provider = await getActiveProvider()
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
      const result = await chat(provider, [
        { role: 'system', content: OPTIMIZE_SYSTEM_PROMPT, timestamp: new Date().toISOString() },
        { role: 'user', content: input.text, timestamp: new Date().toISOString() },
      ])
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
    await sendInput(currentSessionId, '\x15' + optimizedText.value)
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
