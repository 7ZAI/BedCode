/**
 * 对比度计算与受审矩阵（票 09 / 13 的共同数学底座 + 票 15 的回归护栏）
 *
 * 前景色**从 `src/styles.css` 实际解析**（新增/改动样式自动纳入判定，
 * 不手写常量），主题 token 从宿主 `src/style.css` 按真实层叠合成
 * （6 套 palette × 明暗 = 12 套主题）。承载面是结构信息（文字压在哪个面上），
 * 由 AUDITED / GRAPHICS 显式登记。
 *
 * 本文件只在测试期被 import，不进插件 lib 产物（vite 入口是 src/index.ts）。
 */
import { readFileSync, existsSync } from 'node:fs'
import { dirname, resolve as resolvePath } from 'node:path'
import { fileURLToPath } from 'node:url'

export type Rgb = [number, number, number]

const HERE = dirname(fileURLToPath(import.meta.url))
/** 插件根目录（src/__tests__/helpers → 上溯三级） */
export const AGENT_HUB = resolvePath(HERE, '../../..')
export const HOST_STYLE = resolvePath(AGENT_HUB, '../../src/style.css')

// ==================== 颜色解析 ====================

export function parseHex(v: string): Rgb | null {
  const hex = v.trim().replace(/^#/, '')
  if (hex.length === 3) return [0, 1, 2].map((i) => parseInt(hex[i] + hex[i], 16)) as Rgb
  if (hex.length === 6)
    return [
      parseInt(hex.slice(0, 2), 16),
      parseInt(hex.slice(2, 4), 16),
      parseInt(hex.slice(4, 6), 16),
    ] as Rgb
  return null
}

export function parseRgba(v: string): { rgb: Rgb; a: number } | null {
  const m = v.match(
    /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i,
  )
  if (!m) return null
  return {
    rgb: [Number(m[1]), Number(m[2]), Number(m[3])] as Rgb,
    a: m[4] === undefined ? 1 : Number(m[4]),
  }
}

/** 半透明层按 alpha 压在底色上 */
export function over(fg: Rgb, bg: Rgb, a: number): Rgb {
  return [0, 1, 2].map((i) => fg[i] * a + bg[i] * (1 - a)) as Rgb
}

// ==================== WCAG ====================

const chan = (c: number): number => {
  const v = c / 255
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4
}

export function luminance(rgb: Rgb): number {
  return 0.2126 * chan(rgb[0]) + 0.7152 * chan(rgb[1]) + 0.0722 * chan(rgb[2])
}

export function contrast(a: Rgb, b: Rgb): number {
  const la = luminance(a)
  const lb = luminance(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}

/** 色差 ΔE（CIE76，sRGB→XYZ→Lab） */
export function deltaE(a: Rgb, b: Rgb): number {
  const lab = (c: Rgb): number[] => {
    const [r, g, bl] = c.map(chan)
    const x = (r * 0.4124 + g * 0.3576 + bl * 0.1805) / 0.95047
    const y = r * 0.2126 + g * 0.7152 + bl * 0.0722
    const z = (r * 0.0193 + g * 0.1192 + bl * 0.9505) / 1.08883
    const f = (t: number) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116)
    const [fx, fy, fz] = [f(x), f(y), f(z)]
    return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)]
  }
  const [l1, a1, b1] = lab(a)
  const [l2, a2, b2] = lab(b)
  return Math.hypot(l1 - l2, a1 - a2, b1 - b2)
}

/** color-mix(in srgb, A p%, B)：两端 alpha 均为 1 时等价于 sRGB 通道线性插值 */
export function colorMix(a: Rgb, p: number, b: Rgb): Rgb {
  const k = p / 100
  return [0, 1, 2].map((i) => a[i] * k + b[i] * (1 - k)) as Rgb
}

// ==================== styles.css 解析（前景色真源） ====================

export interface Rule {
  selector: string
  decls: Record<string, string>
}

