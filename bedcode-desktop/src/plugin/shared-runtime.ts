/**
 * 插件共享模块运行时
 *
 * 统一初始化 window.__BEDCODE_SHARED__，供插件通过 @bedcode/plugin-sdk-desktop/runtime 访问
 * 必须在 app.mount() 之前调用
 */
import * as Vue from 'vue'
import * as VueI18n from 'vue-i18n'
import * as Pinia from 'pinia'
import type { Router } from 'vue-router'

/** 初始化共享模块全局变量 */
export function setupSharedRuntime(i18n: any, router: Router): void {
  ;(window as any).__BEDCODE_SHARED__ = {
    vue: Vue,
    'vue-i18n': VueI18n,
    pinia: Pinia,
    i18n,
    router,
  }
}
