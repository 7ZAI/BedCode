/**
 * 桌面开屏浅色模式锁（2026-09-24 立）
 *
 * 背景：`SplashLoading.vue` 的开屏色原先是一组**固定深色品牌常量**，注释写明
 * 「不随主题切换」，于是浅色模式下整条开屏链路（原生窗口底色 → index.html 静态首屏 →
 * 品牌开屏）只有深色一套。改成双主题后，风险从「没人记得要适配」变成
 * 「后续改样式时又塞回一个硬编码深色字面值」——这类回退在浅色下表现为局部脏色，
 * 深色下完全正常，肉眼与既有 DOM 用例都测不出来（happy-dom 不解析 scoped CSS 变量）。
 *
 * 本用例把四条不变量钉成门禁：
 * - C-01 浅色基块与 `html.dark` 深色覆盖块都在（缺一套 = 该主题下变量无定义）
 * - C-02 两块 `--splash-*` 变量集合完全一致（漏一个 = 另一主题沿用错色）
 * - C-03 色板块之外的规则只引用变量，不出现字面色值（token-bound）
 * - C-04 index.html 静态首屏有 prefers-color-scheme 浅色分支，且镜像的正是
 *        `style.css` 里 warm 浅色 token 的字面值（token 改了没同步镜像 → 转红）
 */

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'

const SPLASH_COMPONENT = 'src/components/SplashLoading.vue'
const INDEX_HTML = 'index.html'
const STYLE_CSS = 'src/style.css'

/** 取组件 `<style scoped>` 块正文，并剥掉 CSS 注释（注释里的示例色值不参与判定） */
function splashStyleBlock(): string {
  const sfc = readFileSync(SPLASH_COMPONENT, 'utf-8')
  const style = /<style[^>]*>([\s\S]*?)<\/style>/.exec(sfc)?.[1]
  if (!style) throw new Error(`${SPLASH_COMPONENT} 未找到 <style> 块`)
  return style.replace(/\/\*[\s\S]*?\*\//g, '')
}

/** 色板块正文：`(^|\n)` 锚定行首，避免把引用处（如 background 里的 var）误当成块 */
function paletteBlock(css: string, blockRe: RegExp): string {
  // 捕获组约定：1 = 行首换行、2 = 选择器、3 = 块正文
  const body = blockRe.exec(css)?.[3]
  expect(body, `未找到色板块：${blockRe}`).toBeTruthy()
  return body as string
}

/** 块内定义的开屏变量名集合（只认 `--splash-*:` 声明，不认引用） */
function declaredTokens(block: string): string[] {
  return [...block.matchAll(/(--splash-[\w-]+)\s*:/g)].map((m) => m[1]).sort()
}

const LIGHT_BLOCK = /(^|\n)(\.splash-root)\s*\{([^}]*)\}/
const DARK_BLOCK = /(^|\n)(html\.dark \.splash-root)\s*\{([^}]*)\}/

describe('SplashLoading 浅色/深色色板对称锁', () => {
  it('C-01 浅色基块与 html.dark 深色覆盖块都存在', () => {
    const css = splashStyleBlock()
    expect(LIGHT_BLOCK.test(css)).toBe(true)
    expect(DARK_BLOCK.test(css)).toBe(true)
  })

  it('C-02 两套色板定义的 --splash-* 变量集合完全一致', () => {
    const css = splashStyleBlock()
    const lightTokens = declaredTokens(paletteBlock(css, LIGHT_BLOCK))
    const darkTokens = declaredTokens(paletteBlock(css, DARK_BLOCK))

    // 非空守卫：两块都空时集合也「相等」，会让本用例恒真通过
    expect(lightTokens.length).toBeGreaterThan(5)
    expect(darkTokens).toEqual(lightTokens)
  })

  it('C-03 色板块之外的规则不出现字面色值（只引用 var()/token）', () => {
    const css = splashStyleBlock()
    const outsidePalettes = css.replace(/(^|\n)(?:html\.dark )?\.splash-root\s*\{[^}]*\}/g, '')

    const literals = [
      ...[...outsidePalettes.matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((m) => m[0]),
      ...[...outsidePalettes.matchAll(/\b(?:rgb|rgba|hsl|hsla)\(/g)].map((m) => m[0]),
    ]
    // 深色品牌色（#ece8dc / rgba(236,232,220,…) 等）一旦漏进规则，浅色下就是脏色
    expect(literals).toEqual([])
  })
})

describe('index.html 静态首屏浅色锁', () => {
  /** style.css 里 warm 浅色 token 的字面值：`:root` 是文件首个声明块，exec 取到的即默认色板 */
  function lightToken(name: string): string {
    const value = new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{3,8})`).exec(
      readFileSync(STYLE_CSS, 'utf-8'),
    )?.[1]
    expect(value, `style.css 缺少浅色 token --${name}`).toBeTruthy()
    return value as string
  }

  it('C-04 静态首屏浅色分支存在，且色值镜像 style.css 的 warm 浅色 token', () => {
    const html = readFileSync(INDEX_HTML, 'utf-8')
    const markerAt = html.indexOf('@media (prefers-color-scheme: light)')
    expect(markerAt, 'index.html 静态首屏缺少浅色分支').toBeGreaterThan(-1)
    const lightBranch = html.slice(markerAt)

    expect(lightBranch).toMatch(
      new RegExp(`\\.splash\\s*\\{[^}]*background:[^}]*${lightToken('bg-page')}[^}]*\\}`),
    )
    expect(lightBranch).toMatch(
      new RegExp(`\\.splash\\s*\\{[^}]*${lightToken('bg-sidebar')}[^}]*\\}`),
    )
    expect(lightBranch).toMatch(
      new RegExp(`\\.spinner\\s*\\{[^}]*border-top-color:\\s*${lightToken('text-primary')}`),
    )
  })
})