/** 极简 CSS 规则解析：取 `选择器 { 声明 }` 里的声明字典（够本表用，不求完备） */
export function parseRules(css: string): Rule[] {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, '')
  const rules: Rule[] = []
  const re = /([^{}]+)\{([^{}]*)\}/g
  let m: RegExpExecArray | null
  while ((m = re.exec(stripped)) !== null) {
    const selectors = m[1]
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
    const decls: Record<string, string> = {}
    for (const line of m[2].split(';')) {
      const i = line.indexOf(':')
      if (i < 0) continue
      decls[line.slice(0, i).trim()] = line.slice(i + 1).trim()
    }
    for (const sel of selectors) rules.push({ selector: sel, decls })
  }
  return rules
}

function declOf(rules: Rule[], selector: string, prop: string): string | null {
  const hit = rules.filter(
    (r) => r.selector === selector || r.selector.split(',').map((s) => s.trim()).includes(selector),
  )
  for (let i = hit.length - 1; i >= 0; i--) {
    const v = hit[i].decls[prop]
    if (v) return v
  }
  return null
}

/** 取某选择器的 color 表达式 */
export function colorOf(rules: Rule[], selector: string): string | null {
  return declOf(rules, selector, 'color')
}

/** 取某选择器的 background / background-color 表达式 */
export function backgroundOf(rules: Rule[], selector: string): string | null {
  return declOf(rules, selector, 'background') ?? declOf(rules, selector, 'background-color')
}

// ==================== 主题 token 解析（宿主色板） ====================

interface RootBlock {
  palette: string
  dark: boolean
  vars: Record<string, string>
}

function parseBlocks(css: string): RootBlock[] {
  const out: RootBlock[] = []
  const re = /:root(\.dark)?(?:\[data-palette='([^']+)'\])?\s*\{([^}]*)\}/g
  let m: RegExpExecArray | null
  while ((m = re.exec(css)) !== null) {
    const dark = !!m[1]
    const palette = m[2] ?? 'warm'
    const vars: Record<string, string> = {}
    for (const line of m[3].split('\n')) {
      const vm = line.match(/^\s*(--[\w-]+)\s*:\s*(.+?);\s*$/)
      if (vm) vars[vm[1]] = vm[2].trim()
    }
    out.push({ palette, dark, vars })
  }
  return out
}

export interface Theme {
  name: string
  vars: Record<string, string>
}

/** 按真实层叠合成 12 套主题：:root → :root.dark → :root[data-palette] → :root.dark[data-palette] */
export function loadThemes(stylePath: string): Theme[] {
  const blocks = parseBlocks(readFileSync(stylePath, 'utf8'))
  const pick = (palette: string, dark: boolean): Record<string, string> => {
    const order = (b: RootBlock) => (b.palette === 'warm' ? 0 : 1) + (b.dark ? 1 : 0)
    const applicable = blocks
      // 深色元素上 :root 与 [data-palette] 规则同样生效，只是被 .dark 块覆写
      .filter((b) => (dark || !b.dark) && (b.palette === 'warm' || b.palette === palette))
      .sort((a, b) => order(a) - order(b))
    let vars: Record<string, string> = {}
    for (const b of applicable) vars = { ...vars, ...b.vars }
    return vars
  }
  const palettes = [...new Set(blocks.map((b) => b.palette))]
  const themes: Theme[] = []
  for (const pal of palettes) {
    for (const mode of ['light', 'dark'] as const) {
      themes.push({ name: `${pal}/${mode}`, vars: pick(pal, mode === 'dark') })
    }
  }
  return themes.sort((a, b) => a.name.localeCompare(b.name))
}

/** 取宿主 token 颜色为不透明 RGB（rgba token 按其 alpha 压在 base 上） */
export function resolve(vars: Record<string, string>, expr: string, base?: Rgb): Rgb {
  const e = expr.trim()
  const hex = parseHex(e)
  if (hex) return hex
  const rgba = parseRgba(e)
  if (rgba) return over(rgba.rgb, base ?? [255, 255, 255], rgba.a)
  const varM = e.match(/^var\((--[\w-]+)\)$/)
  if (varM) {
    const raw = vars[varM[1]]
    if (raw === undefined) throw new Error(`宿主未定义 token ${varM[1]}`)
    return resolve(vars, raw, base)
  }
  throw new Error(`无法解析颜色表达式：${expr}`)
}

// ==================== 插件派生 token 求解 ====================

let ahVars: Record<string, string> = {}

