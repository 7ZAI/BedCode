/**
 * PTY 输出监听 Composable
 *
 * 使用增量回调模式：每次 PTY 输出事件直接通过回调传递解码后的数据，
 * 不在 ref 中累积完整输出，避免长时间运行后内存无限增长导致页面崩溃
 */

import { onUnmounted, type Ref, computed, watch } from 'vue'
import { onPtyOutput } from '@/composables/useDesktopCommands'

export function usePtyOutput(
  sessionId: string | Ref<string>,
  onData: (data: string, index: number) => void,
) {
  let unlisten: (() => void) | null = null

  // 支持传入字符串或 Ref/Computed
  const sessionIdRef = computed(() => {
    if (typeof sessionId === 'string') return sessionId
    return sessionId.value
  })

  // 建立监听器的核心函数
  async function setupListener(targetSessionId: string) {
    // 清理旧的监听器
    if (unlisten) {
      unlisten()
      unlisten = null
    }

    // 如果 sessionId 为空，不建立监听
    if (!targetSessionId) {
      return
    }

    // 按 session 分 channel 监听，无需前端过滤
    unlisten = await onPtyOutput(targetSessionId, (event: any) => {
      try {
        // atob() 解码后是 Latin-1 编码，需要转换为 UTF-8
        const binaryString = atob(event.data)
        const bytes = new Uint8Array(binaryString.length)
        for (let i = 0; i < binaryString.length; i++) {
          bytes[i] = binaryString.charCodeAt(i)
        }
        const decodedData = new TextDecoder('utf-8', { fatal: false }).decode(bytes)
        onData(decodedData, event.index ?? 0)
      } catch (e) {
        console.error('[usePtyOutput] Failed to decode base64:', e)
        // 解码失败时使用原始数据
        onData(event.data, event.index ?? 0)
      }
    })
  }

  // 监听 sessionId 变化
  watch(sessionIdRef, (newId, oldId) => {
    if (newId !== oldId) {
      setupListener(newId)
    }
  }, { immediate: true })

  onUnmounted(() => {
    if (unlisten) {
      unlisten()
    }
  })
}
