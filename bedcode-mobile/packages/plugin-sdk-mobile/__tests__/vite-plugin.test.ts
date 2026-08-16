/**
 * SDK Vite 插件单测
 *
 * 移动端 bedcodePlugin() 返回两个插件：
 * 1. bedcode-shared-modules — 共享模块外置 + import 改写（与桌面端同实现）
 * 2. bedcode-inline-plugin-css — lib 模式下把 CSS asset 内联进入口 chunk
 *    （宿主只 import() 入口 JS，不加载 dist/style.css）
 */
import { describe, it, expect } from 'vitest'
import type { Plugin } from 'vite'
import { bedcodePlugin } from '../src/vite-plugin'

/** 按 name 取出插件实例 */
function pluginByName(name: string): Plugin {
  const found = bedcodePlugin().find((p) => p.name === name)
  if (!found) throw new Error(`plugin ${name} not found`)
  return found
}

/** 调用插件实例上的指定 hook */
function invokeHook(plugin: Plugin, name: string, ...args: any[]): any {
  const fn = (plugin as any)[name]
  if (typeof fn !== 'function') throw new Error(`hook ${name} not a function`)
  return fn.apply(plugin, args)
}

describe('bedcodePlugin 结构', () => {
  it('返回两个插件：共享模块外置 + CSS 内联', () => {
    const plugins = bedcodePlugin()
    expect(plugins.map((p) => p.name)).toEqual([
      'bedcode-shared-modules',
      'bedcode-inline-plugin-css',
    ])
  })

  it('CSS 内联插件只在 build 阶段生效且 enforce post', () => {
    const css = pluginByName('bedcode-inline-plugin-css')
    expect(css.apply).toBe('build')
    expect(css.enforce).toBe('post')
  })
})

describe('bedcode-shared-modules：config hook', () => {
  it('无既有 external → 注入三个共享模块', () => {
    const result = invokeHook(pluginByName('bedcode-shared-modules'), 'config', {})
    expect(result.build.rollupOptions.external).toEqual(['vue', 'vue-i18n', 'pinia'])
  })

  it('既有数组 external → 追加共享模块（不覆盖）', () => {
    const result = invokeHook(pluginByName('bedcode-shared-modules'), 'config', {
      build: { rollupOptions: { external: ['foo'] } },
    })
    expect(result.build.rollupOptions.external).toEqual(['foo', 'vue', 'vue-i18n', 'pinia'])
  })
})

describe('bedcode-shared-modules：renderChunk import 改写', () => {
  const shared = pluginByName('bedcode-shared-modules')

  it('default import → const 声明读取全局', () => {
    const code = `import Vue from 'vue'\nconsole.log(Vue)`
    const result = invokeHook(shared, 'renderChunk', code, {})
    expect(result.code).toBe('const Vue = window.__BEDCODE_SHARED__["vue"]\nconsole.log(Vue)')
  })

  it('named import → 解构 const（保留花括号内原始空白）', () => {
    const code = `import { ref, computed } from 'vue'\nref(1)`
    const result = invokeHook(shared, 'renderChunk', code, {})
    expect(result.code).toBe(
      'const {  ref, computed  } = window.__BEDCODE_SHARED__["vue"]\nref(1)'
    )
  })

  it('namespace import → const 声明', () => {
    const code = `import * as Pinia from 'pinia'`
    const result = invokeHook(shared, 'renderChunk', code, {})
    expect(result.code).toBe('const Pinia = window.__BEDCODE_SHARED__["pinia"]')
  })

  it('无共享模块 import → 返回 null', () => {
    expect(invokeHook(shared, 'renderChunk', `import x from './local'`, {})).toBeNull()
  })
})

describe('bedcode-inline-plugin-css：generateBundle', () => {
  const css = pluginByName('bedcode-inline-plugin-css')

  it('入口 chunk + CSS asset → style 注入入口头部并删除 CSS 文件', () => {
    const bundle: Record<string, any> = {
      'index.js': { type: 'chunk', isEntry: true, code: 'console.log(1)' },
      'style.css': { type: 'asset', source: '.a{color:red}' },
    }
    invokeHook(css, 'generateBundle', {}, bundle)

    expect(bundle['style.css']).toBeUndefined()
    expect(bundle['index.js'].code).toContain("document.createElement('style')")
    expect(bundle['index.js'].code).toContain("data-bedcode-plugin-css")
    expect(bundle['index.js'].code).toContain('.a{color:red}')
    // 注入代码在入口 JS 之前
    expect(bundle['index.js'].code.endsWith("console.log(1)")).toBe(true)
  })

  it('CSS source 为 Uint8Array → 按 UTF-8 解码后内联', () => {
    const bundle: Record<string, any> = {
      'index.js': { type: 'chunk', isEntry: true, code: 'x' },
      'a.css': { type: 'asset', source: new TextEncoder().encode('.x{width:1px}') },
    }
    invokeHook(css, 'generateBundle', {}, bundle)
    expect(bundle['index.js'].code).toContain('.x{width:1px}')
  })

  it('多个 CSS 文件 → 换行拼接后一次注入', () => {
    const bundle: Record<string, any> = {
      'index.js': { type: 'chunk', isEntry: true, code: 'x' },
      'a.css': { type: 'asset', source: '.a{}' },
      'b.css': { type: 'asset', source: '.b{}' },
    }
    invokeHook(css, 'generateBundle', {}, bundle)
    // CSS 经 JSON.stringify 注入，换行被转义为字面 \n（浏览器解析时还原）
    expect(bundle['index.js'].code).toContain('.a{}')
    expect(bundle['index.js'].code).toContain('.b{}')
    // 拼接顺序：a 在 b 之前
    expect(bundle['index.js'].code.indexOf('.a{}')).toBeLessThan(
      bundle['index.js'].code.indexOf('.b{}')
    )
    expect(bundle['a.css']).toBeUndefined()
    expect(bundle['b.css']).toBeUndefined()
  })

  it('无入口 chunk → 不改动 bundle', () => {
    const bundle: Record<string, any> = {
      'vendor.js': { type: 'chunk', isEntry: false, code: 'x' },
      'style.css': { type: 'asset', source: '.a{}' },
    }
    invokeHook(css, 'generateBundle', {}, bundle)
    expect(bundle['style.css']).toBeDefined()
    expect(bundle['vendor.js'].code).toBe('x')
  })

  it('无 CSS asset → 不改动入口代码', () => {
    const bundle: Record<string, any> = {
      'index.js': { type: 'chunk', isEntry: true, code: 'console.log(1)' },
    }
    invokeHook(css, 'generateBundle', {}, bundle)
    expect(bundle['index.js'].code).toBe('console.log(1)')
  })
})
