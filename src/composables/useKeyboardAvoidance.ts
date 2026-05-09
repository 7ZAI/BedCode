import { ref, onMounted, onUnmounted } from 'vue'

/**
 * 移动端键盘避让 Composable
 *
 * 通过 visualViewport API 检测软键盘的打开/关闭状态，
 * 在键盘弹出时自动调整布局，防止输入框被键盘遮挡。
 */
export function useKeyboardAvoidance() {
  const keyboardHeight = ref(0)
  const isKeyboardOpen = ref(false)

  let initialVisualHeight = 0
  let viewportHandler: (() => void) | null = null

  function updateKeyboardHeight() {
    if (!window.visualViewport) return

    const currentHeight = window.visualViewport.height
    const diff = initialVisualHeight - currentHeight

    // 高度差超过 100px 才认为是键盘弹出
    isKeyboardOpen.value = diff > 100
    keyboardHeight.value = diff > 100 ? diff : 0
  }

  onMounted(() => {
    if (!window.visualViewport) {
      // 降级方案：使用 window resize
      initialVisualHeight = window.innerHeight
      viewportHandler = () => {
        const diff = initialVisualHeight - window.innerHeight
        isKeyboardOpen.value = diff > 100
        keyboardHeight.value = diff > 100 ? diff : 0
      }
      window.addEventListener('resize', viewportHandler)
      return
    }

    initialVisualHeight = window.visualViewport.height
    viewportHandler = updateKeyboardHeight
    window.visualViewport.addEventListener('resize', viewportHandler)
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
   * 将指定元素滚动到可视区域
   * 延迟执行以等待键盘动画完成
   */
  function scrollElementIntoView(el: HTMLElement) {
    setTimeout(() => {
      el.scrollIntoView({ block: 'center', behavior: 'smooth' })
    }, 250)
  }

  return { keyboardHeight, isKeyboardOpen, scrollElementIntoView }
}
