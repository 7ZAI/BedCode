/**
 * File service fixtures — 桌面端文件服务 HTTP 契约 + 挂载同步事件
 *
 * Rust 源：
 * - FileEntryDto / ListResponse ← src-tauri/src/file_service/server.rs
 *   （`#[serde(rename_all = "camelCase")]`——与 auth/session 的 snake_case 不同，
 *   name/size/mtime/is_dir → isDir；notice 带 skip_serializing_if 省略）
 * - SyncFileServiceChanged 事件 ← src-tauri/src/router/event.rs forward_event
 *   （plugin_id/mount_path/available/operations 键集合；operations 为
 *   FileOperation 数组）
 * - FileOperation ← packages/plugin-sdk-mobile/rust/src/types.rs
 *   （`#[serde(rename_all = "lowercase")]`：list / download / upload）
 */

import { assertDtoFields } from './drift'

// ==================== FileEntryDto ====================

export interface FileEntryFixture {
  name: string
  size: number
  mtime: number
  isDir: boolean
}

/** 与 server.rs FileEntryDto 字段一一对应（camelCase） */
export const FILE_ENTRY_DTO_FIELDS = ['name', 'size', 'mtime', 'isDir'] as const

export function makeFileEntry(overrides: Partial<FileEntryFixture> = {}): FileEntryFixture {
  const fixture: FileEntryFixture = {
    name: 'README.md',
    size: 1024,
    mtime: 1755300000,
    isDir: false,
    ...overrides,
  }
  assertDtoFields(fixture, FILE_ENTRY_DTO_FIELDS, 'FileEntryDto')
  return fixture
}

// ==================== ListResponse ====================

export interface ListResponseFixture {
  path: string
  entries: FileEntryFixture[]
  /** skip_serializing_if：undefined 表示序列化省略 */
  notice?: string | undefined
}

/** 与 server.rs ListResponse 字段一一对应（drift 断言要求键集合恒全） */
export const LIST_RESPONSE_DTO_FIELDS = ['path', 'entries', 'notice'] as const

export function makeListResponse(
  overrides: Partial<ListResponseFixture> = {},
): ListResponseFixture {
  const fixture: ListResponseFixture = {
    path: '',
    entries: [
      makeFileEntry({ name: 'docs', size: 0, isDir: true }),
      makeFileEntry(),
    ],
    notice: undefined,
    ...overrides,
  }
  assertDtoFields(fixture, LIST_RESPONSE_DTO_FIELDS, 'ListResponse')
  return fixture
}

// ==================== SyncFileServiceChanged 事件 ====================

/** 与 types.rs FileOperation 变体一一对应（lowercase 线协议值） */
export const FILE_OPERATIONS = ['list', 'download', 'upload'] as const
export type FileOperationValue = (typeof FILE_OPERATIONS)[number]

export interface SyncFileServiceChangedPayload {
  plugin_id: string
  mount_path: string
  available: boolean
  operations: FileOperationValue[]
}

export function makeSyncFileServiceChanged(
  overrides: Partial<SyncFileServiceChangedPayload> = {},
): SyncFileServiceChangedPayload {
  const fixture: SyncFileServiceChangedPayload = {
    plugin_id: 'file-transfer',
    mount_path: 'storage',
    available: true,
    operations: ['list', 'download'],
    ...overrides,
  }
  assertDtoFields(fixture, ['plugin_id', 'mount_path', 'available', 'operations'], 'ws_sync_file_service_changed')
  return fixture
}
