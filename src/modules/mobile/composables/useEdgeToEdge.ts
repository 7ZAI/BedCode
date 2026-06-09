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
 * CSS 变量（自动注入）：
 * - --safe-area-inset-top
 * - --safe-area-inset-bottom
 * - --safe-area-inset-left
 * - --safe-area-inset-right
 * - --keyboard-height
 * - --keyboard-visible
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

  /**
   * 从 CSS 变量读取安全区域值
   */
  function readFromCSSVariables(): Partial<SafeAreaInsets> {
    if (typeof document === 'undefined') return {}

    const html = document.documentElement
    const computedStyle = getComputedStyle(html)

    return {
      top: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-top') || '0'),
      bottom: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-bottom') || '0'),
      left: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-left') || '0'),
      right: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-right') || '0'),
      statusBar: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-top') || '0'),
      navigationBar: parseFloat(computedStyle.getPropertyValue('--safe-area-inset-bottom') || '0'),
    }
  }

  /**
   * 从 CSS 变量读取键盘信息
   */
  function readKeyboardFromCSSVariables(): KeyboardInfo {
    if (typeof document === 'undefined') return { keyboardHeight: 0, isVisible: false }

    const html = document.documentElement
    const computedStyle = getComputedStyle(html)

    const height = parseFloat(computedStyle.getPropertyValue('--keyboard-height') || '0')
    const visible = computedStyle.getPropertyValue('--keyboard-visible') === '1'

    return {
      keyboardHeight: height,
      isVisible: visible,
    }
  }

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
      // Fallback to CSS variables
      const cssValues = readFromCSSVariables()
      safeArea.value = {
        top: cssValues.top || 0,
        right: cssValues.right || 0,
        bottom: cssValues.bottom || 0,
        left: cssValues.left || 0,
        statusBar: cssValues.statusBar || 0,
        navigationBar: cssValues.navigationBar || 0,
      }
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
      // Fallback to CSS variables
      keyboardInfo.value = readKeyboardFromCSSVariables()
      return keyboardInfo.value
    }
  }

  /**
   * 启用 Edge-to-Edge 模式
   */
  async function enable(): Promise<void> {
    if (!platformInfo.value.isMobile) return

    try {
      await invoke('plugin:edge-to-edge|enable')
      console.log('[EdgeToEdge] Enabled')
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to enable:', e)
    }
  }

  /**
   * 禁用 Edge-to-Edge 模式
   */
  async function disable(): Promise<void> {
    if (!platformInfo.value.isMobile) return

    try {
      await invoke('plugin:edge-to-edge|disable')
      console.log('[EdgeToEdge] Disabled')
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to disable:', e)
    }
  }

  /**
   * 显示键盘
   */
  async function showKeyboard(): Promise<void> {
    if (!platformInfo.value.isMobile) return

    try {
      await invoke('plugin:edge-to-edge|show_keyboard')
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to show keyboard:', e)
    }
  }

  /**
   * 隐藏键盘
   */
  async function hideKeyboard(): Promise<void> {
    if (!platformInfo.value.isMobile) return

    try {
      await invoke('plugin:edge-to-edge|hide_keyboard')
    } catch (e) {
      console.warn('[EdgeToEdge] Failed to hide keyboard:', e)
    }
  }

  onMounted(async () => {
    if (!platformInfo.value.isMobile) {
      isReady.value = true
      return
    }

    // 监听安全区域变化事件
    window.addEventListener('safeAreaChanged', handleSafeAreaChange)

    // 初始获取一次
    await getSafeAreaInsets()
    await getKeyboardInfo()

    isReady.value = true
    console.log('[EdgeToEdge] Initialized:', safeArea.value)
  })

  onUnmounted(() => {
    window.removeEventListener('safeAreaChanged', handleSafeAreaChange)
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
    enable,
    disable,
    showKeyboard,
    hideKeyboard,
  }
}
