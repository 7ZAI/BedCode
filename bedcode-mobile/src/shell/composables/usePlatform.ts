/**
 * 平台检测 — 壳内机制副本
 * -----------------------------------------------------------------------------
 * 迁移自旧机制 `src/composables/usePlatform.ts`。
 *
 * 为什么归壳：平台判断是平台级事实，且纪律上**禁止用 UA / viewport 推断平台**，
 * 必须有唯一入口（Tauri `plugin-os`）。壳与将来各应用都从这里取。
 *
 * 与旧实现的三处差异（其余逐字复制）：
 *   1. `PlatformInfo` / `Platform` / `Arch` 类型内联到本文件——壳不依赖旧
 *      类型集合 `@/composables/model`，类型随机制自持（形状逐字一致）。
 *   2. 运行时判定不再读 `window.__TAURI__`：壳的 L1 锁禁止裸读 Tauri 全局
 *      （那等于在业务代码里判断运行时）。改为「直接调 `plugin-os`，失败即视为
 *      非 Tauri 运行时」——真值是插件调用本身，不依赖任何注入全局；浏览器/dev
 *      预览走同一条失败路径回到模拟模式，行为与旧实现一致。
 *   3. 探测失败分支补 `logger.warn`（旧实现靠内联判断，无失败可观测性）——
 *      与壳「无静默错误」纪律一致；回退值与旧实现相同（桌面 / 非移动）。
 *
 * 开发环境（浏览器）可用 `?platform=mobile` 或 `localStorage['platform-mode']`
 * 模拟；生产环境走 Tauri OS 插件真值。
 */

import { ref, readonly, onMounted } from 'vue'
import { logger } from '@/utils/frontendLogger'

export type Platform = 'windows' | 'macos' | 'linux' | 'android' | 'ios'
export type Arch = 'x86_64' | 'aarch64' | 'arm'

/** 平台信息（形状与旧 `@/composables/model::PlatformInfo` 一致） */
export interface PlatformInfo {
  platform: Platform | null
  arch: Arch | null
  osVersion: string | null
  osType: string | null
  isDesktop: boolean
  isMobile: boolean
  isWindows: boolean
  isMacos: boolean
  isLinux: boolean
  isAndroid: boolean
  isIos: boolean
}

const platformInfo = ref<PlatformInfo>({
  platform: null,
  arch: null,
  osVersion: null,
  osType: null,
  // 检测前乐观按移动端呈现（移动端用户更多等待体验），检测完成后覆盖
  isDesktop: false,
  isMobile: true,
  isWindows: false,
  isMacos: false,
  isLinux: false,
  isAndroid: false,
  isIos: false,
})

// 使用 Promise 来同步等待初始化完成
let initialized = false
let initPromise: Promise<PlatformInfo> | null = null

/** 已知平台名（与 Platform 类型同集合）：返回值不在其中即视为探测不可信 */
const KNOWN_PLATFORMS: readonly string[] = ['windows', 'macos', 'linux', 'android', 'ios']

/**
 * 探测运行时平台名。
 *
 * 返回 `null` = 不在 Tauri 运行时（浏览器 / dev 预览）或插件不可用；调用方据此
 * 落到模拟模式。判定真源是插件调用本身：插件模块在任何环境都能 import，但只有
 * 在 Tauri WebView 里带 IPC 桥才会返回已知平台名——**名字必须落在已知集合内**
 * 才认（IPC 桥缺失时拿到的是 undefined / 异常，这类值绝不能被当成真实平台）。
 */
async function detectRuntimePlatform(): Promise<Platform | null> {
  try {
    const { platform } = await import('@tauri-apps/plugin-os')
    const name = platform()
    if (typeof name === 'string' && KNOWN_PLATFORMS.includes(name)) return name as Platform
    logger.log('[Platform] runtime platform unavailable, fall back to browser simulation:', name)
    return null
  } catch (e) {
    logger.log('[Platform] Tauri OS plugin unavailable, fall back to browser simulation:', e)
    return null
  }
}

