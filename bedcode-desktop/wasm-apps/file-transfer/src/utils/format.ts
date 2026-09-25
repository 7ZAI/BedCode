/**
 * 展示格式化工具
 *
 * 字节/速率单位（B/KB/MB/GB）为技术术语不参与翻译；剩余时间与日期
 * 涉及自然语言，经 i18n 的 t 函数取值，禁止中文硬编码。
 */

/** 字节 → 人类可读（1024 进制；目录无大小显示占位符） */
export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / Math.pow(1024, i)
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[i]}`
}

/** 秒 → 剩余时间文案（经 i18n key） */
export function formatEta(
  seconds: number,
  t: (key: string, params?: Record<string, unknown>) => string,
): string {
  if (!Number.isFinite(seconds) || seconds < 0) return ''
  if (seconds < 1) return t('transfer.eta.seconds', { count: 0 })
  if (seconds < 60) return t('transfer.eta.seconds', { count: Math.round(seconds) })
  const mins = Math.floor(seconds / 60)
  if (mins < 60) {
    return t('transfer.eta.minutes', { count: mins, seconds: Math.round(seconds % 60) })
  }
  return t('transfer.eta.hours', { count: Math.floor(mins / 60), minutes: mins % 60 })
}

/** Unix 秒 → 本地化日期（跟随宿主系统 locale，不做强制语言绑定） */
export function formatModified(mtime: number | null | undefined): string {
  if (!mtime) return '—'
  return new Date(mtime * 1000).toLocaleDateString(undefined, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  })
}

/** 任务显示名：取远端路径最后一段（上传方向为本地文件名） */
export function displayName(remotePath: string): string {
  const seg = remotePath.split('/').filter(Boolean).pop()
  return seg || remotePath
}

/** Unix 毫秒 → 本地化时间（历史条目；跟随宿主系统 locale） */
export function formatClock(ms: number | null | undefined): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString(undefined, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/**
 * 长指纹分组展示（对比友好）：按 groupSize 字符一组切分，返回分组数组。
 * 模板以 flex-wrap 渲染分组 span，换行严格落在组边界，避免无间隔长串
 * 任意断行出难看斜边（SSH / Bitcoin 公钥指纹同款惯例）。
 * 空白（含换行/空格）先剥除；空串返回 []；组尾允许不足整组。
 */
export function groupFingerprint(fingerprint: string, groupSize = 4): string[] {
  const clean = fingerprint.replace(/\s+/g, '')
  if (!clean) return []
  const size = Math.max(1, groupSize)
  const groups: string[] = []
  for (let i = 0; i < clean.length; i += size) {
    groups.push(clean.slice(i, i + size))
  }
  return groups
}
