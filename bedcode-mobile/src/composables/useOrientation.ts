import { ref, onMounted, onUnmounted } from 'vue'

/**
 * 屏幕方向检测 Composable
 *
 * 检测设备是否为横屏模式
 */
export function useOrientation() {
  const isLandscape = ref(false)
  const orientation = ref<'portrait' | 'landscape'>('portrait')

  function updateOrientation() {
    const width = window.innerWidth
    const height = window.innerHeight

    // 当宽度大于高度时认为是横屏
    isLandscape.value = width > height
    orientation.value = isLandscape.value ? 'landscape' : 'portrait'

    console.log('[Orientation] Changed to:', orientation.value, 'Size:', width, 'x', height)
  }

  onMounted(() => {
    // 初始检测
    updateOrientation()

    // 监听屏幕旋转
    window.addEventListener('resize', updateOrientation)
    window.addEventListener('orientationchange', updateOrientation)

    // 如果支持 screen.orientation API
    if (screen.orientation) {
      screen.orientation.addEventListener('change', updateOrientation)
    }
  })

  onUnmounted(() => {
    window.removeEventListener('resize', updateOrientation)
    window.removeEventListener('orientationchange', updateOrientation)

    if (screen.orientation) {
      screen.orientation.removeEventListener('change', updateOrientation)
    }
  })

  return {
    isLandscape,
    orientation,
  }
}

/**
 * 响应式断点检测
 */
export function useBreakpoints() {
  const width = ref(window.innerWidth)

  const isMobile = ref(width.value < 768)
  const isTablet = ref(width.value >= 768 && width.value < 1024)
  const isDesktop = ref(width.value >= 1024)
  const isSmall = ref(width.value < 400)

  function updateWidth() {
    width.value = window.innerWidth
    isMobile.value = width.value < 768
    isTablet.value = width.value >= 768 && width.value < 1024
    isDesktop.value = width.value >= 1024
    isSmall.value = width.value < 400
  }

  onMounted(() => {
    window.addEventListener('resize', updateWidth)
  })

  onUnmounted(() => {
    window.removeEventListener('resize', updateWidth)
  })

  return {
    width,
    isMobile,
    isTablet,
    isDesktop,
    isSmall,
  }
}