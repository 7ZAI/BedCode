import { ref, onMounted, onUnmounted } from 'vue'

/**
 * 移动端键盘避让 Composable
 *
 * 通过 visualViewport API 检测软键盘的打开/关闭状态和高度，
 * 在键盘弹出时自动调整布局，防止输入框被键盘遮挡。
 */
export function useKeyboardAvoidance() {
  const keyboardHeight = ref(0)
  const isKeyboardOpen = ref(false)
  const initialViewportHeight = ref(0)

  let viewportHandler: (() => void) | null = null

  function updateKeyboardState() {
    if (!window.visualViewport) {
      // 降级方案：使用 window resize
      const diff = initialViewportHeight.value - window.innerHeight
      isKeyboardOpen.value = diff > 100
      keyboardHeight.value = diff > 100 ? diff : 0
      return
    }

    const currentHeight = window.visualViewport.height
    const initialHeight = initialViewportHeight.value

    // 计算键盘高度（viewport 高度变化）
    const heightDiff = initialHeight - currentHeight

    // 只有高度差超过 100px 才认为是键盘弹出（排除其他因素如浏览器工具栏）
    if (heightDiff > 100) {
      isKeyboardOpen.value = true
      keyboardHeight.value = heightDiff
    } else {
      isKeyboardOpen.value = false
      keyboardHeight.value = 0
    }
  }

  onMounted(() => {
    // 保存初始视口高度（键盘未弹出时的状态）
    if (window.visualViewport) {
      initialViewportHeight.value = window.visualViewport.height

      viewportHandler = updateKeyboardState
      window.visualViewport.addEventListener('resize', viewportHandler)
    } else {
      // 降级方案
      initialViewportHeight.value = window.innerHeight
      viewportHandler = updateKeyboardState
      window.addEventListener('resize', viewportHandler)
    }
  })

  onUnmounted(() => {
    if (!viewportHandler) return

    if (window.visualViewport) {
      window.visualViewport.removeEventListener('resize', viewportHandler)
    } else {
      window.removeEventListener('resize', viewportHandler)
    }
  })

  /**
   * 强制刷新键盘状态（用于手动触发的场景）
   */
  function refreshKeyboardState() {
    if (window.visualViewport) {
      // 重新获取初始高度
      initialViewportHeight.value = window.visualViewport.height
    }
    updateKeyboardState()
  }

  return {
    keyboardHeight,
    isKeyboardOpen,
    initialViewportHeight,
    refreshKeyboardState,
  }
}