/** agent-hub styles.css 里 :root / :root.dark 的插件派生 token（按明暗合成） */
function loadAhVars(agentHubDir: string, dark: boolean): Record<string, string> {
  const rules = parseRules(readFileSync(resolvePath(agentHubDir, 'src/styles.css'), 'utf8'))
  const merged: Record<string, string> = {}
  for (const r of rules) {
    if (r.selector === ':root') Object.assign(merged, r.decls)
    if (dark && r.selector === ':root.dark') Object.assign(merged, r.decls)
  }
  return merged
}

/** 支持 color-mix(in srgb, var(--x) P%, var(--y)) 的最小求值器 */
function resolveMixed(vars: Record<string, string>, expr: string, base?: Rgb): Rgb {
  const mix = expr.match(/^color-mix\(in srgb,\s*var\((--[\w-]+)\)\s*([\d.]+)%,\s*var\((--[\w-]+)\)\)$/)
  if (!mix) return resolve(vars, expr, base)
  const a = resolve(vars, `var(${mix[1]})`, base)
  const b = resolve(vars, `var(${mix[3]})`, base)
  return colorMix(a, Number(mix[2]), b)
}

/** 解析插件派生的 var()（--ah-* / --chart-* 定义在 agent-hub styles.css 的 :root 块） */
export function resolveAh(vars: Record<string, string>, expr: string, base?: Rgb): Rgb {
  const varM = expr.trim().match(/^var\((--[\w-]+)\)$/)
  if (!varM) return resolve(vars, expr, base)
  const raw = ahVars[varM[1]]
  if (raw === undefined) return resolve(vars, expr, base)
  return resolveMixed(vars, raw, base)
}

// ==================== 受审清单 ====================

/** 承载面 → 该面上会被压上的底色表达式（相对 --bg-card 的叠加层） */
export const SURFACES: Record<string, string> = {
  card: 'var(--bg-card)',
  page: 'var(--bg-page)',
  hover: 'var(--bg-hover)',
  sidebar: 'var(--bg-sidebar)',
  success: 'var(--color-success-light)',
  warning: 'var(--color-warning-light)',
  danger: 'var(--color-danger-light)',
  primaryLight: 'var(--color-primary-light)',
  bubble: 'var(--ah-bubble)',
}

export interface Audited {
  /** 前景选择器（前景色从 styles.css 解析） */
  sel: string
  /** 承载面（结构信息，显式登记） */
  surface: keyof typeof SURFACES
  /** 最小字号 px */
  px: number
  /** 字重（>=700 且 >=14px 才适用大字门禁） */
  weight?: number
  /** 覆盖前景表达式（如继承自父级 color 的场景） */
  fg?: string
  /** 豁免理由（非空即视为登记豁免，仍会跑出数值留档） */
  exempt?: string
}

