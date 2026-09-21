/**
 * Plugin Permission Check
 *
 * 前端快速失败 — 调用 Rust API 前先检查权限
 * Rust 端做最终仲裁，前端检查仅用于 UI 反馈和避免无效 invoke
 *
 * 权限词汇与 API 映射都取自生成物 `permission.vocabulary.ts`（真源是桌面 SDK 的
 * `rust/src/permission.rs`）；在本文件里另立清单会让「加一个权限位」变成三处手抄，
 * 由宿主 `src-tauri/src/plugin/permission.rs` 的词汇漂移锁守住三处一致。
 */

import {
  GENERATED_PERMISSION_API_MAP,
  GENERATED_VALID_PERMISSIONS,
} from './permission.vocabulary'

/** 合法权限列表（生成物） */
const VALID_PERMISSIONS = new Set<string>(GENERATED_VALID_PERMISSIONS)

/** 权限到 API 方法的映射（生成物） */
const PERMISSION_API_MAP: Record<string, readonly string[]> = GENERATED_PERMISSION_API_MAP

/** 检查权限是否合法 */
export function isValidPermission(permission: string): boolean {
  return VALID_PERMISSIONS.has(permission)
}

/** 检查插件是否拥有调用指定 API 方法的权限 */
export function hasPermissionForApi(grantedPermissions: string[], apiMethod: string): boolean {
  for (const perm of grantedPermissions) {
    const apis = PERMISSION_API_MAP[perm]
    if (apis && apis.includes(apiMethod)) {
      return true
    }
  }
  return false
}
