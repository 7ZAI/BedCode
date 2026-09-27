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
  runHeatRampCheck,
  templateClasses,
  selectorClassTokens,
} from './helpers/contrast'

const STYLES = readFileSync(resolve(AGENT_HUB, 'src/styles.css'), 'utf8')
const RULES = parseRules(STYLES)

const matrix = runMatrix()
const graphics = runGraphicsMatrix()
const chart = runChartPairCheck()
const heat = runHeatRampCheck()

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

  it.each(
    graphics
      .filter((g) => !g.exempt)
      .map((g) => [g.sel, g] as const),
  )('%s 在承载面上 ≥ 3:1', (_sel, g) => {
    expect(g.worst, `${g.sel} 最差 ${g.worst.toFixed(2)}:1 @ ${g.worstTheme}`).toBeGreaterThanOrEqual(3)
  })

  it('豁免项必须写明理由（S4 同口径：不留「静默不过」）', () => {
    for (const g of graphics.filter((x) => x.exempt)) {
      expect((g.exempt ?? '').length, `${g.sel} 的豁免理由为空`).toBeGreaterThan(20)
    }
  })

  it('热力图色阶的豁免只限低两档（顶档必须硬过 3:1）', () => {
    const exemptHeat = graphics
      .filter((g) => g.sel.startsWith('.ah-heat-cell.lv') && g.exempt)
      .map((g) => g.sel)
      .sort()
    expect(exemptHeat).toEqual(['.ah-heat-cell.lv1', '.ah-heat-cell.lv2'])
  })
})

describe('S3 分类色组可区分度（4 槽：输入 / 输出 / 缓存读 / 缓存写）', () => {
  it('槽位清单就是 4 个（新增分类必须同步登记）', () => {
    expect(chart.slots).toEqual(['--chart-c1', '--chart-c2', '--chart-c3', '--chart-c4'])
  })

  it('任意两槽 ΔE(CIE76) ≥ 25（全组两两，不是只比相邻）', () => {
    expect(chart.worstPair, `最差 ΔE ${chart.worstPair.toFixed(1)} @ ${chart.worstPairTheme}`).toBeGreaterThanOrEqual(25)
  })

  it('与语义三色 ΔE ≥ 25（扇区不得读成「警告色/成功色」）', () => {
    expect(chart.worstSem, `最差 ΔE ${chart.worstSem.toFixed(1)} @ ${chart.worstSemWhere}`).toBeGreaterThanOrEqual(25)
  })

  it('与色板 primary ΔE ≥ 20', () => {
    expect(chart.worstPri, `最差 ΔE ${chart.worstPri.toFixed(1)} @ ${chart.worstPriWhere}`).toBeGreaterThanOrEqual(20)
  })

  it('对承载面 --bg-card ≥ 3:1（扇区 / 图例块是内联绑色的，故在 token 层校验）', () => {
    expect(
      chart.worstCard,
      `最差 ${chart.worstCard.toFixed(2)}:1 @ ${chart.worstCardTheme}（${chart.worstCardSlot}）`,
    ).toBeGreaterThanOrEqual(3)
  })
})

