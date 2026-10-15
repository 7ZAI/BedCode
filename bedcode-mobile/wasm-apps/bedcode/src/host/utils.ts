/**
 * 宿主页纯函数（票 2026-10-09：独立可测，不含任何宿主/插件依赖）
 */

/** 连接状态 → i18n key（hub.status.*） */
export const STATUS_KEYS: Record<string, string> = {
  disconnected: 'hub.status.disconnected',
  connecting: 'hub.status.connecting',
  connected: 'hub.status.connected',
  pairing: 'hub.status.pairing',
  paired: 'hub.status.paired',
  error: 'hub.status.error',
}

export function statusKey(status: string): string {
  return STATUS_KEYS[status] ?? 'hub.status.unknown'
}

/** 地址:端口 解析（连接历史 / 严格格式）；格式非法返回 null */
export function parseAddress(value: string): { address: string; port: number } | null {
  const m = /^([^:]+):(\d+)$/.exec((value ?? '').trim())
  if (!m) return null
  return { address: m[1], port: parseInt(m[2], 10) }
}

/**
 * 地址解析（手动连接口径，与旧 DevicesView 一致）
 *
 * 旧实现在手动连接时把输入拆成 `host:port`，**未写端口则回落设置里的默认端口**
 * （旧 `mobile-settings.defaultPort`，缺省 8765）。本函数保留该口径：
 * - `10.0.0.5`            → `{ address: '10.0.0.5', port: defaultPort }`
 * - `10.0.0.5:8765`       → 按显式端口
 * - `10.0.0.5:`            → 空端口按默认端口（与旧 `parseInt('')` 为 NaN → 回落默认 同效）
 * - 非法端口 / 空主机      → null（由调用方给格式提示）
 *
 * IPv6 字面量不做特殊处理（旧实现同样不支持）：取最后一个冒号切分。
 */
export function parseAddressWithDefault(
  value: string,
  defaultPort: number,
): { address: string; port: number } | null {
  const raw = (value ?? '').trim()
  if (!raw) return null
  const idx = raw.lastIndexOf(':')
  if (idx === -1) return { address: raw, port: defaultPort }

  const host = raw.slice(0, idx).trim()
  const portStr = raw.slice(idx + 1).trim()
  if (!host) return null
  if (!portStr) return { address: host, port: defaultPort }

  const port = Number(portStr)
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return null
  return { address: host, port }
}

/**
 * 上次连接时间 → 友好相对文案（刚刚 / x 分钟前 / x 小时前 / x 天前 / 日期）
 *
 * 旧 DevicesView `formatLastConnected` 逐字还原；文案经 `t` 注入以保持纯函数
 * （可单测、不依 i18n 实例）。60 天以上直接给 `YYYY-MM-DD`。
 */
export function formatLastConnected(
  iso: string | undefined,
  t: (key: string, params?: Record<string, unknown>) => string,
): string {
  if (!iso) return ''
  const ts = new Date(iso).getTime()
  if (Number.isNaN(ts)) return ''
  const minutes = Math.floor((Date.now() - ts) / 60000)
  if (minutes < 1) return t('hub.timeJustNow')
  if (minutes < 60) return t('hub.timeMinutesAgo', { count: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('hub.timeHoursAgo', { count: hours })
  const days = Math.floor(hours / 24)
  if (days < 30) return t('hub.timeDaysAgo', { count: days })
  const d = new Date(ts)
  return `${String(d.getFullYear())}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** 会话状态展示 label key（hub.status*）；未知状态归「已停止」 */
export function sessionStatusKey(status: string): string {
  switch (status) {
    case 'running':
      return 'hub.statusRunning'
    case 'waitingInput':
      return 'hub.statusWaiting'
    default:
      return 'hub.statusStopped'
  }
}

/** 会话是否可停止 / 运行中判定（打开终端前的订阅预热判据） */
export function isSessionActive(status: string): boolean {
  return status === 'running' || status === 'waitingInput'
}