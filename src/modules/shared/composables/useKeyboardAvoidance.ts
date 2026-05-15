import { ref, onMounted, onUnmounted, watch } from 'vue'

/**
 * 移动端键盘避让 Composable
 *
 * 通过多种方案检测软键盘状态：
 * 1. visualViewport API（主方案，最精确）
 * 2. window.innerHeight 变化对比（备用方案）
 * 3. focus/blur 事件（辅助方案，检测输入框焦点）
 *
 * 多种方案取最大值，确保不会漏掉键盘高度
 */
export function useKeyboardAvoidance() {
  const keyboardHeight = ref(0)
  const isKeyboardOpen = ref(false)
  const initialViewportHeight = ref(0)

  let viewportHandler: (() => void) | null = null
  let resizeHandler: (() => void) | null = null
  let focusHandler: (() => void) | null = null
  let blurHandler: (() => void) | null = null

  // 使用多种方案检测，取最大值
  function updateKeyboardState() {
    const viewportHeight = window.visualViewport?.height ?? window.innerHeight
    const windowHeight = window.innerHeight

    // 方案1: visualViewport 高度变化
    let heightFromViewport = 0
    if (window.visualViewport && initialViewportHeight.value > 0) {
      const diff = initialViewportHeight.value - viewportHeight
      heightFromViewport = diff > 50 ? diff : 0
    }

    // 方案2: window.innerHeight 变化
    let heightFromResize = 0
    if (initialViewportHeight.value > 0) {
      const diff = initialViewportHeight.value - windowHeight
      heightFromResize = diff > 50 ? diff : 0
    }

    // 取两种方案的最大值（更保守，确保不被遮挡）
    const maxHeight = Math.max(heightFromViewport, heightFromResize)

    // 只有高度差超过 50px 才认为是键盘弹出
    if (maxHeight > 50) {
      isKeyboardOpen.value = true
      keyboardHeight.value = maxHeight
    } else {
      // 如果检测不到键盘高度但有输入框聚焦，假设键盘可能已打开
      const activeElement = document.activeElement
      const isInputFocused = activeElement?.tagName === 'INPUT' ||
                            activeElement?.tagName === 'TEXTAREA' ||
                            activeElement?.getAttribute('contenteditable') === 'true'

      if (isInputFocused) {
        // 输入框聚焦但没检测到键盘高度，可能是第三方键盘
        // 使用一个保守的估计值
        keyboardHeight.value = Math.max(maxHeight, 200)
        isKeyboardOpen.value = true
      } else {
        isKeyboardOpen.value = false
        keyboardHeight.value = 0
      }
    }
  }

  // 保存初始视口高度
  function captureInitialHeight() {
    // 延迟获取，确保页面完全加载
    setTimeout(() => {
      if (window.visualViewport) {
        initialViewportHeight.value = window.visualViewport.height
      } else {
        initialViewportHeight.value = window.innerHeight
      }
      updateKeyboardState()
    }, 100)
  }

  onMounted(() => {
    // 保存初始视口高度（键盘未弹出时的状态）
    if (window.visualViewport) {
      initialViewportHeight.value = window.visualViewport.height

      // 监听 visualViewport 变化
      viewportHandler = updateKeyboardState
      window.visualViewport.addEventListener('resize', viewportHandler)

      // 同时监听 window resize 作为备用
      resizeHandler = updateKeyboardState
      window.addEventListener('resize', resizeHandler)
    } else {
      // 降级方案：仅使用 window resize
      initialViewportHeight.value = window.innerHeight
      resizeHandler = updateKeyboardState
      window.addEventListener('resize', resizeHandler)
    }

    // 监听输入框 focus/blur 作为辅助检测
    focusHandler = () => {
      // 输入框聚焦时，稍后检测键盘状态
      setTimeout(updateKeyboardState, 300)
    }
    blurHandler = () => {
      // 输入框失焦时，重置键盘状态
      setTimeout(() => {
        if (!document.activeElement ||
            (document.activeElement.tagName !== 'INPUT' &&
             document.activeElement.tagName !== 'TEXTAREA')) {
          isKeyboardOpen.value = false
          keyboardHeight.value = 0
        }
      }, 200)
    }

    // 使用事件委托监听输入框
    document.addEventListener('focusin', focusHandler)
    document.addEventListener('focusout', blurHandler)

    // 初始化高度
    captureInitialHeight()
  })

  onUnmounted(() => {
    if (viewportHandler && window.visualViewport) {
      window.visualViewport.removeEventListener('resize', viewportHandler)
    }
    if (resizeHandler) {
      window.removeEventListener('resize', resizeHandler)
    }
    if (focusHandler) {
      document.removeEventListener('focusin', focusHandler)
    }
    if (blurHandler) {
      document.removeEventListener('focusout', blurHandler)
    }
  })

  /**
   * 强制刷新键盘状态（用于手动触发的场景）
   */
  function refreshKeyboardState() {
    // 重新获取初始高度
    captureInitialHeight()
  }

  /**
   * 手动设置键盘高度（用于特殊情况）
   */
  function setKeyboardHeight(height: number) {
    keyboardHeight.value = height
    isKeyboardOpen.value = height > 50
  }

  return {
    keyboardHeight,
    isKeyboardOpen,
    initialViewportHeight,
    refreshKeyboardState,
    setKeyboardHeight,
  }
}