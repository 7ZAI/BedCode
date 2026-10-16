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

/** 取某选择器的 stroke 表达式（SVG 图形用；线色不走 background） */
function strokeOf(rules: Rule[], selector: string): string | null {
  return declOf(rules, selector, 'stroke')
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
  { sel: '.ah-pv-label', surface: 'card', px: 12 },
  { sel: '.ah-pv-req', surface: 'card', px: 12 },
  { sel: '.ah-pv-radio', surface: 'card', px: 13 },

  // ---------- 使用统计看板 ----------
  { sel: '.ah-card-sub', surface: 'card', px: 11 },
  { sel: '.ah-st-kpi-label', surface: 'card', px: 11 },
  { sel: '.ah-st-kpi-sub', surface: 'card', px: 11 },
  { sel: '.ah-st-table th', surface: 'card', px: 11 },
  { sel: '.ah-trend-ytick', surface: 'card', px: 11 },
  { sel: '.ah-trend-xtick', surface: 'card', px: 11 },
  { sel: '.ah-trend-legend', surface: 'card', px: 11 },
  { sel: '.ah-trend-tip-day', surface: 'sidebar', px: 11 },
  { sel: '.ah-trend-tip-unit', surface: 'sidebar', px: 11 },
  { sel: '.ah-trend-tip-rows', surface: 'sidebar', px: 11 },
  { sel: '.ah-heat-dow', surface: 'card', px: 11 },
  { sel: '.ah-heat-hour', surface: 'card', px: 11 },
  { sel: '.ah-heat-legend-cap', surface: 'card', px: 11 },
  { sel: '.ah-heat-peak-when', surface: 'card', px: 11 },
  { sel: '.ah-donut-center-label', surface: 'card', px: 11 },
  { sel: '.ah-donut-legend-pct', surface: 'card', px: 11 },
  { sel: '.ah-bars-sub', surface: 'card', px: 11 },
  { sel: '.ah-bars-val', surface: 'card', px: 11 },
  { sel: '.ah-bars-rest', surface: 'card', px: 11 },
  // 票 07：适配器降级横幅（信息性底色）+ 数据清空确认条 / 结果提示
  { sel: '.ah-st-degraded-text', surface: 'primaryLight', px: 13 },
  { sel: '.ah-st-clear-text', surface: 'card', px: 13 },
  { sel: '.ah-st-clear-note', surface: 'card', px: 11 },
  {
    sel: '.ah-st-kpi-sub-dim',
    surface: 'card',
    px: 11,
    exempt:
      '装饰级豁免（--text-tertiary）：KPI 第三行是「数据区间」参考信息，' +
      '同一张卡上方的读数与副行已用数据级承载；抬到数据级会让参考行与读数同权重。',
  },
  {
    sel: '.ah-st-empty',
    surface: 'card',
    px: 13,
    exempt:
      '装饰级豁免（--text-tertiary）：空态整句是「无数据」的唯一线索，但不承载任何数值/时间/路径；' +
      '抬到数据级会让空态比有数据的行更抢眼。--text-tertiary 是宿主全局 token，改它影响面超出本插件。',
  },
  {
    sel: '.ah-bars-name',
    surface: 'card',
    px: 13,
    exempt:
      '豁免（--text-primary）：排行行名直接用主文字色，与表格行名一致；' +
      '在 --bg-card 上 --text-primary 的对比度远高于 AA 门禁，无需降档。',
  },
  {
    sel: '.ah-donut-legend-name',
    surface: 'card',
    px: 11,
    exempt:
      '豁免（--text-primary）：环图例的类目名是主文字色（同 .ah-bars-name 口径），' +
      '「类目是什么」必须一眼可读，不降为数据级。',
  },
  {
    sel: '.ah-heat-peak',
    surface: 'card',
    px: 13,
    exempt:
      '豁免（--text-primary）：峰值读数用主文字色（它是本卡的结论数字），同行的时间说明用数据级。',
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

/**
 * 无文本图形（WCAG 1.4.11 门槛 3:1）；`exempt` 非空即登记豁免（仍会跑出数值留档）
 *
 * `paint` 区分取色属性：HTML 色块读 `background`，SVG 描边图形读 `stroke`
 * （环形轨道就是 `stroke: var(--bg-hover)`，`backgroundOf` 解析不到）。
 */
export const GRAPHICS: {
  sel: string
  surface: keyof typeof SURFACES
  paint?: 'background' | 'stroke'
  exempt?: string
}[] = [
  { sel: '.ah-cli-tag.ok .ah-cli-dot', surface: 'success' },
  { sel: '.ah-cli-tag.warn .ah-cli-dot', surface: 'warning' },
  { sel: '.ah-cli-tag.err .ah-cli-dot', surface: 'danger' },
  // 节奏热力图色阶（顺序标度）。**低两档登记豁免**：标度的低端本就该
  // 「若有若无」（0 值与极小值必须看起来几乎一样），硬拉到 3:1 会让色阶
  // 失真；且每格都有 title/aria 文字替代与逐行合计，数值不依赖颜色读取。
  {
    sel: '.ah-heat-cell.lv1',
    surface: 'card',
    exempt:
      '顺序标度低档豁免（对比度 2.38:1 浅 / 2.01:1 深）：热力图的档位是' +
      '「量级」而非「类别」，低档须与空格的 --bg-hover 难以区分才有意义；' +
      'WCAG 1.4.11 不适用于数值另有文字通道的标度格（每格 title + 整图 aria + 逐行合计）。',
  },
  {
    sel: '.ah-heat-cell.lv2',
    surface: 'card',
    exempt:
      '顺序标度低档豁免（对比度 3.19:1 浅 / 2.52:1 深）：理由同 lv1；' +
      '亮度单调性与顶档 ≥3:1 由 heat ramp 检查项另行锁住。',
  },
  { sel: '.ah-heat-cell.lv3', surface: 'card' },
  { sel: '.ah-heat-cell.lv4', surface: 'card' },
  { sel: '.ah-heat-cell', surface: 'card', exempt: '空格轨道底（--bg-hover）：无数据位的底色，承载「这里没有」这一信息。' },
  { sel: '.ah-donut-track', surface: 'card', paint: 'stroke', exempt: '环的轨道底（--bg-hover）：纯装饰底，不承载数据分段。' },
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
  exempt?: string
}

/** 跑无文本图形矩阵（门槛 3:1；豁免项仍跑出数值留档） */
export function runGraphicsMatrix(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): GraphicsRow[] {
  const rules = parseRules(readFileSync(resolvePath(agentHubDir, 'src/styles.css'), 'utf8'))
  const themes = loadThemes(hostStyle)
  const out: GraphicsRow[] = []
  for (const item of GRAPHICS) {
    const fgExpr =
      item.paint === 'stroke' ? strokeOf(rules, item.sel) : backgroundOf(rules, item.sel)
    if (!fgExpr) {
      out.push({ sel: item.sel, fgExpr: null, worst: 0, worstTheme: '-', missing: true, exempt: item.exempt })
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
    out.push({ sel: item.sel, fgExpr, worst, worstTheme, exempt: item.exempt })
  }
  return out
}

export interface ChartPairCheck {
  /** 参与校验的槽位（分类色的数量可变，判据对全组两两生效） */
  slots: string[]
  worstPair: number
  worstPairTheme: string
  worstSem: number
  worstSemWhere: string
  worstPri: number
  worstPriWhere: string
  worstCard: number
  worstCardTheme: string
  worstCardSlot: string
}

/** 分类色槽（styles.css `:root` 里登记的那些；顺序即色相分组序） */
export const CHART_SLOTS = ['--chart-c1', '--chart-c2', '--chart-c3', '--chart-c4']

/**
 * 分类色组校验（4 槽全组两两）：
 * 互相可区分、与语义三色 / 色板 primary 不撞色、且对承载面 ≥ 3:1
 *
 * 环形扇区与图例色块的 `stroke`/`background` 是**内联绑定**的（按数据下标
 * 选槽），无法从 CSS 规则解析前景，因此在 token 层校验：只要 token 本身
 * 过关，两种用法都过关。
 */
export function runChartPairCheck(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): ChartPairCheck {
  const themes = loadThemes(hostStyle)
  let worstPair = Infinity
  let worstPairTheme = ''
  let worstSem = Infinity
  let worstSemWhere = ''
  let worstPri = Infinity
  let worstPriWhere = ''
  let worstCard = Infinity
  let worstCardTheme = ''
  let worstCardSlot = ''
  for (const th of themes) {
    ahVars = loadAhVars(agentHubDir, th.name.endsWith('/dark'))
    const card = resolve(th.vars, 'var(--bg-card)')
    const rgbs = CHART_SLOTS.map((s) => ({ slot: s, rgb: resolveAh(th.vars, `var(${s})`, card) }))
    for (const c of rgbs) {
      const cc = contrast(c.rgb, card)
      if (cc < worstCard) {
        worstCard = cc
        worstCardTheme = th.name
        worstCardSlot = c.slot
      }
      for (const t of ['--color-success', '--color-warning', '--color-danger']) {
        const d = deltaE(c.rgb, resolve(th.vars, `var(${t})`))
        if (d < worstSem) {
          worstSem = d
          worstSemWhere = `${th.name}/${t}/${c.slot}`
        }
      }
      const dp = deltaE(c.rgb, resolve(th.vars, 'var(--color-primary)'))
      if (dp < worstPri) {
        worstPri = dp
        worstPriWhere = `${th.name}/${c.slot}`
      }
    }
    for (let i = 0; i < rgbs.length; i++)
      for (let j = i + 1; j < rgbs.length; j++) {
        const de = deltaE(rgbs[i].rgb, rgbs[j].rgb)
        if (de < worstPair) {
          worstPair = de
          worstPairTheme = `${th.name} ${rgbs[i].slot}~${rgbs[j].slot}`
        }
      }
  }
  return {
    slots: CHART_SLOTS,
    worstPair,
    worstPairTheme,
    worstSem,
    worstSemWhere,
    worstPri,
    worstPriWhere,
    worstCard,
    worstCardTheme,
    worstCardSlot,
  }
}

export interface HeatRampCheck {
  /** 顺序标度的档位（自低到高） */
  steps: string[]
  /** 每档对承载面的最差对比度（按档序） */
  worstPerStep: { step: string; worst: number; worstTheme: string }[]
  /** 相邻档亮度是否严格单调（自低档到高档递减/递增均需一致） */
  monotone: boolean
  /** 顶档（最深）对承载面的最差对比度 */
  topWorst: number
  topWorstTheme: string
}

/**
 * 热力图顺序标度校验
 *
 * 低档对 3:1 豁免（见 GRAPHICS 里的登记理由），所以校验项换成标度本身的
 * 两条硬要求：**亮度单调**（读者才能把「更亮 = 更多」记成一条规则）与
 * **顶档 ≥ 3:1**（最重的那一格必须看得见）。
 */
export function runHeatRampCheck(agentHubDir = AGENT_HUB, hostStyle = HOST_STYLE): HeatRampCheck {
  const steps = ['--chart-heat-1', '--chart-heat-2', '--chart-heat-3', '--chart-heat-4']
  const themes = loadThemes(hostStyle)
  const worstPerStep = steps.map((step) => ({ step, worst: Infinity, worstTheme: '' }))
  // 逐主题判定单调（同一主题内比较才有意义）
  let monotone = true
  for (const th of themes) {
    ahVars = loadAhVars(agentHubDir, th.name.endsWith('/dark'))
    const card = resolve(th.vars, 'var(--bg-card)')
    const lums = steps.map((s) => luminance(resolveAh(th.vars, `var(${s})`, card)))
    const desc = lums.every((v, i) => i === 0 || v < lums[i - 1])
    const asc = lums.every((v, i) => i === 0 || v > lums[i - 1])
    if (!desc && !asc) monotone = false
    steps.forEach((s, i) => {
      const c = contrast(resolveAh(th.vars, `var(${s})`, card), card)
      if (c < worstPerStep[i].worst) {
        worstPerStep[i].worst = c
        worstPerStep[i].worstTheme = th.name
      }
    })
  }
  return {
    steps,
    worstPerStep,
    monotone,
    topWorst: worstPerStep[worstPerStep.length - 1].worst,
    topWorstTheme: worstPerStep[worstPerStep.length - 1].worstTheme,
  }
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
    'components/StatsBars.vue',
    'components/StatsDonut.vue',
    'components/StatsHeatmap.vue',
    'components/StatsTab.vue',
    'components/StatsTrend.vue',
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
