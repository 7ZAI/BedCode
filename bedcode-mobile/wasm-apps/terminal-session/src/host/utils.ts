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

/** 地址:端口 解析（手动连接 / 连接历史）；格式非法返回 null */
export function parseAddress(value: string): { address: string; port: number } | null {
  const m = /^([^:]+):(\d+)$/.exec((value ?? '').trim())
  if (!m) return null
  return { address: m[1], port: parseInt(m[2], 10) }
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