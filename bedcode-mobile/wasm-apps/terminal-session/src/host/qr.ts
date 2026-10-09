/**
 * 二维码配对载荷解析（票 2026-10-09：旧宿主 ScanPanel 的载荷契约迁入插件）
 * -----------------------------------------------------------------------------
 * 旧实现（`src/components/ScanPanel.vue`）区分两种无效：
 *   · JSON 解析失败 → `mobile.scan.invalidQr`（「二维码无效」）
 *   · 解析成功但 `host/port/token` 缺失 → `mobile.scan.invalidQrData`（「信息不完整」）
 * 本模块保留这一区分（`reason`），抽为纯函数以便单测锁定契约（不依赖相机/DOM）。
 *
 * 有意收紧一处：旧实现只判「三字段非空」，端口非数字要到连接阶段才失败；此处要求
 * 端口可归一为**正整数**，无效载荷在扫码时即判为 `incomplete`（错误更早、更贴近用户操作）。
 */

/** 桌面端二维码载荷（`{host, port, token}`） */
export interface QrPayload {
  host: string
  port: number
  token: string
}

/** 解析结果：成功带载荷；失败带原因（决定展示文案） */
export type QrParseResult =
  | { ok: true; payload: QrPayload }
  | { ok: false; reason: 'malformed' | 'incomplete' }

/** 端口归一：数字或纯数字字符串 → 正整数（≤65535）；其余 → null */
function normalizePort(value: unknown): number | null {
  const num = typeof value === 'number' ? value : Number(String(value ?? '').trim())
  if (!Number.isInteger(num) || num <= 0 || num > 65535) return null
  return num
}

/**
 * 解析二维码文本为连接载荷。
 *
 * @returns `{ok:true,payload}` 合法；`{ok:false,reason:'malformed'}` JSON 非法/非对象；
 *          `{ok:false,reason:'incomplete'}` 解析成功但字段缺失或类型不符
 */
export function parseQrText(decodedText: unknown): QrParseResult {
  if (typeof decodedText !== 'string' || !decodedText.trim()) {
    return { ok: false, reason: 'malformed' }
  }

  let raw: unknown
  try {
    raw = JSON.parse(decodedText)
  } catch {
    return { ok: false, reason: 'malformed' }
  }
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return { ok: false, reason: 'malformed' }
  }

  const { host, port, token } = raw as Record<string, unknown>
  if (typeof host !== 'string' || !host.trim()) return { ok: false, reason: 'incomplete' }
  if (typeof token !== 'string' || !token.trim()) return { ok: false, reason: 'incomplete' }

  const normalizedPort = normalizePort(port)
  if (normalizedPort === null) return { ok: false, reason: 'incomplete' }

  return { ok: true, payload: { host: host.trim(), port: normalizedPort, token } }
}