describe('S3-5 热力图顺序标度（低档豁免的代价：标度本身必须成立）', () => {
  it('四档齐全', () => {
    expect(heat.steps).toEqual(['--chart-heat-1', '--chart-heat-2', '--chart-heat-3', '--chart-heat-4'])
  })

  it('亮度严格单调（读者才能把「更深 = 更多」记成一条规则）', () => {
    expect(heat.monotone, '存在主题内亮度非单调的档序，标度方向会读反').toBe(true)
  })

  it('顶档对 --bg-card ≥ 3:1（最重的那一格必须看得见）', () => {
    expect(heat.topWorst, `顶档最差 ${heat.topWorst.toFixed(2)}:1 @ ${heat.topWorstTheme}`).toBeGreaterThanOrEqual(3)
  })

  it('低两档确实低于 3:1（否则「低档豁免」就是无理由放宽）', () => {
    expect(heat.worstPerStep[0].worst, 'lv1 竟已达标，则该登记的豁免理由失效').toBeLessThan(3)
    expect(heat.worstPerStep[1].worst, 'lv2 竟已达标，则该登记的豁免理由失效').toBeLessThan(3)
  })

  it('低两档与空格轨道底仍可区分（否则低档会读成「无数据」）', () => {
    for (const step of heat.worstPerStep.slice(0, 2)) {
      expect(step.worst, `${step.step} 与卡片面对比 ${step.worst.toFixed(2)}:1，太接近空档`).toBeGreaterThan(1.1)
    }
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

  it('写死色值仅限已登记的例外（chart 分类色 + 热力色阶 + 品牌 chip 底色）', () => {
    const allowed = new Set([
      // --chart-c1..c4（分类色；c1/c2 同值于票 13 的 --chart-in/--chart-out）
      '#6f5b3d',
      '#3b3b60',
      '#509b69',
      '#bf69a2',
      '#83835a',
      '#7c7ca2',
      '#359756',
      '#d770b4',
      // --chart-heat-1..4（顺序标度）
      '#afa392',
      '#9a8b76',
      '#847359',
      '#55543b',
      '#646345',
      '#747350',
      'rgba(217, 119, 87, 0.16)',
      'rgba(245, 158, 11, 0.16)',
      'rgba(0, 0, 0, 0.5)', // 弹窗遮罩（宿主 Modal 同款）
      'rgb(0 0 0 / 0.04)', // 输入框常态阴影（= 宿主 tailwind.config 的 boxShadow.xs，--ah-ctl-shadow）
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
    expect(STYLES).toMatch(/\.ah-input \{[^}]*height: var\(--ah-ctl-height\)/)
  })
  it('.ah-sk-url 第三套输入规格已并入 .ah-input', () => {
    expect(STYLES).not.toMatch(/^\.ah-sk-url \{/m)
  })

  it('日期选择器主题覆盖与 .ah-input 引用同一批 --ah-ctl-* 规格 token', () => {
    const index = readFileSync(resolve(AGENT_HUB, 'src/index.ts'), 'utf8')
    expect(index).toMatch(/\.dp__input \{[^}]*height: var\(--ah-ctl-height\)/)
    expect(index).not.toMatch(/\.dp__input \{[^}]*height: 32px/)
  })
})

/** 取 .ah-input / .dp__input 覆盖块的声明体（跨文件同规格对比用） */
function declsOfBlock(css: string, selector: string): Record<string, string> {
  const hit = parseRules(css).filter((r) => r.selector === selector)
  expect(hit.length, `${selector} 没有规则块`).toBe(1)
  return hit[0].decls
}

describe('S12 同排表单控件共用一份规格（查询条件条：下拉 / 关键词 / 日期）', () => {
  // 回归成因：三类控件各写各的数值（关键词 12px 内边距 + base 字号、日期框
  // vendor 字体栈 + 12px 内边距 + 0.7 透明度占位），同排一眼就不齐。
  // 契约：三者逐属性引用同一批 --ah-ctl-* token，且 token 值对齐宿主真源。
  const INDEX = readFileSync(resolve(AGENT_HUB, 'src/index.ts'), 'utf8')
  const input = declsOf('.ah-input')
  const dp = declsOfBlock(INDEX, '.dp__input')

  it('高度 / 圆角 / 底色 / 前景色 / 字号 / 占位色 / 常态阴影：两侧同 token', () => {
    const pairs: [string, string][] = [
      ['height', 'height'],
      ['min-height', 'min-height'],
      ['border-radius', 'border-radius'],
      ['background', 'background'],
      ['color', 'color'],
      ['font-size', 'font-size'],
      ['box-shadow', 'box-shadow'],
    ]
    for (const [a, b] of pairs) {
      expect(dp[b], `.dp__input 缺 ${b}`).toBeTruthy()
      expect(dp[b], `${a}：.dp__input 与 .ah-input 不一致`).toBe(input[a])
    }
    // 边框：.ah-input 写简写，日期框覆盖的是 border-color
    expect(dp['border-color']).toBe('var(--ah-ctl-border-color)')
    expect(input.border).toBe('1px solid var(--ah-ctl-border-color)')
    // 占位色走各自的伪元素块
    const phInput = declsOf('.ah-input::placeholder')
    const phDp = declsOfBlock(INDEX, '.dp__input::placeholder')
    expect(phDp.color).toBe(phInput.color)
    expect(phDp.opacity, 'vendor 占位符 opacity: .7 比同排控件淡 30%').toBe('1')
  })

  /**
   * 高度下限：height 必须与 min-height 成对
   *
   * 实机 bug（2026-09-28，截图实测）：`.ah-input` 带 `flex: 1`（横向 flex 行里
   * 占满宽度），而查询条件条的字段容器是**列向** flex——flex-basis 作用于高度、
   * 容器高度又是 auto，高度被压成内容高：关键词框 20.8px，同排 Select / 日期框
   * 36px。声明层面看不出差别，只有渲染后才发现，故在此锁死 min-height。
   */
  it('两处控件都声明 min-height（列向 flex 里 flex:1 会把 height 压成内容高）', () => {
    for (const [name, decls] of [['ah-input', input], ['dp__input', dp]] as const) {
      expect(decls['min-height'], `${name} 缺 min-height（列向 flex 里高度会被压扁）`).toBe(
        'var(--ah-ctl-height)',
      )
    }
  })

  /**
   * token 用法合法性：长写属性不得引用简写值
   *
   * 实机 bug（同日）：`--ah-ctl-border: 1px solid var(--border-input)` 被同时用作
   * `border`（合法）与 `border-color`（**整条声明失效**）——日期框边框因此回退到
   * vendor 默认色（实测 rgb(236,232,220) = --border 浅色），与同排控件不是一个颜色。
   */
  it('长写属性不引用简写 token（border-color 拿到 1px solid 会整条失效）', () => {
    const tokenOf = (css: string, name: string) =>
      new RegExp(`${name}:\\s*([^;]+);`).exec(css)?.[1]?.trim() ?? ''
    const css = `${STYLES}\n${INDEX}`
    const tokens = [...STYLES.matchAll(/(--ah-ctl-[\w-]+):\s*([^;]+);/g)].map(
      (m) => [m[1], m[2].trim()] as const,
    )
    expect(tokens.length, '未解析到 --ah-ctl-* 族').toBeGreaterThan(6)
    // 颜色类长写：值里不得出现宽度/线型关键字（无论直接写还是经 token）
    const colorLonghands = ['border-color', 'background-color', 'color', 'outline-color']
    for (const rule of parseRules(css)) {
      for (const prop of colorLonghands) {
        const v = rule.decls[prop]
        if (!v) continue
        expect(
          v,
          `${rule.selector} { ${prop}: ${v} } —— 颜色长写里出现宽度/线型关键字，整条声明会失效并回退默认色`,
        ).not.toMatch(/\b(solid|dashed|dotted|double|px|em|rem)\b/)
        for (const [name] of tokens) {
          if (v === `var(${name})`) {
            const value = tokenOf(STYLES, name)
            expect(value, `${rule.selector} { ${prop}: var(${name}) } —— 引用了简写 token`).not.toMatch(
              /\b(solid|dashed|dotted|double|px|em|rem)\b/,
            )
          }
        }
      }
    }
    // 边框色 token 必须是纯颜色（供 .ah-input 的 border 简写与 .dp__input 的 border-color 共用）
    expect(tokenOf(STYLES, '--ah-ctl-border-color')).toBe('var(--border-input)')
  })

  /**
   * vendor 字体栈的每个消费方都要被覆盖
   *
   * 实机 bug（同日）：vendor 把自带字体栈（Linux 落到 -apple-system/sans-serif，
   * 与宿主 'Segoe UI'/system-ui 栈不同字形）设在 `.dp__main` 上，`.dp__input` 的
   * `font-family: inherit` 只是继承到这个栈。逐个消费方核对，vendor 升版新增
   * 消费方时本例会红。
   */
  it('vendor 用 --dp-font-family 的每个选择器都被 font-family: inherit 覆盖', () => {
    const vendorPath = resolve(
      AGENT_HUB,
      'node_modules/@vuepic/vue-datepicker/dist/main.css',
    )
    if (!existsSync(vendorPath)) {
      // vendor 文件缺失（依赖未安装）时本条无意义，但不得静默通过
      expect(existsSync(resolve(AGENT_HUB, 'package.json'))).toBe(true)
      return
    }
    const vendor = readFileSync(vendorPath, 'utf8')
    const consumers = new Set<string>()
    for (const r of parseRules(vendor)) {
      if ((r.decls['font-family'] ?? '').includes('--dp-font-family')) {
        consumers.add(r.selector.split(',')[0].trim())
      }
    }
    expect(consumers.size, '未解析到 vendor 的字体消费方').toBeGreaterThan(1)
    for (const sel of consumers) {
      const covered = parseRules(INDEX).some(
        (r) => r.selector.split(',').map((s) => s.trim()).includes(sel) &&
          r.decls['font-family'] === 'inherit',
      )
      expect(covered, `vendor 的 ${sel} 用了自带字体栈，覆盖里必须显式 font-family: inherit`).toBe(true)
    }
  })

  it('日期框行高跟宿主继承值（vendor 按 --dp-font-size=12px 算 18px，与实际字号脱钩）', () => {
    expect(dp['line-height']).toBe('1.5')
  })

  it('日期框补上 vendor 字体栈与左内边距覆盖（否则与关键词框不齐）', () => {
    expect(dp['font-family'], 'vendor 自带字体栈（Linux 落到 sans-serif）').toBe('inherit')
    expect(dp.padding, '左内边距须与 px-4 一致，右侧留清除按钮位').toContain('var(--ah-ctl-padding-x)')
  })

  it('聚焦态两侧同 token（border-brand + shadow-input-focus）', () => {
    const fInput = declsOf('.ah-input:focus')
    const fDp = declsOfBlock(INDEX, '.dp__input:focus')
    expect(fDp['border-color']).toBe(fInput['border-color'])
    expect(fDp['box-shadow']).toBe(fInput['box-shadow'])
  })

  it('token 值与宿主真源一致（tailwind.config 的 fontSize.sm / boxShadow.xs）', () => {
    const tw = readFileSync(resolve(AGENT_HUB, '../../tailwind.config.js'), 'utf8')
    // 两个段都含 xs/sm 键，必须按段切开再取，否则会串到 fontSize 的同名键
    const section = (name: string) =>
      new RegExp(`${name}:\\s*\\{([^}]*)\\}`).exec(tw)?.[1] ?? ''
    const sm = /sm:\s*'([^']+)'/.exec(section('fontSize'))?.[1] ?? ''
    const xs = /xs:\s*'([^']+)'/.exec(section('boxShadow'))?.[1] ?? ''
    expect(sm, 'tailwind.config fontSize.sm 读取失败').not.toBe('')
    expect(xs, 'tailwind.config boxShadow.xs 读取失败').not.toBe('')
    // 字号：--font-size-lg 必须与 tailwind text-sm 同表达式（否则两控件字号不等）
    const hostCss = readFileSync(resolve(AGENT_HUB, '../../src/style.css'), 'utf8')
    const lg = /--font-size-lg:\s*([^;]+);/.exec(hostCss)?.[1] ?? ''
    expect(lg, '宿主 --font-size-lg 定义变了').toBe(sm)
    const ctlFont = /--ah-ctl-font-size:\s*([^;]+);/.exec(STYLES)?.[1] ?? ''
    expect(ctlFont.trim()).toBe('var(--font-size-lg)')
    // 常态阴影：= tailwind shadow-xs 同值
    const ctlShadow = /--ah-ctl-shadow:\s*([^;]+);/.exec(STYLES)?.[1] ?? ''
    expect(ctlShadow.trim()).toBe(xs)
    expect(input['box-shadow']).toBe('var(--ah-ctl-shadow)')
  })

  it('宿主 Input.vue / SDK Select 的类组合未漂移（漂移即需同步本规格）', () => {
    for (const [rel, marker] of [
      ['../../src/components/Input.vue', 'shadow-xs dark:shadow-none'],
      ['../../src/components/Input.vue', 'placeholder:text-[var(--text-tertiary)]'],
      ['../../src/components/Input.vue', 'focus:shadow-input-focus'],
      ['../../packages/plugin-sdk-desktop/src/ui/Select.vue', 'rounded-input'],
      ['../../packages/plugin-sdk-desktop/src/ui/Select.vue', "h-[var(--input-height)] px-4 text-sm"],
    ] as const) {
      expect(readFileSync(resolve(AGENT_HUB, rel), 'utf8'), `${rel} 缺 ${marker}`).toContain(marker)
    }
  })
})

describe('S13 分段栏宽度与选中态无关', () => {
  // 回归成因（实机）：.ah-tab.active 改 font-weight: 600，粗体字更宽，而
  // .ah-tabs 是 inline-flex 收缩容器 → 整条分段栏随选中项宽度伸缩。
  // 契约：字重在 .ah-tab 上恒定，选中态规则不得再声明 font-weight。
  it('.ah-tab.active 不改字重（否则分段栏宽度随选中项变化）', () => {
    const active = declsOf('.ah-tab.active')
    expect(
      active['font-weight'],
      '.ah-tab.active 不得改 font-weight（粗体更宽 → inline-flex 整条跟着变宽）',
    ).toBeUndefined()
    expect(declsOf('.ah-tab')['font-weight'], '字重须在 .ah-tab 上恒定').toBe('600')
  })
})

/** 取单类名规则的声明体（用于样式层布局契约断言） */
function declsOf(selector: string): Record<string, string> {
  const hit = RULES.filter((r) => r.selector === selector)
  expect(hit.length, `${selector} 在 styles.css 里没有单类名规则`).toBeGreaterThan(0)
  return Object.assign({}, ...hit.map((r) => r.decls)) as Record<string, string>
}

describe('S10 顶部分段栏宽度与纵向滚动条解耦', () => {
  // 回归成因：.ah-view 是 flex 列容器，.ah-tabs 作为 flex item 会被 stretch
  // 拉成整行宽（＝内容区宽 − 滚动条宽）—— 滚动条一出现/消失，分段栏就变宽/横移。
  // 宽度必须只由内容决定（原型里就是 inline-flex 收窄形态）。
  it('.ah-tabs 收窄为内容宽（align-self: flex-start），不再随容器伸缩', () => {
    const tabs = declsOf('.ah-tabs')
    expect(tabs['align-self'], '.ah-tabs 缺 align-self: flex-start（会退回整行宽）').toBe(
      'flex-start',
    )
    expect(tabs['max-width'], '缺 max-width: 100%（窄面板会撑破整列）').toBe('100%')
  })

  it('.ah-tab 单项不压缩：靠 nowrap + 分段栏换行，而不是把中文标签挤断行', () => {
    const tab = declsOf('.ah-tab')
    expect(tab['white-space']).toBe('nowrap')
    expect(declsOf('.ah-tabs')['flex-wrap'], '窄面板需靠 flex-wrap 换行兜底').toBe('wrap')
  })

  it('滚动条槽位常驻（.ah-view scrollbar-gutter: stable），整列内容不横移', () => {
    expect(declsOf('.ah-view')['scrollbar-gutter']).toBe('stable')
  })
})

describe('S11 预设编辑器弹窗：头 / 体 / 脚三段 + 纵向字段', () => {
  it('面板是 flex 列且自身不滚动（滚动交给体部，头脚常驻）', () => {
    const panel = declsOf('.ah-modal-panel')
    expect(panel.display).toBe('flex')
    expect(panel['flex-direction']).toBe('column')
    expect(panel.overflow).toBe('hidden')
    expect(panel.padding, '面板须清掉 .ah-card 的内边距，改由三段各自负责').toBe('0')
  })

  it('体部独立滚动 + min-height: 0（否则超高表单会把脚部挤出 max-height）', () => {
    const body = declsOf('.ah-modal-body')
    expect(body['overflow-y']).toBe('auto')
    expect(body['min-height'], '缺 min-height: 0，flex 子项无法收缩').toBe('0')
  })

  it('脚部动作行右对齐且与体部有分隔线', () => {
    const foot = declsOf('.ah-modal-foot')
    expect(foot['justify-content']).toBe('flex-end')
    expect(foot['border-top']).toBeTruthy()
  })

  it('字段为纵向堆叠，且控件解除 flex: 1（纵向 flex 里 flex-basis 作用于高度）', () => {
    expect(declsOf('.ah-pv-field')['flex-direction']).toBe('column')
    const input = declsOf('.ah-pv-input')
    expect(input.flex, '缺 flex: 0 0 auto → 36px 输入框会被压成 0 高').toBe('0 0 auto')
    expect(input.width).toBe('100%')
    // key 行的输入框例外：行内要恢复弹性，否则把清空按钮挤出可视区
    const keyInput = declsOf('.ah-pv-keyrow .ah-pv-input')
    expect(keyInput.flex).toBe('1 1 auto')
  })

  it('label 关联与必填标记的类都在（模板侧契约由 components.test.ts 覆盖）', () => {
    for (const cls of ['.ah-pv-label', '.ah-pv-req', '.ah-modal-head', '.ah-modal-title']) {
      expect(declsOf(cls), `${cls} 无规则`).toBeTruthy()
    }
  })
})
