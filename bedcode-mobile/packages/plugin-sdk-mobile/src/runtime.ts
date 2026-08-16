/**
 * @binblink/plugin-sdk-mobile 运行时代理
 */
import type { PluginContext } from './types'

const SHARED_KEY = '__BEDCODE_SHARED__'

/** 获取宿主共享模块 */
export function getSharedModule(name: string): any {
  const shared = (window as any)[SHARED_KEY]
  if (!shared) throw new Error('[PluginSDK] Shared runtime not initialized')
  const mod = shared[name]
  if (!mod) throw new Error(`[PluginSDK] Shared module "${name}" not found`)
  return mod
}

/** 获取宿主 i18n 实例 */
export function getI18n(): any { return getSharedModule('i18n') }

/** 获取宿主 Vue 实例 */
export function getVue(): any { return getSharedModule('vue') }

/** 获取宿主 vue-i18n 模块 */
export function getVueI18n(): any { return getSharedModule('vue-i18n') }

/** 获取宿主 Pinia 模块 */
export function getPinia(): any { return getSharedModule('pinia') }

/** 获取宿主 Router 实例 */
export function getRouter(): any { return getSharedModule('router') }

/** 获取宿主预设任务 composable */
export function getPresetTasks(): any { return getSharedModule('presetTasks') }

/** 获取宿主移动端连接/HTTP 能力（MobileHostApi） */
export function getMobileApi(): any { return getSharedModule('mobileApi') }

/** 从 Vue inject 获取 PluginContext */
export function getPluginContext(): PluginContext {
  const vue = getVue()
  const context = vue.inject('pluginContext')
  if (!context) throw new Error('[PluginSDK] PluginContext not available')
  return context
}
