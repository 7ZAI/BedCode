//! Tauri API Types Re-export and Composables
//!
//! 从各模块重新导出 Tauri 相关的类型和函数

import { ref } from 'vue'
import {
  generateQrCode,
  clearQrCode,
  getQrConnectionInfo,
  getQrTokenTtl,
  setQrTokenTtl,
  type PairingCodeInfo,
} from '@/modules/desktop/composables/useDesktopCommands'

// Re-export types from desktop model
export type { SessionInfo, SessionConfig, DeviceConnectionInfo, WslDistro } from '@/modules/desktop/composables/model'

// Re-export types from mobile model
export type { ConnectionStatus, RemoteDevice, AuthCredentials, ConnectionInfo, AuthState, RemoteSession, TerminalOutputEvent, TerminalIncrementalOutput } from '@/modules/mobile/composables/model'

// Re-export types from shared model
export type { QrConnectionInfo, SessionStatusEvent, SessionRestartEvent, AnsiRenderOptions, AppError, Shortcut, BufferedOutput, OutputBlock, PairedDevice, Notification } from '@/modules/shared/composables/model'

// 重新导出桌面端 composables
export { useWsl } from '@/modules/desktop/composables/useWsl'
export { useWslStore } from '@/modules/desktop/stores/wsl'
export { usePtyOutput } from '@/modules/desktop/composables/usePtyOutput'
export { usePairing } from '@/modules/desktop/composables/usePairing'
export { useNetwork } from '@/modules/desktop/composables/useNetwork'
export { useConnectedDevices } from '@/modules/desktop/composables/useConnectedDevices'

// 导出配对相关类型
export type { PairingCodeInfo }

// 重新导出 QR 码 composable
export { useQrCode } from '@/modules/shared/composables/useQrCode'

// QR 码 API（用于设置页面）
export function useQrCodeApi() {
  const qrTokenTtl = ref(300)

  async function getQrTokenTtlApi() {
    qrTokenTtl.value = await getQrTokenTtl()
    return qrTokenTtl.value
  }

  async function setQrTokenTtlApi(ttl: number) {
    await setQrTokenTtl(ttl)
    qrTokenTtl.value = ttl
  }

  async function generateQrCodeApi() {
    await generateQrCode()
  }

  async function clearQrCodeApi() {
    await clearQrCode()
  }

  async function getQrConnectionInfoApi(host?: string) {
    return await getQrConnectionInfo(host)
  }

  return {
    qrTokenTtl,
    getQrTokenTtl: getQrTokenTtlApi,
    setQrTokenTtl: setQrTokenTtlApi,
    generateQrCode: generateQrCodeApi,
    clearQrCode: clearQrCodeApi,
    getQrConnectionInfo: getQrConnectionInfoApi,
  }
}