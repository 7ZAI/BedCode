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

    unlisten = await onPtyOutput((event: any) => {
      if (event.session_id === targetSessionId) {
        output.value += event.data
      }
    })
    console.log('[usePtyOutput] Listener set up for session:', targetSessionId)
  }

  // 监听 sessionId 变化
  watch(sessionIdRef, (newId, oldId) => {
    if (newId !== oldId) {
      console.log('[usePtyOutput] sessionId changed:', oldId, '->', newId)
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