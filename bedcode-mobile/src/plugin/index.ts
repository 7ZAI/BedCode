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
import {
  httpTaskQueueList,
  httpTaskQueueAdd,
  httpTaskQueueRemove,
  httpTaskQueueCancel,
  httpTaskQueueClear,
  httpTaskQueueUpdate,
  httpTaskQueueReorder,
  httpSessionSettings,
  httpSetSessionMode,
  httpCurrentTask,
  httpListSupportedAgents,
  httpTaskHistoryList,
  httpScheduledJobsList,
  httpScheduledJobCreate,
} from '@/composables/useHttpApi'

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
  // mobileApi：移动端宿主连接/HTTP 能力（当前活动会话 + AutoTask 队列接口），
  // 供插件前端经 SDK getMobileApi() 访问对端桌面端 REST API
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
      httpTaskQueueList,
      httpTaskQueueAdd,
      httpTaskQueueRemove,
      httpTaskQueueCancel,
      httpTaskQueueClear,
      httpTaskQueueUpdate,
      httpTaskQueueReorder,
      httpSessionSettings,
      httpSetSessionMode,
      httpCurrentTask,
      httpListSupportedAgents,
      httpTaskHistoryList,
      httpScheduledJobsList,
      httpScheduledJobCreate,
    },
  )

  // 2. 加载所有已激活插件的前端模块
  await pluginLoader.loadAll()

  console.log('[PluginSystem] Initialized')
}
