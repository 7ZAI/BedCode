/**
 * SDK Vite 插件单测：共享模块外部化 + import 改写
 *
 * 构建期把 vue/vue-i18n/pinia 的 import 语句改写为读取
 * window.__BEDCODE_SHARED__ 的 const 声明，保证插件与宿主共用
 * 同一 Vue 实例。直接调用 hook 函数，不启动 Vite 容器。
 */
import { describe, it, expect } from 'vitest'
import type { Plugin } from 'vite'
import { bedcodePlugin } from '../src/vite-plugin'

/** 调用插件实例上的指定 hook（renderChunk/generateBundle 类） */
function invokeHook(name: string, ...args: any[]): any {
  const plugin = bedcodePlugin()
  const fn = (plugin as any)[name]
  if (typeof fn !== 'function') throw new Error(`hook ${name} not a function`)
  return fn.apply(plugin, args)
}

describe('config hook：rollup external 注入', () => {
  it('无既有 external → 注入三个共享模块', () => {
    const result = invokeHook('config', {})
    expect(result.build.rollupOptions.external).toEqual(['vue', 'vue-i18n', 'pinia'])
  })

  it('既有数组 external → 追加共享模块（不覆盖）', () => {
    const result = invokeHook('config', {
      build: { rollupOptions: { external: ['foo'] } },
    })
    expect(result.build.rollupOptions.external).toEqual(['foo', 'vue', 'vue-i18n', 'pinia'])
  })

  it('既有字符串 external → 转数组后追加', () => {
    const result = invokeHook('config', {
      build: { rollupOptions: { external: 'foo' } },
    })
    expect(result.build.rollupOptions.external).toEqual(['foo', 'vue', 'vue-i18n', 'pinia'])
  })
})

describe('renderChunk：共享模块 import 改写', () => {
  it('default import → const 声明读取全局', () => {
    const code = `import Vue from 'vue'\nconsole.log(Vue)`
    const result = invokeHook('renderChunk', code, {})
    expect(result.code).toBe('const Vue = window.__BEDCODE_SHARED__["vue"]\nconsole.log(Vue)')
  })

  it('named import → 解构 const（保留花括号内原始空白）', () => {
    const code = `import { ref, computed } from 'vue'\nref(1)`
    const result = invokeHook('renderChunk', code, {})
    expect(result.code).toBe(
      'const {  ref, computed  } = window.__BEDCODE_SHARED__["vue"]\nref(1)'
    )
  })

  it('namespace import → const 声明', () => {
    const code = `import * as VueI18n from 'vue-i18n'\nVueI18n.createI18n()`
    const result = invokeHook('renderChunk', code, {})
    expect(result.code).toBe(
      'const VueI18n = window.__BEDCODE_SHARED__["vue-i18n"]\nVueI18n.createI18n()'
    )
  })

  it('同文件多个共享模块 import 全部改写', () => {
    const code = `import Vue from 'vue'\nimport { createPinia } from 'pinia'`
    const result = invokeHook('renderChunk', code, {})
    expect(result.code).toBe(
      'const Vue = window.__BEDCODE_SHARED__["vue"]\n' +
        'const {  createPinia  } = window.__BEDCODE_SHARED__["pinia"]'
    )
  })

  it('非共享模块 import 保持原样', () => {
    const code = `import Vue from 'vue'\nimport local from './local'`
    const result = invokeHook('renderChunk', code, {})
    expect(result.code).toContain(`import local from './local'`)
  })

  it('无共享模块 import → 返回 null（不破坏 chunk）', () => {
    const code = `import x from './local'\nconsole.log(x)`
    expect(invokeHook('renderChunk', code, {})).toBeNull()
  })

  it('改写后附带 sourcemap', () => {
    const result = invokeHook('renderChunk', `import Vue from 'vue'`, {})
    expect(result.map).toBeTruthy()
    expect(result.map.mappings.length).toBeGreaterThan(0)
  })
})
