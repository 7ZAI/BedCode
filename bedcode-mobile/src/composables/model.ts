//! Shared Composable Types

export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
}

export interface PlatformInfo {
  platform: 'windows' | 'macos' | 'linux' | 'android' | 'ios' | null
  arch: 'x86_64' | 'aarch64' | 'arm' | null
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
