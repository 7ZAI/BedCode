/**
 * SDK 运行时代理单测：共享模块获取与 PluginContext 注入
 *
 * 宿主把共享模块挂到 window.__BEDCODE_SHARED__，插件经 runtime 代理读取，
 * 避免插件直接依赖全局变量名。node 环境无 window，测试前手工注入。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import {
  getSharedModule,
  getI18n,
  getVue,
  getVueI18n,
  getPinia,
  getRouter,
  getPluginContext,
} from '../src/runtime'

const SHARED_KEY = '__BEDCODE_SHARED__'

/** 当前用例注入的共享模块表，afterEach 时随 window 一起清除 */
let shared: Record<string, any>

beforeEach(() => {
  shared = {}
  ;(globalThis as any).window = { [SHARED_KEY]: shared }
})

afterEach(() => {
  delete (globalThis as any).window
})

describe('getSharedModule', () => {
  it('shared runtime 未初始化（window 上无共享对象）→ 抛错', () => {
    ;(globalThis as any).window = {}
    expect(() => getSharedModule('vue')).toThrow(/Shared runtime not initialized/)
  })

  it('请求不存在的模块 → 抛错并带模块名', () => {
    expect(() => getSharedModule('nonexistent')).toThrow(
      /Shared module "nonexistent" not found/
    )
  })

  it('存在的模块 → 原样返回宿主对象（不拷贝）', () => {
    const vue = { version: '3.5.0' }
    shared.vue = vue
    expect(getSharedModule('vue')).toBe(vue)
  })
})

describe('共享模块代理函数', () => {
  it('getVue / getI18n / getVueI18n / getPinia / getRouter 各取对应键', () => {
    const vue = { createApp: () => {} }
    const i18n = { t: () => 'x' }
    const vueI18n = { createI18n: () => {} }
    const pinia = { defineStore: () => {} }
    const router = { push: () => {} }
    shared.vue = vue
    shared.i18n = i18n
    shared['vue-i18n'] = vueI18n
    shared.pinia = pinia
    shared.router = router

    expect(getVue()).toBe(vue)
    expect(getI18n()).toBe(i18n)
    expect(getVueI18n()).toBe(vueI18n)
    expect(getPinia()).toBe(pinia)
    expect(getRouter()).toBe(router)
  })
})

describe('getPluginContext', () => {
  it('组件在 PluginViewHost 内渲染 → 返回 vue inject 的 context', () => {
    const context = { id: 'test-plugin' }
    shared.vue = { inject: (key: string) => (key === 'pluginContext' ? context : null) }
    expect(getPluginContext()).toBe(context)
  })

  it('vue 共享模块缺失 → 抛错（走 getVue 的模块级错误）', () => {
    expect(() => getPluginContext()).toThrow()
  })

  it('inject 返回空 → 抛错提示需在 PluginViewHost 内渲染', () => {
    shared.vue = { inject: () => null }
    expect(() => getPluginContext()).toThrow(/PluginContext not available/)
  })
})
