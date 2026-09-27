/**
 * 使用统计展示层格式化（票据 06 + 看板改版）
 *
 * 看板与会话日志共用的纯函数：token 量缩写、时长缩写、按天标签。
 * 数字与源数据一致性由后端聚合保证，本层只做无损展示换算（四舍五入
 * 截断进位方向向上取整到 0.1 粒度，避免「显示 0 但实际非 0」的假零）。
 */
import type { StatsMetric } from '../types'

/** token / 大数字量缩写：0–999 原样，k / M / G 三级，1 位小数 */
export function formatTokens(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return '—'
  const v = Math.max(0, n)
  if (v < 1000) return String(Math.round(v))
  const units = ['', 'k', 'M', 'G', 'T'] as const
  let tier = 0
  let scaled = v
  while (scaled >= 1000 && tier < units.length - 1) {
    scaled /= 1000
    tier++
  }
  // 1 位小数，向上进位避免假零（如 0.04k → 0.1k 而非 0k）
  const rounded = scaled < 10 ? (Math.ceil(scaled * 10) / 10).toFixed(1) : Math.round(scaled).toString()
  return `${rounded}${units[tier]}`
}

/** 毫秒时长缩写：<1s 为 ms；<60s 为 s；<60m 为 min；<24h 为 h（1 位小数）；否则 d */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  const s = ms / 1000
  if (s < 60) return `${roundCeil(s)}s`
  const min = s / 60
  if (min < 60) return `${roundCeil(min)}min`
  const h = min / 60
  if (h < 24) return `${roundCeil(h)}h`
  const d = h / 24
  return `${roundCeil(d)}d`
}

/** 向上取整到 0.1 粒度并格式化（整数则不带小数点；-1e-9 抵消浮点误差） */
function roundCeil(v: number): string {
  const scaled = Math.ceil(v * 10 - 1e-9) / 10
  return Number.isInteger(scaled) ? String(scaled) : scaled.toFixed(1)
}

/** epoch ms → 本地「MM-DD HH:mm」标签（会话列表用） */
export function formatSessionTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return '—'
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** epoch ms → 本地「HH:mm:ss」标签（事件流时间列用） */
export function formatEventTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return ''
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 项目路径缩写：家目录前缀折叠为 ~ */
export function abbreviateProject(project: string | null | undefined, home: string): string {
  if (!project) return '—'
  if (home && (project === home || project.startsWith(`${home}/`))) {
    return `~${project.slice(home.length)}`
  }
  return project
}

/** $ 成本展示：null 不估算显示 —；有值保留 2 位小数 */
export function formatCost(cost: number | null | undefined): string {
  if (cost == null || !Number.isFinite(cost)) return '—'
  return `$${cost.toFixed(2)}`
}

// ==================== 看板指标换算 ====================

/**
 * 最小 token 桶形状
 *
 * 字段全可选：看板的分组行（按天 / CLI / 项目 / 模型）与节奏格
 * （只有总量 `tokens`、无分桶）都是它的子集，取值函数因此能对
 * **所有**行型统一工作而不必断言成某个具体行型。
 */
export interface TokenBuckets {
  tokens_in?: number
  tokens_out?: number
  tokens_cache_read?: number
  tokens_cache_write?: number
}

/** 看板任一行的度量字段超集（指标取值的唯一入参形状；不导出：调用方直接传具体行型） */
interface MetricRow extends TokenBuckets {
  sessions?: number
  tokens_reasoning?: number
  cost_total?: number | null
  duration_ms?: number
  /** 节奏格专用：已算好的 token 总量（该格无分桶） */
  tokens?: number
}

/** token 总量 = 输入 + 输出 + 缓存读 + 缓存写
 *
 * **不加推理**：claude 的 `thinking_tokens` 是 `output_tokens` 的子集
 * （见 guest `usage_parse::claude`），重复相加会凭空放大总量。推理另作
 * 独立指标（`tokens_reasoning`）呈现，语义上标注为「其中推理」。
 *
 * 入参用 [`MetricRow`]（而非 [`TokenBuckets`]）：调用方拿到的都是完整行，
 * 带上 `tokens_reasoning` 字段也应当能直接传入——这正是「推理被忽略」
 * 得以被单测证明的前提。
 */
export function totalTokens(r: MetricRow | null | undefined): number {
  if (!r) return 0
  return (
    (r.tokens_in || 0) +
    (r.tokens_out || 0) +
    (r.tokens_cache_read || 0) +
    (r.tokens_cache_write || 0)
  )
}

/** 按指标取值（趋势 / 排行 / 占比共用的唯一取值口径；缺失归 0） */
export function metricValue(r: MetricRow, metric: StatsMetric): number {
  switch (metric) {
    case 'tokens':
      return r.tokens ?? totalTokens(r)
    case 'tokens_in':
      return r.tokens_in ?? 0
    case 'tokens_out':
      return r.tokens_out ?? 0
    case 'tokens_cache_read':
      return r.tokens_cache_read ?? 0
    case 'tokens_cache_write':
      return r.tokens_cache_write ?? 0
    case 'tokens_reasoning':
      return r.tokens_reasoning ?? 0
    case 'sessions':
      return r.sessions ?? 0
    case 'cost_total':
      return r.cost_total ?? 0
    case 'duration_ms':
      return r.duration_ms ?? 0
  }
}

/**
 * 缓存命中率 = 缓存读 / (输入 + 缓存读 + 缓存写)
 *
 * 分母是「进模型的输入侧总量」：命中越多，同样的任务要重付的输入越少。
 * 分母为 0（还没产生过输入）时无命中率可言，返回 null 而不是 0%——
 * 「0%」会被读成「缓存完全没起作用」。
 */
export function cacheHitRate(r: TokenBuckets | null | undefined): number | null {
  if (!r) return null
  const denom = (r.tokens_in || 0) + (r.tokens_cache_read || 0) + (r.tokens_cache_write || 0)
  if (denom <= 0) return null
  return (r.tokens_cache_read || 0) / denom
}

/** 百分比展示（0–1 小数 → 整数 %，null → —） */
export function formatPercent(ratio: number | null | undefined, digits = 0): string {
  if (ratio == null || !Number.isFinite(ratio)) return '—'
  return `${(ratio * 100).toFixed(digits)}%`
}

/**
 * 「好看的」坐标轴上界：1 / 2 / 2.5 / 5 × 10^k 中不小于 max 的最小值
 *
 * 直接用 max 当上界会让最高点顶到画布边缘（无呼吸），用 ceil 到整数则会在
 * 0.3 这种小值上产生巨大空白。数据全 0 时返回 1（避免除零与空路径）。
 */
export function niceMax(max: number): number {
  if (!Number.isFinite(max) || max <= 0) return 1
  const exp = Math.floor(Math.log10(max))
  const base = 10 ** exp
  for (const m of [1, 2, 2.5, 5, 10]) {
    const cand = m * base
    if (cand >= max - 1e-9) return cand
  }
  return 10 * base
}

/** 坐标轴刻度：上界向下取 3–4 档（0% / 50% / 100% 或四等分） */
export function axisTicks(max: number, count = 3): number[] {
  return Array.from({ length: count + 1 }, (_, i) => (max * (count - i)) / count)
}
