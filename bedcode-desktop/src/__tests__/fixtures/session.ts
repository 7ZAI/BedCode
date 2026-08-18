/**
 * Session fixtures — 会话信息 / 会话配置 / WSL 发行版 / 设备连接信息 / PTY 输出事件
 *
 * Rust DTO 源：
 * - SessionInfo          ← src-tauri/src/session/session_event.rs（serde 默认 snake_case；
 *   status 枚举 camelCase 序列化（running/stopped/...），session_type camelCase（pty）；
 *   started_at/stopped_at 为 Option 恒序列化（None→null）；
 *   task_status/task_reason/task_updated_at/task_questions 带 skip_serializing_if，
 *   None 时不出现在 JSON 中——本 fixture 取「全部字段都在」的最大线协议形态）
 * - SessionConfig        ← src-tauri/src/db/models.rs（#[serde(rename_all = "camelCase")]：
 *   id/name/environment/wslDistro/workingDir/command/autoStart/createdAt/updatedAt；
 *   前端 model.ts 同时声明 snake_case 别名（wsl_distro/working_dir/...）为遗留双写，线协议以 camelCase 为准）
 * - WslDistro            ← src-tauri/src/pty/wsl.rs（name/is_default/state/version；
 *   前端 model.ts 仅声明 name/state 子集视图，is_default/version 已同步补齐）
 * - DeviceConnectionInfo ← src-tauri/src/server/connection_types.rs（addr/device_id/fingerprint/session_count）
 *
 * 命名规则：默认 snake_case，SessionConfig 例外为 camelCase（Rust rename_all）。
 * 对齐机制：DTO_FIELDS 清单 + 工厂内 assertDtoFields 运行时断言；
 * 前端类型存在遗留别名/子集视图，不适用严格类型级 Equals，仅做单向检查。
 */

import { assertDtoFields } from './drift'

// ==================== SessionInfo ====================

export interface SessionInfoFixture {
  id: string
  config_id: string
  name: string
  /** 枚举序列化为 camelCase：idle/starting/running/waitingInput/stopping/stopped/error */
  status: string
  /** RFC3339 */
  created_at: string
  started_at: string | null
  stopped_at: string | null
  /** 枚举序列化为 camelCase，当前仅 pty */
  session_type: string
  /** 任务状态（snake_case：idle/in_progress/asking/completed/interrupted）或 null */
  task_status: string | null
  task_reason: string | null
  task_updated_at: string | null
  task_questions: unknown[] | null
}

/** 与 session_event.rs SessionInfo 字段一一对应（task_* 取「全部出现」的最大形态） */
export const SESSION_INFO_DTO_FIELDS = [
  'id',
  'config_id',
  'name',
  'status',
  'created_at',
  'started_at',
  'stopped_at',
  'session_type',
  'task_status',
  'task_reason',
  'task_updated_at',
  'task_questions',
] as const

export function makeSessionInfo(overrides: Partial<SessionInfoFixture> = {}): SessionInfoFixture {
  const fixture: SessionInfoFixture = {
    id: 'session-1',
    config_id: 'config-1',
    name: 'Test Session',
    status: 'running',
    created_at: '2025-01-01T00:00:00Z',
    started_at: '2025-01-01T00:00:00Z',
    stopped_at: null,
    session_type: 'pty',
    task_status: null,
    task_reason: null,
    task_updated_at: null,
    task_questions: null,
    ...overrides,
  }
  assertDtoFields(fixture, SESSION_INFO_DTO_FIELDS, 'SessionInfo')
  return fixture
}

// ==================== SessionConfig（camelCase 线协议） ====================

export interface SessionConfigFixture {
  id: string
  name: string
  environment: string
  wslDistro: string | null
  workingDir: string | null
  command: string | null
  autoStart: boolean
  /** RFC3339 */
  createdAt: string
  updatedAt: string
}

/** 与 db/models.rs SessionConfig（camelCase）字段一一对应 */
export const SESSION_CONFIG_DTO_FIELDS = [
  'id',
  'name',
  'environment',
  'wslDistro',
  'workingDir',
  'command',
  'autoStart',
  'createdAt',
  'updatedAt',
] as const

export function makeSessionConfig(overrides: Partial<SessionConfigFixture> = {}): SessionConfigFixture {
  const fixture: SessionConfigFixture = {
    id: 'config-1',
    name: 'Default',
    environment: 'wsl',
    wslDistro: 'Ubuntu',
    workingDir: '/home/user',
    command: 'claude',
    autoStart: false,
    createdAt: '2025-01-01T00:00:00Z',
    updatedAt: '2025-01-01T00:00:00Z',
    ...overrides,
  }
  assertDtoFields(fixture, SESSION_CONFIG_DTO_FIELDS, 'SessionConfig')
  return fixture
}

// ==================== WslDistro ====================

export interface WslDistroFixture {
  name: string
  is_default: boolean
  /** wsl --status 输出的 State 字符串（Running/Stopped/...） */
  state: string
  version: number
}

/** 与 pty/wsl.rs WslDistro 字段一一对应 */
export const WSL_DISTRO_DTO_FIELDS = ['name', 'is_default', 'state', 'version'] as const

export function makeWslDistro(overrides: Partial<WslDistroFixture> = {}): WslDistroFixture {
  const fixture: WslDistroFixture = {
    name: 'Ubuntu',
    is_default: false,
    state: 'Running',
    version: 2,
    ...overrides,
  }
  assertDtoFields(fixture, WSL_DISTRO_DTO_FIELDS, 'WslDistro')
  return fixture
}

// ==================== DeviceConnectionInfo ====================

export interface DeviceConnectionInfoFixture {
  addr: string
  device_id: string
  /** 设备指纹（与 db pairings 记录关联）；None 序列化为 null */
  fingerprint: string | null
  session_count: number
}

/** 与 connection_types.rs DeviceConnectionInfo 字段一一对应 */
export const DEVICE_CONNECTION_INFO_DTO_FIELDS = ['addr', 'device_id', 'fingerprint', 'session_count'] as const

export function makeDeviceConnectionInfo(overrides: Partial<DeviceConnectionInfoFixture> = {}): DeviceConnectionInfoFixture {
  const fixture: DeviceConnectionInfoFixture = {
    addr: '192.168.1.50',
    device_id: 'device-1',
    fingerprint: null,
    session_count: 1,
    ...overrides,
  }
  assertDtoFields(fixture, DEVICE_CONNECTION_INFO_DTO_FIELDS, 'DeviceConnectionInfo')
  return fixture
}
