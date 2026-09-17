//! Desktop Commands - Rust 后端命令封装（聚合层）
//!
//! 所有桌面端可用的 Tauri 命令调用。命令实现已按领域拆分到
//! src/composables/commands/ 下，本文件仅聚合 re-export 并保留
//! useDesktopCommands() composable 兼容既有调用方：
//! - sessionCommands：WSL 探测 + 会话生命周期 + 会话配置 CRUD
//! - deviceCommands：设备 / 配对 / 连接历史 / QR / 快捷操作
//! - settingsCommands：设置 / 系统
//! - eventListeners：设备连接事件监听与清理

export * from './commands/sessionCommands'
export * from './commands/deviceCommands'
export * from './commands/settingsCommands'
export * from './commands/eventListeners'

import {
  listWslDistributions,
  isWslAvailable,
  startSession,
  createSessionNoStart,
  startExistingSession,
  listSessions,
  getSession,
  killSession,
  deleteSession,
  restartSession,
  resizeSession,
  writeToSession,
  sendSpecialKey,
  createSessionConfig,
  listSessionConfigs,
  getSessionConfig,
  deleteSessionConfig,
  updateSessionConfig,
} from './commands/sessionCommands'
import {
  getConnectedDevices,
  generatePairingCode,
  getCurrentPairingCode,
  verifyPairingCode,
  clearPairingCode,
  getPairingCodeTtl,
  setPairingCodeTtl,
  listPairedDevices,
  removePairedDevice,
  listConnectionHistory,
  deleteConnectionHistory,
  generateQrCode,
  clearQrCode,
  getQrConnectionInfo,
  getQrTokenTtl,
  setQrTokenTtl,
  listQuickActions,
  createQuickAction,
  updateQuickAction,
  deleteQuickAction,
} from './commands/deviceCommands'
import {
  getAllDbSettings,
  setDbSetting,
  getAppSettings,
  saveAppSettings,
  ping,
  getAppVersion,
  getStartupTime,
  getLocalIpAddresses,
} from './commands/settingsCommands'
import { onDeviceConnected, onDeviceDisconnected, cleanupEventListeners } from './commands/eventListeners'

// ==================== Desktop Commands Composable ====================

/**
 * 桌面端命令 composable
 * 整合所有桌面端可用的 Rust 命令
 */
export function useDesktopCommands() {
  return {
    // WSL
    listWslDistributions,
    isWslAvailable,

    // Session
    startSession,
    createSessionNoStart,
    startExistingSession,
    listSessions,
    getSession,
    killSession,
    deleteSession,
    restartSession,
    resizeSession,
    writeToSession,
    sendSpecialKey,

    // Device
    getConnectedDevices,

    // Config
    createSessionConfig,
    listSessionConfigs,
    getSessionConfig,
    deleteSessionConfig,
    updateSessionConfig,

    // Pairing
    generatePairingCode,
    getCurrentPairingCode,
    verifyPairingCode,
    clearPairingCode,
    getPairingCodeTtl,
    setPairingCodeTtl,
    listPairedDevices,
    removePairedDevice,

    // Connection History
    listConnectionHistory,
    deleteConnectionHistory,

    // QR
    generateQrCode,
    clearQrCode,
    getQrConnectionInfo,
    getQrTokenTtl,
    setQrTokenTtl,

    // Quick Actions
    listQuickActions,
    createQuickAction,
    updateQuickAction,
    deleteQuickAction,

    // Settings
    getAllDbSettings,
    setDbSetting,
    getAppSettings,
    saveAppSettings,

    // System
    ping,
    getAppVersion,
    getStartupTime,
    getLocalIpAddresses,

    // Events
    onDeviceConnected,
    onDeviceDisconnected,
    cleanupEventListeners,
  }
}
