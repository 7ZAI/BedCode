import { ref, onUnmounted, type Ref, computed, watch } from 'vue'
import { onPtyOutput } from '@/modules/desktop/composables/useDesktopCommands'

export function usePtyOutput(sessionId: string | Ref<string>) {
  const output = ref<string>('')
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
      output.value = ''
      return
    }

    // 建立实时监听
    unlisten = await onPtyOutput((event: any) => {
      if (event.sessionId === targetSessionId) {
        // 解码 base64 数据后再追加
        try {
          // atob() 解码后是 Latin-1 编码，需要转换为 UTF-8
          const binaryString = atob(event.data)
          const bytes = new Uint8Array(binaryString.length)
          for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i)
          }
          const decodedData = new TextDecoder('utf-8', { fatal: false }).decode(bytes)
          output.value += decodedData
        } catch (e) {
          console.error('[usePtyOutput] Failed to decode base64:', e);
          // 如果解码失败，直接使用原始数据（可能是旧数据格式）
          output.value += event.data
        }
      }
    })
  }

  // 监听 sessionId 变化
  watch(sessionIdRef, (newId, oldId) => {
    if (newId !== oldId) {
      setupListener(newId)
    }
  }, { immediate: true })

  function clearOutput() {
    output.value = ''
  }

  onUnmounted(() => {
    if (unlisten) {
      unlisten()
    }
  })

  return {
    output,
    clearOutput,
  }
}