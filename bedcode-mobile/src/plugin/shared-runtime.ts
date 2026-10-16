/**
 * Plugin Shared Runtime
 *
 * 将宿主的 Vue、Pinia、vue-i18n、Router 实例暴露到 window.__BEDCODE_SHARED__
 * 供插件 TS 代码通过 SDK 访问
 *
 * 键名约定与桌面端 @binblink/bedcode-plugin-sdk-desktop 对齐：
 * - 'vue' / 'vue-i18n' / 'pinia'：模块（SDK vite-plugin 把插件 import 重写为
 *   对应共享键，插件侧 `import { useI18n } from 'vue-i18n'` 依赖此键）
 * - i18n：vue-i18n 实例（SDK getI18n() / 宿主 context 内部翻译用）
 *
 * pinia 一项**不是**原生模块对象：`defineStore` 已换成运行期作用域包装版
 * （见 app-run-scope.ts）——插件 store 归属该插件当前运行期，停止应用即作废，
 * 再次启动是全新 store。其余导出（storeToRefs / createPinia / ...）原样透传。
 */

import { scopedDefineStore } from './app-run-scope'

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
  const vueI18n = await import('vue-i18n')
  const piniaModule = await import('pinia')
  // defineStore → 运行期作用域包装版（其余导出透传）；插件 store 因此可在停用时
  // 整体作废、再次启动得到初始 state（模块被 ESM 缓存不重求值也不受影响）
  const scopedPiniaModule: Record<string, unknown> = Object.assign({}, piniaModule, {
    defineStore: scopedDefineStore,
  })
  ;(window as any)[SHARED_KEY] = {
    vue,
    'vue-i18n': vueI18n,
    pinia: scopedPiniaModule,
    i18n,
    router,
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
