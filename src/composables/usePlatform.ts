//! Platform Detection Composable
//!
//! 提供跨平台检测功能
//!
//! 开发环境 (浏览器):
//!   - 通过 URL 参数 ?platform=mobile 模拟移动端
//!   - 通过 localStorage.setItem('platform-mode', 'mobile') 持久化设置
//!
//! 生产环境 (Tauri):
//!   - 使用 @tauri-apps/plugin-os 获取真实平台信息

import { ref, readonly, onMounted } from 'vue'

export type Platform = 'windows' | 'macos' | 'linux' | 'android' | 'ios'
export type Arch = 'x86_64' | 'aarch64' | 'arm'

interface PlatformInfo {
  platform: Platform | null
  arch: Arch | null
  osVersion: string | null
  osType: string | null
  isDesktop: boolean
  isMobile: boolean
  isWindows: boolean
  isMacos: boolean
  isLinux: boolean
}

const platformInfo = ref<PlatformInfo>({
  platform: null,
  arch: null,
  osVersion: null,
  osType: null,
  isDesktop: false,
  isMobile: false,
  isWindows: false,
  isMacos: false,
  isLinux: false,
})

let initialized = false

/**
 * 检测是否在 Tauri 运行时环境中
 */
function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

/**
 * 从 Tauri OS 插件获取平台信息
 */
async function detectFromTauri(): Promise<PlatformInfo | null> {
  try {
    const { platform, arch, version, type } = await import('@tauri-apps/plugin-os')

    const platformResult = platform()
    const archResult = arch()
    const versionResult = version()
    const typeResult = type()

    const isDesktop = platformResult !== null &&
      !['android', 'ios'].includes(platformResult)
    const isMobile = platformResult !== null &&
      ['android', 'ios'].includes(platformResult)

    return {
      platform: platformResult as Platform | null,
      arch: archResult as Arch | null,
      osVersion: versionResult,
      osType: typeResult,
      isDesktop,
      isMobile,
      isWindows: platformResult === 'windows',
      isMacos: platformResult === 'macos',
      isLinux: platformResult === 'linux',
    }
  } catch (e) {
    console.warn('[Platform] Tauri OS plugin not available:', e)
    return null
  }
}

/**
 * 在浏览器环境中模拟平台信息 (用于开发调试)
 */
function simulateForBrowser(): PlatformInfo {
  // 优先级: URL 参数 > localStorage > 默认桌面
  const urlParams = new URLSearchParams(window.location.search)
  const urlMode = urlParams.get('platform')
  const storedMode = localStorage.getItem('platform-mode')
  const simulatedPlatform = urlMode || storedMode || 'desktop'

  const isMobile = simulatedPlatform === 'mobile'

  console.log(
    '[Platform] Browser simulation mode:',
    simulatedPlatform,
    '- Use ?platform=mobile or localStorage to switch'
  )

  return {
    platform: isMobile ? 'android' : 'windows',
    arch: 'x86_64',
    osVersion: 'Browser',
    osType: 'Web',
    isDesktop: !isMobile,
    isMobile,
    isWindows: !isMobile,
    isMacos: false,
    isLinux: false,
  }
}

/**
 * 平台检测 composable
 */
export function usePlatform() {
  async function detectPlatform() {
    if (initialized) {
      return
    }

    let info: PlatformInfo | null = null

    // 生产环境: Tauri 运行时
    if (isTauriRuntime()) {
      info = await detectFromTauri()
      if (info) {
        console.log('[Platform] Detected (Tauri):', info)
      }
    }

    // 开发环境或 Tauri 检测失败: 浏览器模拟
    if (!info) {
      info = simulateForBrowser()
    }

    platformInfo.value = info
    initialized = true
  }

  onMounted(() => {
    detectPlatform()
  })

  return {
    platformInfo: readonly(platformInfo),
    detectPlatform,
  }
}

/**
 * 快速检测是否为桌面平台
 */
export function useIsDesktop() {
  const isDesktop = ref(true)

  onMounted(async () => {
    if (isTauriRuntime()) {
      try {
        const { platform } = await import('@tauri-apps/plugin-os')
        const p = platform()
        isDesktop.value = p !== 'android' && p !== 'ios'
      } catch {
        isDesktop.value = true
      }
    } else {
      // 浏览器环境: 检查模拟模式
      const storedMode = localStorage.getItem('platform-mode')
      const urlMode = new URLSearchParams(window.location.search).get('platform')
      const mode = urlMode || storedMode || 'desktop'
      isDesktop.value = mode !== 'mobile'
    }
  })

  return readonly(isDesktop)
}

/**
 * 快速检测是否为移动平台
 */
export function useIsMobile() {
  const isMobile = ref(false)

  onMounted(async () => {
    if (isTauriRuntime()) {
      try {
        const { platform } = await import('@tauri-apps/plugin-os')
        const p = platform()
        isMobile.value = p === 'android' || p === 'ios'
      } catch {
        isMobile.value = false
      }
    } else {
      // 浏览器环境: 检查模拟模式
      const storedMode = localStorage.getItem('platform-mode')
      const urlMode = new URLSearchParams(window.location.search).get('platform')
      const mode = urlMode || storedMode || 'desktop'
      isMobile.value = mode === 'mobile'
    }
  })

  return readonly(isMobile)
}