/** 从 Tauri OS 插件取完整平台信息；非 Tauri 运行时返回 null */
async function detectFromTauri(): Promise<PlatformInfo | null> {
  const platformName = await detectRuntimePlatform()
  if (platformName === null) return null

  try {
    const { arch, version, type } = await import('@tauri-apps/plugin-os')
    const archResult = arch()
    const versionResult = version()
    const typeResult = type()

    const isDesktop = !['android', 'ios'].includes(platformName)
    const isMobile = ['android', 'ios'].includes(platformName)

    return {
      platform: platformName,
      arch: (archResult as Arch | null) ?? null,
      osVersion: versionResult ?? null,
      osType: typeResult ?? null,
      isDesktop,
      isMobile,
      isWindows: platformName === 'windows',
      isMacos: platformName === 'macos',
      isLinux: platformName === 'linux',
      isAndroid: platformName === 'android',
      isIos: platformName === 'ios',
    }
  } catch (e) {
    logger.warn('[Platform] Tauri OS plugin metadata unavailable:', e)
    return null
  }
}

/** 浏览器 / dev 预览的模拟模式判据（URL 参数 > localStorage > 默认桌面） */
function browserSimulatedMode(): 'desktop' | 'mobile' {
  const urlMode = new URLSearchParams(window.location.search).get('platform')
  const storedMode = localStorage.getItem('platform-mode')
  return (urlMode || storedMode || 'desktop') === 'mobile' ? 'mobile' : 'desktop'
}

/** 在浏览器环境中模拟平台信息（开发调试用） */
function simulateForBrowser(): PlatformInfo {
  const simulatedMode = browserSimulatedMode()
  const isMobile = simulatedMode === 'mobile'

  logger.log(
    '[Platform] Browser simulation mode:',
    simulatedMode,
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

/** 平台检测 composable */
export function usePlatform() {
  async function detectPlatform() {
    if (initialized) {
      return
    }

    const info = (await detectFromTauri()) ?? simulateForBrowser()
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
 * 立即初始化平台检测（用于路由守卫）
 * 返回 Promise，等待检测完成后返回平台信息
 */
export async function initPlatform(): Promise<PlatformInfo> {
  // 已初始化，直接返回当前值
  if (initialized && platformInfo.value.platform !== null) {
    return platformInfo.value
  }

  // 正在初始化，等待完成
  if (initPromise) {
    return initPromise
  }

  // 开始初始化
  initPromise = (async () => {
    const info = (await detectFromTauri()) ?? simulateForBrowser()
    platformInfo.value = info
    initialized = true
    return info
  })()

  return initPromise
}

/** 获取当前平台信息（同步，可能为初始状态） */
export function getPlatformInfo(): PlatformInfo {
  return platformInfo.value
}

/** 快速检测是否为桌面平台 */
export function useIsDesktop() {
  const isDesktop = ref(true)

  onMounted(async () => {
    const platformName = await detectRuntimePlatform()
    if (platformName === null) {
      // 浏览器 / dev 预览：按模拟模式判定，与 detectPlatform 同一真源
      isDesktop.value = browserSimulatedMode() === 'desktop'
      return
    }
    isDesktop.value = platformName !== 'android' && platformName !== 'ios'
  })

  return readonly(isDesktop)
}

/** 快速检测是否为移动平台 */
export function useIsMobile() {
  const isMobile = ref(false)

  onMounted(async () => {
    const platformName = await detectRuntimePlatform()
    if (platformName === null) {
      // 浏览器 / dev 预览：按模拟模式判定，与 detectPlatform 同一真源
      isMobile.value = browserSimulatedMode() === 'mobile'
      return
    }
    isMobile.value = platformName === 'android' || platformName === 'ios'
  })

  return readonly(isMobile)
}
