/**
 * Dev Shell 共享运行时
 *
 * 与宿主 shared-runtime.ts 同构：把 dev-shell 自身的 vue / vue-i18n / pinia /
 * router / i18n 实例 + mock 模块（presetTasks / dialogs / mobileApi）暴露到
 * window.__BEDCODE_SHARED__，插件经 SDK getSharedModule() 取用。
 * 必须在导入插件模块之前初始化。
 */

const SHARED_KEY = '__BEDCODE_SHARED__'

export interface SharedRuntimeOptions {
  pinia: any
  router: any
  i18n: any
  presetTasks: any
  dialogs: any
  mobileApi: any
}

export async function initSharedRuntime(options: SharedRuntimeOptions): Promise<void> {
  const vue = await import('vue')
  const vueI18n = await import('vue-i18n')
  const piniaMod = await import('pinia')
  ;(window as any)[SHARED_KEY] = {
    vue,
    'vue-i18n': vueI18n,
    pinia: options.pinia,
    i18n: options.i18n,
    router: options.router,
    presetTasks: options.presetTasks,
    dialogs: options.dialogs,
    mobileApi: options.mobileApi,
  }
}

/** 获取共享模块（SDK runtime.ts 同款实现） */
export function getSharedModule(name: string): any {
  const shared = (window as any)[SHARED_KEY]
  if (!shared) throw new Error('[DevShell] Shared runtime not initialized')
  const mod = shared[name]
  if (!mod) throw new Error(`[DevShell] Shared module "${name}" not found`)
  return mod
}
