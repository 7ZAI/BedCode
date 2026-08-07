/**
 * Plugin Shared Runtime
 *
 * 将宿主的 Vue、Pinia、vue-i18n、Router 实例暴露到 window.__BEDCODE_SHARED__
 * 供插件 TS 代码通过 SDK 访问
 */

const SHARED_KEY = '__BEDCODE_SHARED__'

/** 初始化共享运行时（应用启动时调用一次） */
export async function initSharedRuntime(
  app: any,
  pinia: any,
  router: any,
  i18n: any,
  presetTasks: any,
  dialogs: any,
  mobileApi: any,
): Promise<void> {
  const vue = await import('vue')
  ;(window as any)[SHARED_KEY] = {
    vue,
    pinia,
    router,
    i18n,
    presetTasks,
    dialogs,
    mobileApi,
  }
}

/** 获取共享模块 */
export function getSharedModule(name: string): any {
  const shared = (window as any)[SHARED_KEY]
  if (!shared) throw new Error('[PluginSDK] Shared runtime not initialized')
  const mod = shared[name]
  if (!mod) throw new Error(`[PluginSDK] Shared module "${name}" not found`)
  return mod
}
