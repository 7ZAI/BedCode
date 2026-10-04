/**
 * 页面过渡统一性锁
 *
 * 背景：本次收敛前，桌面端「整视图 / 分区切换」散落着 5 套各自为政的过渡
 * （宿主 `page` 0.2s/8px、agent-hub `ah-page`、file-transfer `ft-page`、
 * terminal-session 三处同名 `tab-fade` 但时长各不相同、ai-chatbox `page-fade` 与
 * `view-slide`），且全部用 `mode="out-in"`。
 *
 * 两个缺陷：
 * 1. **黑屏 + 闪烁**：out-in 在「旧页退场完成」与「新页入场挂载」之间留出一帧以上
 *    的空容器窗口，露出 `--bg-page`——五套暗色主题下它全是近黑
 *    （#15130f / #0f172a / #101713 / #0b1620 / #1b1210）。
 * 2. **必然漂移**：同一份页面过渡抄在 6 个文件里，注释声称「与宿主保持一致」，
 *    实际时长已经各走各的；没有 token、没有门禁，改一处不会带另一处。
 *
 * 本用例把「统一」变成可执行门禁：命名唯一、无 out-in、容器必带 page-swap、
 * 效果切换点唯一且合法、CSS 变体齐全、时长 token 单一来源、退场层必须脱离文档流。
 *
 * 刻意不锁：`ft-swap`（file-transfer 表格区加载/错误/空态/表格之间的状态机交叉
 * 淡入）仍用 out-in —— 它是同区域状态切换而非页面过渡，120ms 是刻意压短的节奏，
 * 不属于本次统一范围。
 */

import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import type { PageTransitionEffect } from '@/utils/pageTransition'
import {
  PAGE_TRANSITION_EFFECTS,
  PAGE_TRANSITION_EFFECT,
  DEFAULT_PAGE_TRANSITION_EFFECT,
  applyPageTransitionEffect,
} from '@/utils/pageTransition'

const HOST_STYLE = 'src/style.css'
const SCAN_ROOTS = ['src', 'wasm-apps']
/** 已退役的页面级过渡名：任何一处再出现即视为统一性回退 */
const RETIRED_PAGE_TRANSITIONS = ['ah-page', 'ft-page', 'page-fade', 'view-slide', 'tab-fade']

/** 页面过渡的四条基础类：只允许在宿主 style.css 里定义 */
const PAGE_TRANSITION_CLASSES = [
  'page-enter-active',
  'page-leave-active',
  'page-enter-from',
  'page-leave-to',
]

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === '__tests__') {
      continue
    }
    const path = join(dir, entry.name)
    if (entry.isDirectory()) walk(path, out)
    else out.push(path)
  }
  return out
}

const ALL_FILES = SCAN_ROOTS.flatMap((root) => walk(root))
const VUE_FILES = ALL_FILES.filter((f) => f.endsWith('.vue'))
const STYLE_FILES = ALL_FILES.filter((f) => f.endsWith('.css'))
const read = (path: string): string => readFileSync(path, 'utf8')

const hostStyle = read(HOST_STYLE)

/**
 * `<template>` 块内容（注释里提到 `<Transition name="page">` 不算真实用法）。
 * 根模板可能带属性（`<template v-if="...">`），结束标签在 SFC 约定里位于行首
 * 列 0；两者任一写死都会让整个文件在这几条断言上**空转**（已被变异自检捕获：
 * `indexOf('<template>')` 对带属性的根模板返回 -1）。
 */
function templateOf(source: string): string {
  const start = source.search(/<template[\s>]/)
  if (start === -1) return ''
  const rest = source.slice(start)
  const end = rest.search(/^<\/template>/m)
  return end === -1 ? rest : rest.slice(0, end)
}

/** 剔除 HTML 注释，避免说明文字里的类名 / 过渡名被当成真实声明 */
function stripComments(template: string): string {
  return template.replace(/<!--[\s\S]*?-->/g, '')
}

/** 某文件模板里所有 `<Transition ...>` 开标签（含属性），已剔除注释 */
function transitionTags(source: string): string[] {
  return [...stripComments(templateOf(source)).matchAll(/<Transition\b[^>]*>/g)].map((m) => m[0])
}

describe('C-101 页面级过渡命名唯一', () => {
  it('宿主与四个 wasm 应用都不再出现已退役的页面级过渡名', () => {
    const hits: string[] = []
    for (const file of [...VUE_FILES, ...STYLE_FILES]) {
      const src = read(file)
      const template = stripComments(templateOf(src))
      for (const name of RETIRED_PAGE_TRANSITIONS) {
        if (template.includes(`<Transition name="${name}"`) || src.includes(`.${name}-enter-active`)) {
          hits.push(`${file} → ${name}`)
        }
      }
    }
    expect(hits, `页面级过渡名回退：${hits.join(', ')}`).toEqual([])
  })

  it('扫描面非空且覆盖宿主布局与四个 wasm 应用（防门禁自身空转）', () => {
    expect(VUE_FILES.length).toBeGreaterThan(20)
    for (const app of ['agent-hub', 'ai-chatbox', 'file-transfer', 'terminal-session']) {
      expect(
        VUE_FILES.some((f) => f.startsWith(join('wasm-apps', app))),
        `扫描面漏掉 wasm 应用 ${app}`,
      ).toBe(true)
    }
    expect(VUE_FILES).toContain(join('src', 'components', 'DesktopLayout.vue'))
  })
})

