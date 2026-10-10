/**
 * Mobile Plugin System
 *
 * 模块入口 + 初始化函数
 */

export { pluginLoader } from './loader'
export { getPluginRegistry } from './registry'
export { initSharedRuntime, getSharedModule } from './shared-runtime'
export type * from './types'

import { initSharedRuntime } from './shared-runtime'
import { logger } from '@/utils/frontendLogger'
import { pluginLoader } from './loader'
import { pluginDialogHost } from './dialog-host'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useMobileSettings, readAllSettings, writeSetting } from '@/composables/useMobileSettings'
import { useMdnsDiscovery } from '@/composables/useMdnsDiscovery'
import {
  wsGetBiometricKeyStatus,
  wsBindBiometricCredential,
  wsUnbindBiometricCredential,
} from '@/composables/useMobileCommands'
import { useIsDark } from '@/composables/useTheme'
import { httpRequest } from '@/composables/useHttpApi'
import { MOCK_SESSION_ID } from '@binblink/bedcode-plugin-sdk-mobile'
import { subscribeHostSessionEvents } from './host-events'
import { subscribeConnectionEvents } from './connection-events'
import { openTerminalStream } from './terminal-stream'

/**
 * 初始化插件系统
 *
 * 在 main.ts 中调用，在 app.mount() 之前
 */
export async function initPluginSystem(
  app: any,
  pinia: any,
  router: any,
  i18n: any,
): Promise<void> {
  // 1. 初始化共享运行时（含对话框服务）
  // mobileApi：移动端宿主通用连接/HTTP 能力（活动会话/会话配置/连接态 + 通用 httpRequest 通道），
  // 供插件前端经 SDK getMobileApi() 访问；具体业务端点（任务队列/任务历史/定时任务等）
  // 由各插件基于 httpRequest 自行封装，宿主不感知插件领域细节
  const connection = useMobileConnection()
  const mobileSettings = useMobileSettings()
  const mdns = useMdnsDiscovery()
  await initSharedRuntime(
    app,
    pinia,
    router,
    i18n,
    { usePresetTasks },
    pluginDialogHost,
    {
      // ── 会话 / 连接投影（既有） ──
      activeSessionId: connection.activeSessionId,
      activeSessions: connection.activeSessions,
      sessionConfigs: connection.sessionConfigs,
      isConnected: connection.isConnected,
      httpRequest,
      loadActiveSessions: connection.loadActiveSessions,
      loadSessionConfigs: connection.loadSessionConfigs,
      hasLoadedConfigs: connection.hasLoadedConfigs,
      isLoadingConfigs: connection.isLoadingConfigs,
      // 票 15：终端 UI 域下沉所需宿主机制面（见 SDK MobileHostApi 注释）
      openTerminalStream,
      isDark: useIsDark(),
      mobileSettings: mobileSettings.settings,
      // 票 2026-10-10：通用设置 KV 桥——业务设置项的 UI 与真源归各 wasm app 自持
      // （§5.1 B3：宿主不持有产品事实），宿主只提供持久化通道（ADR 0022 ① 引擎实现）
      readAllSettings,
      writeSetting,
      onSessionEvent: subscribeHostSessionEvents,
      mockSessionId: import.meta.env.DEV ? MOCK_SESSION_ID : null,
      // ── 连接引擎面（票 2026-10-09：宿主页下沉 terminal-session，引擎事实与动作）──
      connectionStatus: connection.connectionStatus,
      isConnecting: connection.isConnecting,
      currentDevice: connection.currentDevice,
      connectionHistory: connection.connectionHistory,
      connectDevice: connection.connect,
      cancelConnection: connection.cancelConnection,
      disconnect: connection.disconnect,
      loadConnectionHistory: connection.loadConnectionHistory,
      clearConnectionHistory: connection.clearConnectionHistory,
      removeFromConnectionHistory: connection.removeFromConnectionHistory,
      onConnectionEvent: subscribeConnectionEvents,
      // ── mDNS 主机发现引擎事实（原始发现事实投影，派生列表归插件自持）──
      mdnsServices: mdns.discoveredServices,
      mdnsScanning: mdns.isScanning,
      mdnsStart: mdns.startDiscovery,
      mdnsStop: mdns.stopDiscovery,
      mdnsRefresh: mdns.refreshServices,
      // ── 生物凭证引擎面（C4：凭证留宿主，只投状态 + 绑定动作）──
      getBiometricKeyStatus: wsGetBiometricKeyStatus,
      bindBiometricCredential: wsBindBiometricCredential,
      unbindBiometricCredential: wsUnbindBiometricCredential,
    },
  )

  // 2. 加载所有已激活插件的前端模块
  await pluginLoader.loadAll()

  logger.log('[PluginSystem] Initialized')
}
