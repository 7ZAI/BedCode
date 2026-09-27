/**
 * Agent Hub 样式护栏（票 09 / 13 / 14 / 15）
 *
 * 前景色一律**从 `src/styles.css` 实际解析**（新增/改动样式自动纳入判定，
 * 不手写常量），主题 token 从宿主 `src/style.css` 按真实层叠合成
 * （6 套 palette × 明暗 = 12 套主题）。承载面是结构信息（文字压在哪个面上），
 * 由 AUDITED 显式登记。
 *
 * 契约：
 * - S1 每个受审文字组合在 12 套主题下都 ≥ WCAG AA（4.5:1；≥18px 或 ≥14px+bold 记 3.0:1）
 * - S2 无文本图形（语义状态圆点 / 图表双段）≥ 3:1（WCAG 1.4.11）
 * - S3 图表双段互相可区分（ΔE ≥ 25）且不与语义三色（ΔE ≥ 25）、
 *   不与色板 primary 撞色（ΔE ≥ 20）
 * - S4 豁免项必须写明理由，且理由非空（不留「静默不过」）
 * - S5 模板里用到的每个 `ah-*` 类都能在 styles.css 找到规则（死类/漏样式守门）
 * - S6 插件样式里不再出现 `--text-secondary` / `--color-warning` 直接当文字色
 * - S7 不写死第三方 hex 色板（`--chart-*` 与 Avatar 档位是显式登记的例外）
 * - S8 不再出现 `100vh` 魔数（弹窗 `max-height` 是唯一合法例外，因为它 Teleport 到 body）
 */

import { describe, it, expect } from 'vitest'
import { readFileSync, existsSync } from 'node:fs'
import { resolve } from 'node:path'
import {
  AGENT_HUB,
  AUDITED,
  gateFor,
  parseRules,
  runMatrix,
  runGraphicsMatrix,
  runChartPairCheck,
  templateClasses,
  selectorClassTokens,
} from './helpers/contrast'

const STYLES = readFileSync(resolve(AGENT_HUB, 'src/styles.css'), 'utf8')
const RULES = parseRules(STYLES)

const matrix = runMatrix()
const graphics = runGraphicsMatrix()
const chart = runChartPairCheck()

describe('S1 文字对比度矩阵（12 套主题取最差）', () => {
  it('受审清单非空且每个选择器都能在 styles.css 解析到 color（防清单腐化）', () => {
    expect(AUDITED.length).toBeGreaterThan(40)
    const missing = matrix.filter((r) => r.missing).map((r) => r.sel)
    expect(missing, `这些选择器在 styles.css 里没有 color 声明：${missing.join(', ')}`).toEqual([])
  })

  it.each(matrix.filter((r) => !r.missing && !r.exempt).map((r) => [r.sel, r] as const))(
    '%s 达到 WCAG AA',
    (_sel, r) => {
      expect(
        r.worst,
        `${r.sel} 最差对比度 ${r.worst.toFixed(2)}:1 @ ${r.worstTheme}，低于门禁 ${r.gate}:1（前景 ${r.fgExpr}）`,
      ).toBeGreaterThanOrEqual(r.gate)
    },
  )

  it('S4 豁免项必须写明理由', () => {
    for (const r of matrix.filter((x) => x.exempt)) {
      expect((r.exempt ?? '').length, `${r.sel} 的豁免理由为空`).toBeGreaterThan(20)
    }
  })

  it('门禁口径：≥18px 或 ≥14px+bold 才降到 3.0，其余一律 4.5', () => {
    expect(gateFor(11)).toBe(4.5)
    expect(gateFor(13, 700)).toBe(4.5)
    expect(gateFor(14, 700)).toBe(3.0)
    expect(gateFor(18)).toBe(3.0)
    expect(gateFor(24, 400)).toBe(3.0)
  })
})

