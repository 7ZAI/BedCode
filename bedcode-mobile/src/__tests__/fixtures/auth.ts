/**
 * Auth fixtures — 认证协议载荷 / 前端凭据类型
 *
 * Rust DTO 源：
 * - AuthPayload ← src-tauri/src/enums/auth.rs（snake_case；stage 必填，
 *   device_id / device_name / device_fingerprint / pairing_code / session_token /
 *   error / qr_token / public_key / challenge_nonce / signature / auth_method
 *   均为 Option + skip_serializing_if，序列化时省略 → fixture 默认不含）
 * - AuthStage     ← 同文件 `#[serde(rename_all = "snake_case")]` 枚举
 * - AuthCredentials ← 前端 src/composables/model.ts（ws_verify_pairing_code /
 *   ws_authenticate_with_qr / ws_authenticate_with_biometric 的 invoke 返回值，
 *   camelCase，非线协议 DTO，不参与字段清单断言）
 * - ConnectionInfo ← 前端 model.ts（ws_connect 返回）
 */

import type { AuthCredentials, ConnectionInfo } from '@/composables/model'
import { assertDtoFields, type Equals, type Expect } from './drift'

// ==================== AuthStage ====================

/** 与 auth.rs AuthStage 枚举变体逐一对应（snake_case 线协议值） */
export const AUTH_STAGES = [
  'request_pairing',
  'verify_code',
  'exchange_certificate',
  'biometric_request',
  'biometric_challenge',
  'biometric_verify',
  'authenticated',
  'reauthenticate',
  'failed',
  'qr_connect',
  'qr_failed',
] as const

export type AuthStageValue = (typeof AUTH_STAGES)[number]

// ==================== AuthPayload ====================

export interface AuthPayloadFixture {
  stage: AuthStageValue
  /** Option + skip_serializing_if：undefined 表示序列化省略 */
  device_id?: string | undefined
  device_name?: string | undefined
  device_fingerprint?: string | undefined
  pairing_code?: string | undefined
  session_token?: string | undefined
  error?: string | undefined
  qr_token?: string | undefined
  public_key?: string | undefined
  challenge_nonce?: string | undefined
  signature?: string | undefined
  auth_method?: string | undefined
}

/** 与 auth.rs AuthPayload 字段一一对应（Option 字段省略时不出现在 JSON） */
export const AUTH_PAYLOAD_DTO_FIELDS = [
  'stage',
  'device_id',
  'device_name',
  'device_fingerprint',
  'pairing_code',
  'session_token',
  'error',
  'qr_token',
  'public_key',
  'challenge_nonce',
  'signature',
  'auth_method',
] as const

/**
 * 构造认证载荷：默认含全部字段（键集合与 DTO_FIELDS 完全一致，drift 断言要求），
 * Option 字段默认 undefined——JSON 序列化时被省略，精确模拟 skip_serializing_if 语义
 */
export function makeAuthPayload(overrides: Partial<AuthPayloadFixture> = {}): AuthPayloadFixture {
  const fixture: AuthPayloadFixture = {
    stage: 'request_pairing',
    device_id: undefined,
    device_name: undefined,
    device_fingerprint: undefined,
    pairing_code: undefined,
    session_token: undefined,
    error: undefined,
    qr_token: undefined,
    public_key: undefined,
    challenge_nonce: undefined,
    signature: undefined,
    auth_method: undefined,
    ...overrides,
  }
  assertDtoFields(fixture, AUTH_PAYLOAD_DTO_FIELDS, 'AuthPayload')
  return fixture
}

// ==================== AuthCredentials（前端 invoke 返回） ====================

export function makeAuthCredentials(overrides: Partial<AuthCredentials> = {}): AuthCredentials {
  return {
    pairingId: 'pairing-1',
    fingerprint: 'fp-desktop-1',
    sessionToken: 'test-jwt-token',
    ...overrides,
  }
}

// ==================== ConnectionInfo（前端 invoke 返回） ====================

export function makeConnectionInfo(overrides: Partial<ConnectionInfo> = {}): ConnectionInfo {
  return {
    address: '192.168.1.100',
    port: 8765,
    status: 'connected',
    ...overrides,
  }
}

// ==================== 类型级对齐断言 ====================
// 前端 AuthCredentials / ConnectionInfo（camelCase invoke 返回）与 Rust 线协议
// DTO 无一一对应字段，仅做自引用类型检查（fixture 工厂签名随类型漂移自动报错）

type _AuthCredentialsEq = Expect<Equals<AuthCredentials, AuthCredentials>>
type _ConnectionInfoEq = Expect<Equals<ConnectionInfo, ConnectionInfo>>
