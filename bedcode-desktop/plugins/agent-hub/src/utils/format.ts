/**
 * 使用统计展示层格式化（票据 06）
 *
 * 看板与会话日志共用的纯函数：token 量缩写、时长缩写、按天标签。
 * 数字与源数据一致性由后端聚合保证，本层只做无损展示换算（四舍五入
 * 截断进位方向向上取整到 0.1 粒度，避免「显示 0 但实际非 0」的假零）。
 */

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