describe('C-102 页面级过渡不得用 mode="out-in"（黑屏/闪烁根因）', () => {
  it('没有 name="page" 的过渡声明 mode', () => {
    const hits: string[] = []
    for (const file of VUE_FILES) {
      for (const tag of transitionTags(read(file))) {
        if (/name="page"/.test(tag) && /\bmode=/.test(tag)) hits.push(`${file} → ${tag}`)
      }
    }
    expect(hits, `out-in 会留出空容器帧：${hits.join(', ')}`).toEqual([])
  })
})

describe('C-103 过渡容器必须带 page-swap（退场层的定位上下文）', () => {
  it('用 name="page" 的文件都在过渡之前声明了 page-swap 容器', () => {
    const bad: string[] = []
    for (const file of VUE_FILES) {
      const template = stripComments(templateOf(read(file)))
      const idx = template.indexOf('<Transition name="page"')
      if (idx === -1) continue
      if (!template.slice(0, idx).includes('page-swap')) bad.push(file)
    }
    expect(bad, `过渡缺少 page-swap 容器：${bad.join(', ')}`).toEqual([])
  })

  it('DesktopLayout 的 <main> 带 page-swap（router-view 隔了一层的唯一站点）', () => {
    const layout = read(join('src', 'components', 'DesktopLayout.vue'))
    const mainTag = layout.match(/<main\b[^>]*>/)?.[0] ?? ''
    expect(mainTag).toContain('page-swap')
  })
})

describe('C-104 效果切换点唯一且合法', () => {
  it('当前效果在已登记清单内', () => {
    expect(PAGE_TRANSITION_EFFECTS).toContain(PAGE_TRANSITION_EFFECT)
  })

  it('applyPageTransitionEffect 把效果写到根元素 data-page-fx', () => {
    const el = document.createElement('html')
    applyPageTransitionEffect(el)
    expect(el.dataset.pageFx).toBe(PAGE_TRANSITION_EFFECT)
  })

  it('未知效果名抛错，不静默回退（静默回退 = 过渡类全部落空的硬切）', () => {
    const el = document.createElement('html')
    expect(() => applyPageTransitionEffect(el, 'nope' as PageTransitionEffect)).toThrow(
      /unknown page transition effect "nope"/,
    )
    // 抛错时不得留下半截状态
    expect(el.dataset.pageFx).toBeUndefined()
  })

  it('缺省效果与 CSS 无 data-page-fx 时的兜底一致（纵向位移 = slide-up）', () => {
    expect(DEFAULT_PAGE_TRANSITION_EFFECT).toBe('slide-up')
    const base = hostStyle.slice(hostStyle.indexOf('.page-enter-from'))
    expect(base).toMatch(/\.page-enter-from[\s\S]{0,120}translateY\(var\(--motion-page-shift\)\)/)
  })
})

describe('C-105 CSS 侧效果变体齐全 + 无障碍降级', () => {
  it('每个已登记效果都有对应的 CSS 变体规则', () => {
    const missing = PAGE_TRANSITION_EFFECTS.filter(
      (fx) => !hostStyle.includes(`html[data-page-fx='${fx}']`),
    )
    expect(missing, `CSS 缺效果变体：${missing.join(', ')}`).toEqual([])
  })

  it('减弱动态下入场起点为完全可见、无位移（否则仍有 1 帧全透明黑闪）', () => {
    const block = hostStyle.slice(hostStyle.indexOf('@media (prefers-reduced-motion: reduce)'))
    expect(block).toMatch(/\.page-enter-from[\s\S]{0,200}opacity:\s*1\s*!important/)
    expect(block).toMatch(/\.page-enter-from[\s\S]{0,200}transform:\s*none\s*!important/)
  })

  it('退场层脱离文档流（黑屏/闪烁的修复本体：旧页不再占据文档流、新页立即占位）', () => {
    // 取布局专属块：以「出场层脱离文档流」注释为锚（第一个 `.page-leave-active`
    // 是与 `.page-enter-active` 共用的 transition 规则，第三个是 reduced-motion
    // 覆盖块，均不含 position）
    const anchor = hostStyle.indexOf('出场层脱离文档流')
    const rule = hostStyle.slice(anchor)
    expect(rule.slice(0, 800)).toMatch(/position:\s*absolute/)
  })
})

describe('C-106 时长/缓动 token 单一来源', () => {
  it('宿主 style.css 定义 --motion-page-* 全套 token', () => {
    for (const token of [
      '--motion-page-duration-in',
      '--motion-page-duration-out',
      '--motion-page-ease',
      '--motion-page-shift',
      '--motion-page-scale',
    ]) {
      expect(hostStyle, `缺 token ${token}`).toContain(`${token}:`)
    }
  })

  it('页面过渡四条基础类只在宿主 style.css 里定义（任何第二处定义即视为漂移源）', () => {
    const offenders: string[] = []
    for (const file of [...VUE_FILES, ...STYLE_FILES]) {
      if (file === HOST_STYLE) continue
      const src = read(file)
      for (const cls of PAGE_TRANSITION_CLASSES) {
        if (src.includes(`.${cls}`)) offenders.push(`${file} → .${cls}`)
      }
    }
    expect(offenders, `页面过渡类被二次定义：${offenders.join(', ')}`).toEqual([])
  })
})
