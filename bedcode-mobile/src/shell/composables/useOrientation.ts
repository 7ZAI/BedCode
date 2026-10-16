/**
 * 屏幕方向与断点检测 — 壳内机制副本
 * -----------------------------------------------------------------------------
 * 迁移自旧机制 `src/composables/useOrientation.ts`（逻辑逐字复制，仅 logger
 * 仍取共享实现 `@/utils/frontendLogger`——日志是跨新旧 UI 的通用基础设施，
 * 复制会变成两套攒批转发，复制到壳内反而有害）。
 *
 * 平台纪律：方向 / 断点判断走本机制，禁止用 UA 或 viewport 宽度推断平台
 * （平台判断走 `usePlatform`，Tauri API 为真源）。
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { logger } from '@/utils/frontendLogger'

/** 横竖屏检测 */
export function useOrientation() {
  const isLandscape = ref(false)
  const orientation = ref<'portrait' | 'landscape'>('portrait')

  function updateOrientation() {
    const width = window.innerWidth
    const height = window.innerHeight

    // 当宽度大于高度时认为是横屏
    isLandscape.value = width > height
    orientation.value = isLandscape.value ? 'landscape' : 'portrait'

    logger.log('[Orientation] Changed to:', orientation.value, 'Size:', width, 'x', height)
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

/** 响应式断点检测（结构层适配用；连续值调节走 clamp + cqw） */
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
