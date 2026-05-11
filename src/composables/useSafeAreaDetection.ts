/**
 * 移动端安全区域检测 Composable
 *
 * 自动检测并计算状态栏和导航栏的高度
 * 支持 iOS 刘海屏、Android 挖孔屏/刘海屏
 */
import { ref, onMounted, onUnmounted } from 'vue'

export interface SafeArea {
  top: number
  bottom: number
  left: number
  right: number
  // 状态栏高度（Android）
  statusBar: number
  // 导航栏高度（Android）
  navigationBar: number
}

export function useSafeAreaDetection() {
  const safeArea = ref<SafeArea>({
    top: 0,
    bottom: 0,
    left: 0,
    right: 0,
    statusBar: 0,
    navigationBar: 0,
  })

  const isDetected = ref(false)

  function detectSafeArea() {
    if (typeof document === 'undefined') return

    const html = document.documentElement
    const body = document.body

    // 从 CSS env() 获取 iOS 安全区域
    const top = parseInt(getComputedStyle(html).getPropertyValue('--safe-area-inset-top') || '0')
    const bottom = parseInt(getComputedStyle(html).getPropertyValue('--safe-area-inset-bottom') || '0')
    const left = parseInt(getComputedStyle(html).getPropertyValue('--safe-area-inset-left') || '0')
    const right = parseInt(getComputedStyle(html).getPropertyValue('--safe-area-inset-right') || '0')

    // 尝试通过 JavaScript 获取 Android 状态栏高度
    let statusBarHeight = 0
    let navigationBarHeight = 0

    // 方法1: 通过 window.visualViewport 获取
    if (window.visualViewport) {
      const viewport = window.visualViewport
      const diff = window.innerHeight - viewport.height

      // 如果有高度差，说明键盘弹出或工具栏显示
      if (diff > 0 && diff < 300) {
        // 可能是键盘弹出
      }
    }

    // 方法2: 通过屏幕高度和可用高度计算 (Android)
    const screenHeight = window.screen.height
    const availHeight = window.screen.availHeight
    const innerHeight = window.innerHeight

    // 状态栏 = 屏幕高度 - 可用高度 - 导航栏
    // 这种计算方式在应用全屏时比较准确
    if (availHeight < screenHeight) {
      navigationBarHeight = screenHeight - availHeight
    }

    // 方法3: 通过 innerHeight 和 availHeight 的差值判断状态栏
    // 在全屏模式下，innerHeight 应该等于 availHeight（减去导航栏）
    if (innerHeight < availHeight) {
      // 状态栏高度 = 可用高度 - 实际高度
      statusBarHeight = availHeight - innerHeight
    }

    // 综合计算：优先使用 CSS env()，如果没有则使用 JS 计算
    safeArea.value = {
      top: top || statusBarHeight || 24,     // iOS 或 Android 状态栏
      bottom: bottom || navigationBarHeight || 0,
      left,
      right,
      statusBar: statusBarHeight || top || 24,
      navigationBar: navigationBarHeight,
    }

    // 更新 CSS 变量供其他组件使用
    html.style.setProperty('--status-bar-height', `${safeArea.value.statusBar}px`)

    isDetected.value = true

    console.log('[SafeArea] Detected:', safeArea.value)
  }

  // 使用 ResizeObserver 监听视口变化
  let resizeObserver: ResizeObserver | null = null

  onMounted(() => {
    // 延迟检测，等待 DOM 完全加载
    requestAnimationFrame(() => {
      detectSafeArea()
    })

    // 监听视口变化
    if (window.visualViewport) {
      window.visualViewport.addEventListener('resize', detectSafeArea)
    }
    window.addEventListener('resize', detectSafeArea)

    // 使用 ResizeObserver 监听 body 变化
    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(() => {
        detectSafeArea()
      })
      resizeObserver.observe(document.body)
    }
  })

  onUnmounted(() => {
    if (window.visualViewport) {
      window.visualViewport.removeEventListener('resize', detectSafeArea)
    }
    window.removeEventListener('resize', detectSafeArea)
    resizeObserver?.disconnect()
  })

  return {
    safeArea,
    isDetected,
    detectSafeArea,
  }
}