export const AUDITED: Audited[] = [
  // ---------- 徽章 ----------
  { sel: '.ah-cli-tag', surface: 'hover', px: 11 },
  { sel: '.ah-cli-tag.ok', surface: 'success', px: 11 },
  { sel: '.ah-cli-tag.warn', surface: 'warning', px: 11 },
  { sel: '.ah-cli-tag.err', surface: 'danger', px: 11 },
  { sel: '.ah-cli-method', surface: 'hover', px: 11 },
  { sel: '.ah-lg-sources-count', surface: 'hover', px: 11 },
  { sel: '.ah-lg-badge-current', surface: 'primaryLight', px: 10, weight: 600 },
  // 票 07：来源形态标记（目录 / SQLite 库）
  { sel: '.ah-lg-source-kind', surface: 'card', px: 11 },

  // ---------- 概览 / 安装 ----------
  { sel: '.ah-env-label', surface: 'card', px: 13 },
  { sel: '.ah-cli-meta', surface: 'card', px: 13 },
  { sel: '.ah-cli-path', surface: 'card', px: 13 },
  { sel: '.ah-speed-text', surface: 'card', px: 13 },
  { sel: '.ah-speed-url', surface: 'card', px: 13 },
  { sel: '.ah-mirror-tmp', surface: 'card', px: 13 },
  { sel: '.ah-inst-versions', surface: 'card', px: 11 },
  { sel: '.ah-inst-outdated', surface: 'card', px: 11, weight: 700 },
  { sel: '.ah-inst-hint', surface: 'card', px: 11 },
  { sel: '.ah-cli-error', surface: 'card', px: 11 },

  // ---------- Skills / 供应商 ----------
  { sel: '.ah-sk-row-dir', surface: 'card', px: 11 },
  { sel: '.ah-sk-row-desc', surface: 'card', px: 11 },
  { sel: '.ah-sk-loading', surface: 'page', px: 13 },
  { sel: '.ah-sk-raw-error', surface: 'card', px: 11 },
  { sel: '.ah-pv-label', surface: 'card', px: 13 },
  { sel: '.ah-pv-radio', surface: 'card', px: 13 },

  // ---------- 使用统计 ----------
  { sel: '.ah-st-total-label', surface: 'card', px: 11 },
  { sel: '.ah-st-clabel', surface: 'card', px: 11 },
  { sel: '.ah-st-cval', surface: 'card', px: 11 },
  { sel: '.ah-st-table th', surface: 'card', px: 11 },
  { sel: '.ah-st-sub', surface: 'card', px: 11 },
  { sel: '.ah-st-legend', surface: 'card', px: 11 },
  { sel: '.ah-st-row-meta', surface: 'card', px: 11 },
  // 票 07：适配器降级横幅（信息性底色）+ 数据清空确认条 / 结果提示
  { sel: '.ah-st-degraded-text', surface: 'primaryLight', px: 13 },
  { sel: '.ah-st-clear-text', surface: 'card', px: 13 },
  { sel: '.ah-st-clear-note', surface: 'card', px: 11 },
  {
    sel: '.ah-st-empty',
    surface: 'card',
    px: 13,
    exempt:
      '装饰级豁免（--text-tertiary）：空态整句是「无数据」的唯一线索，但不承载任何数值/时间/路径；' +
      '抬到数据级会让空态比有数据的行更抢眼。--text-tertiary 是宿主全局 token，改它影响面超出本插件。',
  },

  // ---------- 会话日志 ----------
  { sel: '.ah-lg-source-path', surface: 'card', px: 11 },
  { sel: '.ah-lg-source-scan', surface: 'card', px: 11 },
  { sel: '.ah-lg-filter-label', surface: 'card', px: 11 },
  { sel: '.ah-lg-table-head', surface: 'card', px: 11 },
  { sel: '.ah-lg-col-time', surface: 'card', px: 11 },
  { sel: '.ah-lg-col-dur', surface: 'card', px: 11 },
  { sel: '.ah-lg-col-tokens', surface: 'card', px: 11 },
  { sel: '.ah-lg-pager-info', surface: 'card', px: 11 },
  { sel: '.ah-lg-cell-agent-name', surface: 'card', px: 11 },
  { sel: '.ah-lg-cell-sub', surface: 'card', px: 11 },
  { sel: '.ah-lg-head-src', surface: 'card', px: 11 },
  { sel: '.ah-lg-head-meta', surface: 'card', px: 11 },
  { sel: '.ah-lg-raw', surface: 'card', px: 11 },
  {
    sel: '.ah-lg-col-go',
    surface: 'card',
    px: 11,
    exempt: '装饰级豁免：「›」进入箭头是纯字形，行标题与 title 已说明可点击，读不出不影响任何信息获取。',
  },

  // ---------- 二级详情对话 ----------
  { sel: '.ah-msg-text', surface: 'hover', px: 12 },
  { sel: '.ah-msg-head', surface: 'hover', px: 11 },
  { sel: '.ah-msg-model', surface: 'hover', px: 11 },
  { sel: '.ah-msg-time', surface: 'hover', px: 11 },
  { sel: '.ah-msg-meta', surface: 'hover', px: 11 },
  { sel: '.ah-msg.role-tool .ah-msg-text', surface: 'sidebar', px: 11 },
  { sel: '.ah-msg.role-tool .ah-msg-head', surface: 'card', px: 11 },
  { sel: '.ah-msg.role-system .ah-msg-body', surface: 'hover', px: 11 },
  { sel: '.ah-msg.role-user .ah-msg-text', surface: 'bubble', px: 12, fg: 'var(--ah-on-primary)' },
  { sel: '.ah-msg.role-user .ah-msg-meta', surface: 'bubble', px: 11, fg: 'var(--ah-on-primary)' },

  // ---------- 导航 ----------
  { sel: '.ah-tab', surface: 'sidebar', px: 12 },
  { sel: '.ah-lg-tab', surface: 'sidebar', px: 11 },
  { sel: '.ah-speed-name', surface: 'card', px: 13 },
  { sel: '.ah-speed-ms', surface: 'card', px: 13 },
]

