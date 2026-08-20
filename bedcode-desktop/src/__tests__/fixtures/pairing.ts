/**
 * Pairing fixtures — 配对码 / 已配对设备 / 二维码连接信息
 *
 * Rust DTO 源：
 * - PairingCodeInfo ← src-tauri/src/utils/auth/pairing.rs（手工 Serialize impl：
 *   code / created_at / expires_in 三个字段，expires_in 序列化为「剩余秒数」而非原始 TTL）
 * - Pairing         ← src-tauri/src/db/models.rs（list_paired_devices 返回 Vec<db::Pairing>，
 *   **#[serde(rename_all = "camelCase")]**：deviceName / deviceFingerprint / pairedAt /
 *   lastSeen / connectCount / isActive 等均为 camelCase 线协议；address / sessionToken /
 *   lastSeen 为 Option，None 序列化为 null）
 * - QrConnectionInfo ← src-tauri/src/commands/qr.rs（get_qr_connection_info 返回，
 *   snake_case；前端 model.ts 的 QrConnectionInfo 多出 url 字段为前端自建，不在线协议上）
 * - PendingDevice   ← src-tauri/src/utils/auth/pairing.rs（pairing_service 的
 *   get_pending_devices 返回；前端暂无消费方，fixture 供对齐回归使用）
 *
 * 命名规则：Pairing 为 camelCase（Rust rename_all），其余默认 snake_case；
 * DateTime<Utc> 序列化为 RFC3339 字符串。
 * 对齐机制：DTO_FIELDS 清单 + 工厂内 assertDtoFields 运行时断言 + 类型级 Equals。
 */

import type { PairingCodeInfo } from '@/composables/useDesktopCommands'
import { assertDtoFields, type Equals, type Expect } from './drift'

// ==================== PairingCodeInfo ====================

export interface PairingCodeInfoFixture {
  code: string
  /** RFC3339，如 2025-01-01T00:00:00Z */
  created_at: string
  /** 剩余有效秒数（服务端序列化时动态计算） */
  expires_in: number
}

/** 与 pairing.rs 手工 Serialize 的三个字段一一对应 */
export const PAIRING_CODE_INFO_DTO_FIELDS = ['code', 'created_at', 'expires_in'] as const

export function makePairingCodeInfo(
  overrides: Partial<PairingCodeInfoFixture> = {},
): PairingCodeInfoFixture {
  const fixture: PairingCodeInfoFixture = {
    code: '123456',
    created_at: '2025-01-01T00:00:00Z',
    expires_in: 60,
    ...overrides,
  }
  assertDtoFields(fixture, PAIRING_CODE_INFO_DTO_FIELDS, 'PairingCodeInfo')
  return fixture
}

// ==================== Pairing（已配对设备，db::Pairing，camelCase） ====================

export interface PairingFixture {
  id: string
  deviceName: string
  deviceFingerprint: string
  publicKey: string
  address: string | null
  sessionToken: string | null
  /** RFC3339 */
  pairedAt: string
  lastSeen: string | null
  connectCount: number
  isActive: boolean
}

/** 与 db/models.rs Pairing（#[serde(rename_all = "camelCase")]）字段一一对应 */
export const PAIRING_DTO_FIELDS = [
  'id',
  'deviceName',
  'deviceFingerprint',
  'publicKey',
  'address',
  'sessionToken',
  'pairedAt',
  'lastSeen',
  'connectCount',
  'isActive',
] as const

export function makePairing(overrides: Partial<PairingFixture> = {}): PairingFixture {
  const fixture: PairingFixture = {
    id: 'device-1',
    deviceName: 'Phone 1',
    deviceFingerprint: 'fp-1',
    publicKey: 'pk-1',
    address: null,
    sessionToken: null,
    pairedAt: '2025-01-01T00:00:00Z',
    lastSeen: null,
    connectCount: 0,
    isActive: true,
    ...overrides,
  }
  assertDtoFields(fixture, PAIRING_DTO_FIELDS, 'Pairing')
  return fixture
}

// ==================== QrConnectionInfo ====================

export interface QrConnectionInfoFixture {
  token: string
  host: string
  port: number
  /** 剩余有效时间（秒） */
  remaining_secs: number
}

/** 与 qr.rs QrConnectionInfo 字段一一对应（前端 model.ts 的 url 为前端自建，不在 DTO 内） */
export const QR_CONNECTION_INFO_DTO_FIELDS = ['token', 'host', 'port', 'remaining_secs'] as const

export function makeQrConnectionInfo(
  overrides: Partial<QrConnectionInfoFixture> = {},
): QrConnectionInfoFixture {
  const fixture: QrConnectionInfoFixture = {
    token: 'qr-token-abc',
    host: '192.168.1.5',
    port: 8766,
    remaining_secs: 300,
    ...overrides,
  }
  assertDtoFields(fixture, QR_CONNECTION_INFO_DTO_FIELDS, 'QrConnectionInfo')
  return fixture
}

// ==================== PendingDevice（待配对设备，utils/auth/pairing.rs） ====================

export interface PendingDeviceFixture {
  device_id: string
  device_name: string
  device_fingerprint: string
  /** RFC3339 */
  requested_at: string
}

/** 与 pairing.rs PendingDevice 四字段一一对应 */
export const PENDING_DEVICE_DTO_FIELDS = [
  'device_id',
  'device_name',
  'device_fingerprint',
  'requested_at',
] as const

export function makePendingDevice(
  overrides: Partial<PendingDeviceFixture> = {},
): PendingDeviceFixture {
  const fixture: PendingDeviceFixture = {
    device_id: 'pending-device-1',
    device_name: 'New Phone',
    device_fingerprint: 'fp-pending',
    requested_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
  assertDtoFields(fixture, PENDING_DEVICE_DTO_FIELDS, 'PendingDevice')
  return fixture
}

// ==================== 类型级对齐断言（编译期，防字段漂移） ====================

type _PairingCodeInfoEq = Expect<Equals<PairingCodeInfoFixture, PairingCodeInfo>>