describe('S2 无文本图形对比度（WCAG 1.4.11）', () => {
  it('图形清单完整（每个选择器都能解析到 background）', () => {
    expect(graphics.filter((g) => g.missing).map((g) => g.sel)).toEqual([])
  })

  it.each(graphics.map((g) => [g.sel, g] as const))('%s 在承载面上 ≥ 3:1', (_sel, g) => {
    expect(g.worst, `${g.sel} 最差 ${g.worst.toFixed(2)}:1 @ ${g.worstTheme}`).toBeGreaterThanOrEqual(3)
  })
})

describe('S3 图表双段可区分度', () => {
  it('两段 ΔE(CIE76) ≥ 25', () => {
    expect(chart.worstPair, `最差 ΔE ${chart.worstPair.toFixed(1)} @ ${chart.worstPairTheme}`).toBeGreaterThanOrEqual(25)
  })

  it('与语义三色 ΔE ≥ 25（输出段不得读成「警告色」）', () => {
    expect(chart.worstSem, `最差 ΔE ${chart.worstSem.toFixed(1)} @ ${chart.worstSemWhere}`).toBeGreaterThanOrEqual(25)
  })

  it('与色板 primary ΔE ≥ 20', () => {
    expect(chart.worstPri, `最差 ΔE ${chart.worstPri.toFixed(1)} @ ${chart.worstPriWhere}`).toBeGreaterThanOrEqual(20)
  })
})

describe('S5 模板类名与样式一致（死类 / 漏样式守门）', () => {
  const { used, prefixes } = templateClasses()
  const defined = new Set(RULES.flatMap((r) => selectorClassTokens(r.selector)))

  it('每个 ah-* 类都能在 styles.css 找到规则', () => {
    // 排除 <Transition name="ah-page"> 这类只作前缀的标记（其派生类另由下一条校验）
    const orphans = [...used].filter(
      (c) => c.startsWith('ah-') && !defined.has(c) && !prefixes.some((p) => c.startsWith(p)),
    )
    expect(orphans, `模板在用但 styles.css 无规则：${orphans.join(', ')}`).toEqual([])
  })

  it('动态类名前缀（Transition name / 模板串）都有对应规则', () => {
    const uncovered = prefixes.filter((p) => ![...defined].some((d) => d.startsWith(p)))
    expect(uncovered, `动态类名前缀无对应规则：${uncovered.join(', ')}`).toEqual([])
  })

  it('反向：单一类名规则不得零引用（死规则守门）', () => {
    // 只查「单一类名」规则：复合/后代规则（.ah-x.y、.a .b）由 :class 绑定
    // 与结构派生，逐一比对会产生假阳性
    const simple = RULES.filter((r) => {
      const tokens = selectorClassTokens(r.selector)
      return tokens.length === 1 && r.selector.trim() === `.${tokens[0]}`
    }).map((r) => selectorClassTokens(r.selector)[0])
    const dead = simple.filter((c) => !used.has(c) && !prefixes.some((p) => c.startsWith(p)))
    expect(dead, `styles.css 有零引用规则：${dead.join(', ')}`).toEqual([])
  })
})