/** 无文本图形（WCAG 1.4.11 门槛 3:1） */
export const GRAPHICS: { sel: string; surface: keyof typeof SURFACES }[] = [
  { sel: '.ah-cli-tag.ok .ah-cli-dot', surface: 'success' },
  { sel: '.ah-cli-tag.warn .ah-cli-dot', surface: 'warning' },
  { sel: '.ah-cli-tag.err .ah-cli-dot', surface: 'danger' },
  { sel: '.ah-st-cin', surface: 'card' },
  { sel: '.ah-st-cout', surface: 'card' },
  { sel: '.ah-st-k.in', surface: 'card' },
  { sel: '.ah-st-k.out', surface: 'card' },
]

/** 门禁：WCAG AA。≥18px，或 ≥14px 且 bold，记 3.0:1 */
export function gateFor(px: number, weight = 400): number {
  return px >= 18 || (px >= 14 && weight >= 700) ? 3.0 : 4.5
}

export interface MatrixRow extends Audited {
  fgExpr: string | null
  gate: number
  worst: number
  worstTheme: string
  missing?: boolean
}

/** 跑文字矩阵：前景从 styles.css 解析，12 套主题取最差 */
export function runMatrix(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): MatrixRow[] {
  const rules = parseRules(readFileSync(resolvePath(agentHubDir, 'src/styles.css'), 'utf8'))
  const themes = loadThemes(hostStyle)
  const results: MatrixRow[] = []
  for (const item of AUDITED) {
    const fgExpr = item.fg ?? colorOf(rules, item.sel)
    if (!fgExpr) {
      results.push({ ...item, fgExpr: null, gate: gateFor(item.px, item.weight), worst: 0, worstTheme: '-', missing: true })
      continue
    }
    const gate = gateFor(item.px, item.weight)
    // 圆点实际坐在父徽章上（父的浅色底），文字层则按登记表里的承载面
    const bgExpr = item.sel.endsWith(' .ah-cli-dot') ? item.sel.replace(' .ah-cli-dot', '') : null
    let worst = Infinity
    let worstTheme = ''
    for (const th of themes) {
      ahVars = loadAhVars(agentHubDir, th.name.endsWith('/dark'))
      const card = resolve(th.vars, 'var(--bg-card)')
      const bg = bgExpr
        ? resolveAh(th.vars, backgroundOf(rules, bgExpr) ?? `var(${SURFACES[item.surface]})`, card)
        : resolveAh(th.vars, SURFACES[item.surface], card)
      const fg = resolveAh(th.vars, fgExpr, bg)
      const c = contrast(fg, bg)
      if (c < worst) {
        worst = c
        worstTheme = th.name
      }
    }
    results.push({ ...item, fgExpr, gate, worst, worstTheme })
  }
  return results
}

export interface GraphicsRow {
  sel: string
  fgExpr: string | null
  worst: number
  worstTheme: string
  missing?: boolean
}

/** 跑无文本图形矩阵（门槛 3:1） */
export function runGraphicsMatrix(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): GraphicsRow[] {
  const rules = parseRules(readFileSync(resolvePath(agentHubDir, 'src/styles.css'), 'utf8'))
  const themes = loadThemes(hostStyle)
  const out: GraphicsRow[] = []
  for (const item of GRAPHICS) {
    const fgExpr = backgroundOf(rules, item.sel)
    if (!fgExpr) {
      out.push({ sel: item.sel, fgExpr: null, worst: 0, worstTheme: '-', missing: true })
      continue
    }
    let worst = Infinity
    let worstTheme = ''
    for (const th of themes) {
      ahVars = loadAhVars(agentHubDir, th.name.endsWith('/dark'))
      const card = resolve(th.vars, 'var(--bg-card)')
      const bg = resolveAh(th.vars, SURFACES[item.surface], card)
      const fg = resolveAh(th.vars, fgExpr, bg)
      const c = contrast(fg, bg)
      if (c < worst) {
        worst = c
        worstTheme = th.name
      }
    }
    out.push({ sel: item.sel, fgExpr, worst, worstTheme })
  }
  return out
}

