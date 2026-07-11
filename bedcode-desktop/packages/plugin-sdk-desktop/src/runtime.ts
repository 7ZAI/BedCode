/**
 * @bedcode/plugin-sdk-desktop 运行时代理
 *
 * 插件通过这些函数访问宿主共享模块，避免直接依赖 window 全局变量
 */
import type { PluginContext, I18nAPI } from './types'

/** 共享模块全局变量键名 */
const SHARED_KEY = '__BEDCODE_SHARED__'

/** 获取宿主共享模块 */
export function getSharedModule(name: string): any {
  const shared = (window as any)[SHARED_KEY]
  if (!shared) throw new Error('[PluginSDK] Shared runtime not initialized')
  const mod = shared[name]
  if (!mod) throw new Error(`[PluginSDK] Shared module "${name}" not found`)
  return mod
}

/** 获取宿主 i18n 实例（模块级代码使用，组件内请用 useI18n()） */
export function getI18n(): any {
  return getSharedModule('i18n')
}

/** 获取宿主 Vue 实例 */
export function getVue(): any {
  return getSharedModule('vue')
}

/** 获取宿主 vue-i18n 模块 */
export function getVueI18n(): any {
  return getSharedModule('vue-i18n')
}

/** 获取宿主 Pinia 模块 */
export function getPinia(): any {
  return getSharedModule('pinia')
}

/** 获取宿主 Router 实例 */
export function getRouter(): any {
  return getSharedModule('router')
}

/** 从 Vue inject 获取 PluginContext（组件 setup 阶段调用） */
export function getPluginContext(): PluginContext {
  const vue = getVue()
  const context = vue.inject('pluginContext')
  if (!context) throw new Error('[PluginSDK] PluginContext not available — ensure component is rendered inside PluginViewHost')
  return context
}