describe('S6 语义 token 不直接当文字色', () => {
  /** 只看声明（剔除注释），且只匹配独立的 color: 属性（border-color: 不算） */
  const DECLS = STYLES.replace(/\/\*[\s\S]*?\*\//g, '')
  const colorProp = (token: string) => new RegExp(String.raw`(?:^|[;{}\n])\s*color:\s*var\(${token}\)`, 'm')

  it('不再有 color: var(--text-secondary)（数据级统一走 --ah-text-data）', () => {
    expect(DECLS).not.toMatch(colorProp('--text-secondary'))
  })

  it('不再有 color: var(--color-warning)（警告色不得当文字/数据色）', () => {
    expect(DECLS).not.toMatch(colorProp('--color-warning'))
  })

  it('不再有 color: var(--color-success) / var(--color-danger) 直接当文字色', () => {
    expect(DECLS).not.toMatch(colorProp('--color-success'))
    expect(DECLS).not.toMatch(colorProp('--color-danger'))
  })

  it('--ah-text-data 派生自 --text-secondary（数据级可读 token 的唯一来源）', () => {
    expect(STYLES).toMatch(/--ah-text-data:\s*color-mix\(in srgb, var\(--text-secondary\)/)
  })
})

describe('S7 无写死第三方色板', () => {
  /** 允许写死的：--chart-* 数据色、Avatar 档位、品牌 chip 底色（注释已说明理由） */
  function hardcodedColors(): string[] {
    const body = STYLES.replace(/\/\*[\s\S]*?\*\//g, '')
    const hits: string[] = []
    for (const m of body.matchAll(/#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)/g)) hits.push(m[0])
    return hits
  }

  it('写死色值仅限已登记的例外（chart 双段 + 品牌 chip 底色）', () => {
    const allowed = new Set([
      '#6f5b3d',
      '#3b3b60',
      '#83835a',
      '#7c7ca2',
      'rgba(217, 119, 87, 0.16)',
      'rgba(245, 158, 11, 0.16)',
      'rgba(0, 0, 0, 0.5)', // 弹窗遮罩（宿主 Modal 同款）
    ])
    const unexpected = hardcodedColors().filter((c) => !allowed.has(c))
    expect(unexpected, `出现未登记的写死色值：${unexpected.join(', ')}`).toEqual([])
  })

  it('AgentIcon 不再内联 hex 渐变色板', () => {
    const src = readFileSync(resolve(AGENT_HUB, 'src/components/AgentIcon.vue'), 'utf8')
    expect(src).not.toMatch(/linear-gradient\(/)
    expect(src).not.toMatch(/#[0-9a-fA-F]{6}/)
    expect(src).toMatch(/var\(--ah-avatar-/)
  })
})

describe('S8 无 100vh 魔数', () => {
  it('styles.css 里的 100vh 只剩弹窗面板的 max-height（Teleport 到 body，不在 flex 链内）', () => {
    const body = STYLES.replace(/\/\*[\s\S]*?\*\//g, '')
    const uses = [...body.matchAll(/[^m]100vh/g)]
    expect(uses.length, `styles.css 仍有多处 100vh：${uses.length} 处`).toBeLessThanOrEqual(1)
    expect(body).toMatch(/\.ah-modal-panel\s*\{[^}]*max-height:\s*calc\(100vh/)
  })
})

describe('S9 死代码清理（票 14）', () => {
  it('TabPlaceholder 组件与 hub.placeholder.* i18n 键已删除', () => {
    expect(existsSync(resolve(AGENT_HUB, 'components/TabPlaceholder.vue'))).toBe(false)
    for (const f of ['i18n/messages.ts', 'i18n/zh-CN.ts', 'i18n/en.ts']) {
      expect(readFileSync(resolve(AGENT_HUB, 'src', f), 'utf8'), `${f} 仍有 hub.placeholder 键`).not.toContain(
        'hub.placeholder',
      )
    }
  })

  it('.ah-input 只剩一处定义（票 12：两处冲突定义已合并）', () => {
    expect((STYLES.match(/^\.ah-input \{/gm) ?? []).length).toBe(1)
    expect(STYLES).toMatch(/\.ah-input \{[^}]*height: var\(--input-height\)/)
  })

  it('.ah-sk-url 第三套输入规格已并入 .ah-input', () => {
    expect(STYLES).not.toMatch(/^\.ah-sk-url \{/m)
  })

  it('日期选择器主题覆盖与 .ah-input 同一规格', () => {
    const index = readFileSync(resolve(AGENT_HUB, 'src/index.ts'), 'utf8')
    expect(index).toMatch(/\.dp__input \{[^}]*height: var\(--input-height\)/)
    expect(index).not.toMatch(/\.dp__input \{[^}]*height: 32px/)
  })
})
