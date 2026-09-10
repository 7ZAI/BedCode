/**
 * Sync fixtures — 同步事件载荷（ws_sync_*）
 *
 * Rust 源：src-tauri/src/router/event.rs forward_event 的 serde_json::json!
 * 字面量（非 serde DTO，键集合由 emit 代码直接定义）：
 * - SyncSessionCreated / SyncSessionStatusChanged / SyncSessionStopped /
 *   SyncSessionRemoved / SyncConfigCreated / SyncConfigUpdated / SyncConfigRemoved /
 *   SyncTaskStatusChanged / SyncTaskQueueChanged / SyncTaskScheduledChanged /
 *   SyncSessionModeChanged
 *
 * 注意：Option 字段（task_reason / task_questions / task_id / status）经 json!
 * 宏序列化为 null（与 skip_serializing_if 的省略语义不同——事件层恒出现键）。
 */

import type { SessionSummaryFixture, SessionConfigSummaryFixture } from './session'
import { assertDtoFields } from './drift'

// ==================== 会话同步事件 ====================

export interface SyncSessionCreatedPayload {
  session: SessionSummaryFixture
  source_device: string
}

export function makeSyncSessionCreated(
  session: SessionSummaryFixture,
  sourceDevice = 'desktop-host',
): SyncSessionCreatedPayload {
  const fixture = { session, source_device: sourceDevice }
  assertDtoFields(fixture, ['session', 'source_device'], 'ws_sync_session_created')
  return fixture
}

export interface SyncSessionStatusChangedPayload {
  session_id: string
  old_status: string
  new_status: string
  session_name: string
}

export function makeSyncSessionStatusChanged(
  overrides: Partial<SyncSessionStatusChangedPayload> = {},
): SyncSessionStatusChangedPayload {
  const fixture: SyncSessionStatusChangedPayload = {
    session_id: 'session-1',
    old_status: 'running',
    new_status: 'stopped',
    session_name: 'claude-code',
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'old_status', 'new_status', 'session_name'], 'ws_sync_session_status_changed')
  return fixture
}

export interface SyncSessionStoppedPayload {
  session_id: string
  session_name: string
}

export function makeSyncSessionStopped(
  overrides: Partial<SyncSessionStoppedPayload> = {},
): SyncSessionStoppedPayload {
  const fixture: SyncSessionStoppedPayload = {
    session_id: 'session-1',
    session_name: 'claude-code',
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'session_name'], 'ws_sync_session_stopped')
  return fixture
}

export interface SyncSessionRemovedPayload {
  session_id: string
  session_name: string
}

export function makeSyncSessionRemoved(
  overrides: Partial<SyncSessionRemovedPayload> = {},
): SyncSessionRemovedPayload {
  const fixture: SyncSessionRemovedPayload = {
    session_id: 'session-1',
    session_name: 'claude-code',
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'session_name'], 'ws_sync_session_removed')
  return fixture
}

// ==================== 配置同步事件 ====================

export interface SyncConfigCreatedPayload {
  config: SessionConfigSummaryFixture
  source_device: string
}

export function makeSyncConfigCreated(
  config: SessionConfigSummaryFixture,
  sourceDevice = 'desktop-host',
): SyncConfigCreatedPayload {
  const fixture = { config, source_device: sourceDevice }
  assertDtoFields(fixture, ['config', 'source_device'], 'ws_sync_config_created')
  return fixture
}

export function makeSyncConfigUpdated(
  config: SessionConfigSummaryFixture,
  sourceDevice = 'desktop-host',
): SyncConfigCreatedPayload {
  const fixture = { config, source_device: sourceDevice }
  assertDtoFields(fixture, ['config', 'source_device'], 'ws_sync_config_updated')
  return fixture
}

export interface SyncConfigRemovedPayload {
  config_id: string
  config_name: string
}

export function makeSyncConfigRemoved(
  overrides: Partial<SyncConfigRemovedPayload> = {},
): SyncConfigRemovedPayload {
  const fixture: SyncConfigRemovedPayload = {
    config_id: 'config-1',
    config_name: '默认',
    ...overrides,
  }
  assertDtoFields(fixture, ['config_id', 'config_name'], 'ws_sync_config_removed')
  return fixture
}

// ==================== 任务状态同步事件 ====================

export interface SyncTaskStatusChangedPayload {
  session_id: string
  task_status: string
  /** Rust Option → null */
  task_reason: string | null
  task_questions: unknown[] | null
}

export function makeSyncTaskStatusChanged(
  overrides: Partial<SyncTaskStatusChangedPayload> = {},
): SyncTaskStatusChangedPayload {
  const fixture: SyncTaskStatusChangedPayload = {
    session_id: 'session-1',
    task_status: 'running',
    task_reason: null,
    task_questions: null,
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'task_status', 'task_reason', 'task_questions'], 'ws_sync_task_status_changed')
  return fixture
}

// ==================== 任务队列同步事件 ====================

export interface SyncTaskQueueChangedPayload {
  session_id: string
  queue_count: number
  action: string
  task_id: string | null
  status: string | null
}

export function makeSyncTaskQueueChanged(
  overrides: Partial<SyncTaskQueueChangedPayload> = {},
): SyncTaskQueueChangedPayload {
  const fixture: SyncTaskQueueChangedPayload = {
    session_id: 'session-1',
    queue_count: 2,
    action: 'add',
    task_id: null,
    status: null,
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'queue_count', 'action', 'task_id', 'status'], 'ws_sync_task_queue_changed')
  return fixture
}

// ==================== 定时任务同步事件 ====================

export interface SyncTaskScheduledChangedPayload {
  job_id: string
  status: string
  action: string
}

export function makeSyncTaskScheduledChanged(
  overrides: Partial<SyncTaskScheduledChangedPayload> = {},
): SyncTaskScheduledChangedPayload {
  const fixture: SyncTaskScheduledChangedPayload = {
    job_id: 'job-1',
    status: 'pending',
    action: 'create',
    ...overrides,
  }
  assertDtoFields(fixture, ['job_id', 'status', 'action'], 'ws_sync_task_scheduled_changed')
  return fixture
}

// ==================== 会话模式同步事件 ====================

export interface SyncSessionModeChangedPayload {
  session_id: string
  auto_approve: boolean
}

export function makeSyncSessionModeChanged(
  overrides: Partial<SyncSessionModeChangedPayload> = {},
): SyncSessionModeChangedPayload {
  const fixture: SyncSessionModeChangedPayload = {
    session_id: 'session-1',
    auto_approve: true,
    ...overrides,
  }
  assertDtoFields(fixture, ['session_id', 'auto_approve'], 'ws_sync_session_mode_changed')
  return fixture
}
