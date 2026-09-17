/**
 * 通用格式化工具（多文件重复实现的单源收敛）
 *
 * 从 src/plugin/contributionKinds.ts（formatBytes/formatTime）与
 * src/views/SessionsConfigView.vue（formatDateTime）抽取，宿主侧统一引用。
 */

/** 字节数格式化 */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

/** unix 毫秒时间戳格式化，缺失时显示 '—' */
export function formatTime(ms?: number): string {
  if (!ms) return '—'
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 日期时间字符串格式化（ISO 时间串 → zh-CN 短格式），空值显示 '--' */
export function formatDateTime(dateStr: string): string {
  if (!dateStr) return '--'
  return new Date(dateStr).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}
