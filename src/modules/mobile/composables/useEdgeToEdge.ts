/**
 * Edge-to-Edge 插件 Composable
 *
 * 使用 tauri-plugin-edge-to-edge 实现移动端安全区域和键盘高度检测
 *
 * 功能：
 * - 获取安全区域 insets (top, bottom, left, right)
 * - 获取键盘高度和可见状态
 * - 监听安全区域变化事件
 *
 * 初始化策略：
 * - 在 composable 创建时立即启动异步初始化（不等 onMounted）
 * - 首次渲染时 safeArea 为全 0，由 isReady 控制内容显示时机
 * - Android WebView 不支持 CSS env(safe-area-inset-*)，完全依赖 JS 值
 */
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { usePlatform } from '@/modules/shared/composables/usePlatform'

export interface SafeAreaInsets {
  top: number
  right: number
  bottom: number
  left: number
  statusBar: number
  navigationBar: number
}

export interface KeyboardInfo {
  keyboardHeight: number
  isVisible: boolean
}

export function useEdgeToEdge() {
  const { platformInfo } = usePlatform()

  const safeArea = ref<SafeAreaInsets>({
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
    statusBar: 0,
    navigationBar: 0,
  })

  const keyboardInfo = ref<KeyboardInfo>({
    keyboardHeight: 0,
    isVisible: false,
  })

  const isReady = ref(false)

  // 清理函数列表
  const cleanupFunctions: Array<() => void> = []

  /**
   * 处理安全区域变化事件
   */
  function handleSafeAreaChange(event: Event) {
    const customEvent = event as CustomEvent<{
      top: number
      right: number
      bottom: number
      left: number
      keyboardHeight: number
      keyboardVisible: boolean
    }>

    const { top, right, bottom, left, keyboardHeight, keyboardVisible } = customEvent.detail

    safeArea.value = {
      top,
      right,
      bottom,
      left,
      statusBar: top,
      navigationBar: bottom,
    }

    keyboardInfo.value = {
      keyboardHeight,
      isVisible: keyboardVisible,
    }

    console.log('[EdgeToEdge] Safe area changed:', safeArea.value, 'Keyboard:', keyboardInfo.value)
  }

  /**
   * 通过 Tauri 命令获取安全区域
   */
  async function getSafeAreaInsets(): Promise<SafeAreaInsets> {
    if (!platformInfo.value.isMobile) {
      return {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
        statusBar: 0,
        navigationBar: 0,
      }
    }

    try {
      const result = await invoke<SafeAreaInsets>('plugin:edge-to-edge|get_safe_area_insets')
      safeArea.value = result
      return result
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to get safe area insets:', e)
      return safeArea.value
    }
  }

  /**
   * 通过 Tauri 命令获取键盘信息
   */
  async function getKeyboardInfo(): Promise<KeyboardInfo> {
    if (!platformInfo.value.isMobile) {
      return { keyboardHeight: 0, isVisible: false }
    }

    try {
      const result = await invoke<KeyboardInfo>('plugin:edge-to-edge|get_keyboard_info')
      keyboardInfo.value = result
      return result
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to get keyboard info:', e)
      return keyboardInfo.value
    }
  }

  /**
   * 使用 Tauri 事件系统监听安全区域变化
   */
  async function setupEventListener() {
    try {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<{
        top: number
        right: number
        bottom: number
        left: number
        keyboardHeight: number
        keyboardVisible: boolean
      }>('safeAreaChanged', (event) => {
        const { top, right, bottom, left, keyboardHeight, keyboardVisible } = event.payload

        safeArea.value = {
          top,
          right,
          bottom,
          left,
          statusBar: top,
          navigationBar: bottom,
        }

        keyboardInfo.value = {
          keyboardHeight,
          isVisible: keyboardVisible,
        }

        console.log('[EdgeToEdge] Safe area changed via Tauri event:', safeArea.value)
      })

      return unlisten
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to setup Tauri event listener:', e)
      return null
    }
  }

  /**
   * 核心初始化逻辑 — 立即执行，不依赖 onMounted
   *
   * 提前启动异步初始化，使 safeArea 值在首次渲染前就可能就绪
   * onMounted 仅负责注册 DOM 事件监听和清理
   */
  async function initialize() {
    if (!platformInfo.value.isMobile) {
      isReady.value = true
      return
    }

    // 设置 Tauri 事件监听（推荐方式）
    const unlisten = await setupEventListener()
    if (unlisten) {
      cleanupFunctions.push(unlisten)
    }

    // 初始获取安全区域
    await getSafeAreaInsets()
    await getKeyboardInfo()

    isReady.value = true
    console.log('[EdgeToEdge] Initialized:', safeArea.value)
  }

  // 立即启动初始化，不等待 onMounted
  // 桌面端会同步设 isReady = true，移动端异步获取后设置
  initialize()

  onMounted(() => {
    // DOM 事件监听（fallback）
    if (platformInfo.value.isMobile) {
      window.addEventListener('safeAreaChanged', handleSafeAreaChange as EventListener)
    }
  })

  onUnmounted(() => {
    window.removeEventListener('safeAreaChanged', handleSafeAreaChange as EventListener)
    cleanupFunctions.forEach(fn => fn())
    cleanupFunctions.length = 0
  })

  // 计算属性：是否有安全区域
  const hasSafeArea = computed(() => {
    return safeArea.value.top > 0 || safeArea.value.bottom > 0
  })

  // 计算属性：键盘是否打开
  const isKeyboardOpen = computed(() => keyboardInfo.value.isVisible)

  // 计算属性：总底部安全区域（包括键盘）
  const totalBottomInset = computed(() => {
    const safeBottom = safeArea.value.bottom || 0
    const keyboardHeight = keyboardInfo.value.keyboardHeight || 0
    return safeBottom + keyboardHeight
  })

  return {
    safeArea,
    keyboardInfo,
    isReady,
    hasSafeArea,
    isKeyboardOpen,
    totalBottomInset,
    getSafeAreaInsets,
    getKeyboardInfo,
    enable: async () => {
      if (!platformInfo.value.isMobile) return
      try {
        await invoke('plugin:edge-to-edge|enable')
        console.log('[EdgeToEdge] Enabled')
      } catch (e) {
        console.warn('[EdgeToEdge] Failed to enable:', e)
      }
    },
    disable: async () => {
      if (!platformInfo.value.isMobile) return
      try {
        await invoke('plugin:edge-to-edge|disable')
        console.log('[EdgeToEdge] Disabled')
      } catch (e) {
        console.warn('[EdgeToEdge] Failed to disable:', e)
      }
    },
    showKeyboard: async () => {
      if (!platformInfo.value.isMobile) return
      try {
        await invoke('plugin:edge-to-edge|show_keyboard')
      } catch (e) {
        console.warn('[EdgeToEdge] Failed to show keyboard:', e)
      }
    },
    hideKeyboard: async () => {
      if (!platformInfo.value.isMobile) return
      try {
        await invoke('plugin:edge-to-edge|hide_keyboard')
      } catch (e) {
        console.warn('[EdgeToEdge] Failed to hide keyboard:', e)
      }
    },
  }
}