export interface ChartPairCheck {
  worstPair: number
  worstPairTheme: string
  worstSem: number
  worstSemWhere: string
  worstPri: number
  worstPriWhere: string
}

/** 图表双段：互相可区分，且不与语义三色 / 色板 primary 撞色 */
export function runChartPairCheck(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): ChartPairCheck {
  const themes = loadThemes(hostStyle)
  let worstPair = Infinity
  let worstPairTheme = ''
  let worstSem = Infinity
  let worstSemWhere = ''
  let worstPri = Infinity
  let worstPriWhere = ''
  for (const th of themes) {
    ahVars = loadAhVars(agentHubDir, th.name.endsWith('/dark'))
    const card = resolve(th.vars, 'var(--bg-card)')
    const cin = resolveAh(th.vars, 'var(--chart-in)', card)
    const cout = resolveAh(th.vars, 'var(--chart-out)', card)
    const de = deltaE(cin, cout)
    if (de < worstPair) {
      worstPair = de
      worstPairTheme = th.name
    }
    for (const t of ['--color-success', '--color-warning', '--color-danger']) {
      const d = Math.min(
        deltaE(cin, resolve(th.vars, `var(${t})`)),
        deltaE(cout, resolve(th.vars, `var(${t})`)),
      )
      if (d < worstSem) {
        worstSem = d
        worstSemWhere = `${th.name}/${t}`
      }
    }
    const dp = Math.min(
      deltaE(cin, resolve(th.vars, 'var(--color-primary)')),
      deltaE(cout, resolve(th.vars, 'var(--color-primary)')),
    )
    if (dp < worstPri) {
      worstPri = dp
      worstPriWhere = th.name
    }
  }
  return { worstPair, worstPairTheme, worstSem, worstSemWhere, worstPri, worstPriWhere }
}

/** 从 CSS 选择器里抽出全部类名 token（`.a.b .c` → a,b,c） */
export function selectorClassTokens(selector: string): string[] {
  return [...selector.matchAll(/\.([A-Za-z0-9_-]+)/g)].map((m) => m[1])
}

/** 收集模板里出现的类名：静态 class、字符串字面量、Transition name、:class 动态键 */
export function templateClasses(agentHubDir = AGENT_HUB): { used: Set<string>; prefixes: string[] } {
  const files = [
    'components/AgentHubView.vue',
    'components/AgentIcon.vue',
    'components/CliCard.vue',
    'components/CliIcon.vue',
    'components/InstallTab.vue',
    'components/OverviewTab.vue',
    'components/ProviderApply.vue',
    'components/ProvidersTab.vue',
    'components/SessionLogsTab.vue',
    'components/SkillEditor.vue',
    'components/SkillsTab.vue',
    'components/StatsTab.vue',
  ]
  const used = new Set<string>()
  const prefixes: string[] = []
  for (const f of files) {
    const p = resolvePath(agentHubDir, 'src', f)
    if (!existsSync(p)) continue
    const src = readFileSync(p, 'utf8')
    // 静态 class="..."
    for (const m of src.matchAll(/class="([^"]*)"/g)) {
      for (const cls of m[1].split(/\s+/)) {
        if (cls.startsWith('ah-')) used.add(cls)
      }
    }
    // 字符串字面量（:class="'ah-cli-tag'"、:class="{ ok: cond }" 里的键）
    for (const m of src.matchAll(/['"`]([A-Za-z0-9_-]*(?:ah-[A-Za-z0-9_-]+[A-Za-z0-9_-]*))['"`]/g)) {
      used.add(m[1])
    }
    // <Transition name="ah-page"> 生成的 enter/leave 类
    for (const m of src.matchAll(/<Transition[^>]*name="(ah-[a-z-]+)"/g)) {
      prefixes.push(`${m[1]}-`, m[1])
    }
    // `:class="`role-${e.role}`"` 这类模板串前缀
    for (const m of src.matchAll(/`([a-z-]*ah-[a-z-]*)\$\{/g)) prefixes.push(m[1])
  }
  return { used, prefixes }
}
