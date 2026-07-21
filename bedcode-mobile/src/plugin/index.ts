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
import { usePresetTasks } from '@/composables/usePresetTasks'

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
  // 1. 初始化共享运行时
  await initSharedRuntime(app, pinia, router, i18n, { usePresetTasks })

  // 2. 加载所有已激活插件的前端模块
  await pluginLoader.loadAll()

  console.log('[PluginSystem] Initialized')
}
