/**
 * Session fixtures — 会话摘要 / 会话配置摘要
 *
 * Rust DTO 源：
 * - SessionSummary     ← src-tauri/src/enums/sumary.rs（id/name/status/created_at
 *   恒序列化；started_at / session_type 为 Option 无 skip_serializing_if → 恒序列化
 *   null；config_id / task_status / task_reason 带 skip_serializing_if → Some 才出现）
 * - SessionConfigSummary ← 同文件（id/name/environment/working_dir/command 恒序列化；
 *   wsl_distro 为 Option 无注解 → 恒序列化 null）
 */

import { assertDtoFields } from './drift'

// ==================== SessionSummary ====================

export interface SessionSummaryFixture {
  id: string
  name: string
  status: string
  created_at: string
  started_at: string | null
  session_type: string | null
  /** skip_serializing_if：undefined 表示序列化省略 */
  config_id?: string | undefined
  task_status?: string | undefined
  task_reason?: string | undefined
}

/** 与 sumary.rs SessionSummary 字段一一对应（drift 断言要求键集合恒全） */
export const SESSION_SUMMARY_DTO_FIELDS = [
  'id',
  'name',
  'status',
  'created_at',
  'started_at',
  'session_type',
  'config_id',
  'task_status',
  'task_reason',
] as const

export function makeSessionSummary(
  overrides: Partial<SessionSummaryFixture> = {},
): SessionSummaryFixture {
  const fixture: SessionSummaryFixture = {
    id: 'session-1',
    name: 'claude-code',
    status: 'running',
    created_at: '2026-08-16T10:00:00Z',
    started_at: '2026-08-16T10:00:01Z',
    session_type: 'pty',
    config_id: undefined,
    task_status: undefined,
    task_reason: undefined,
    ...overrides,
  }
  assertDtoFields(fixture, SESSION_SUMMARY_DTO_FIELDS, 'SessionSummary')
  return fixture
}

// ==================== SessionConfigSummary ====================

export interface SessionConfigSummaryFixture {
  id: string
  name: string
  environment: string
  wsl_distro: string | null
  working_dir: string
  command: string
}

/** 与 sumary.rs SessionConfigSummary 字段一一对应 */
export const SESSION_CONFIG_SUMMARY_DTO_FIELDS = [
  'id',
  'name',
  'environment',
  'wsl_distro',
  'working_dir',
  'command',
] as const

export function makeSessionConfigSummary(
  overrides: Partial<SessionConfigSummaryFixture> = {},
): SessionConfigSummaryFixture {
  const fixture: SessionConfigSummaryFixture = {
    id: 'config-1',
    name: '默认',
    environment: 'windows',
    wsl_distro: null,
    working_dir: 'D:/workspace',
    command: 'claude',
    ...overrides,
  }
  assertDtoFields(fixture, SESSION_CONFIG_SUMMARY_DTO_FIELDS, 'SessionConfigSummary')
  return fixture
}
