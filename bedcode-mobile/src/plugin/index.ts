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
import { pluginLoader } from './loader'
import { pluginDialogHost } from './dialog-host'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { httpRequest } from '@/composables/useHttpApi'

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
  await initSharedRuntime(
    app,
    pinia,
    router,
    i18n,
    { usePresetTasks },
    pluginDialogHost,
    {
      activeSessionId: connection.activeSessionId,
      activeSessions: connection.activeSessions,
      sessionConfigs: connection.sessionConfigs,
      isConnected: connection.isConnected,
      httpRequest,
    },
  )

  // 2. 加载所有已激活插件的前端模块
  await pluginLoader.loadAll()

  console.log('[PluginSystem] Initialized')
}
