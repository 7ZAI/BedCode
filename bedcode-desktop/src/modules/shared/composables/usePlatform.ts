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

// Re-export from model
import type { PlatformInfo } from './model'
export type { PlatformInfo }

export type Platform = 'windows' | 'macos' | 'linux' | 'android' | 'ios'
export type Arch = 'x86_64' | 'aarch64' | 'arm'

const platformInfo = ref<PlatformInfo>({
  platform: null,
  arch: null,
  osVersion: null,
  osType: null,
  isDesktop: false,
  isMobile: true,
  isWindows: false,
  isMacos: false,
  isLinux: false,
  isAndroid: false,
  isIos: false,
})

let initialized = false
let initPromise: Promise<PlatformInfo> | null = null

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

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
      isAndroid: platformResult === 'android',
      isIos: platformResult === 'ios',
    }
  } catch (e) {
    console.warn('[Platform] Tauri OS plugin not available:', e)
    return null
  }
}

function simulateForBrowser(): PlatformInfo {
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
    isAndroid: isMobile,
    isIos: false,
  }
}

export function usePlatform() {
  async function detectPlatform() {
    if (initialized) {
      return
    }

    let info: PlatformInfo | null = null

    if (isTauriRuntime()) {
      info = await detectFromTauri()
      if (info) {
        console.log('[Platform] Detected (Tauri):', info)
      }
    }

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

export async function initPlatform(): Promise<PlatformInfo> {
  if (initialized && platformInfo.value.platform !== null) {
    return platformInfo.value
  }

  if (initPromise) {
    return initPromise
  }

  initPromise = (async () => {
    let info: PlatformInfo | null = null

    if (isTauriRuntime()) {
      info = await detectFromTauri()
      if (info) {
        console.log('[Platform] Detected (Tauri) via initPlatform:', info)
      }
    }

    if (!info) {
      info = simulateForBrowser()
    }

    platformInfo.value = info
    initialized = true
    return info
  })()

  return initPromise
}
