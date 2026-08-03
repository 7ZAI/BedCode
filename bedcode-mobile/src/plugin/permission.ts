/**
 * Plugin Permission
 *
 * 前端权限检查 — 调用 API 前快速失败
 */

/** 权限到 API 方法的映射（与 SDK Rust permission.rs 单一事实来源一致） */
const PERMISSION_API_MAP: Record<string, string[]> = {
  'terminal:input': ['terminal.sendInput', 'terminal.onInput'],
  'terminal:output': ['terminal.onOutput'],
  'session:read': ['session.list', 'session.get', 'session.onStatusChange'],
  'session:write': ['session.create', 'session.stop'],
  'ui:toolbox': ['ui.registerToolboxPage'],
  'ui:navtab': ['ui.registerNavTab'],
  'ui:settings': ['ui.registerSettingsSection'],
  'ui:input': ['ui.registerTerminalToolbarItem'],
  'network:http': ['http.registerEndpoint'],
  'storage': ['storage.get', 'storage.set', 'storage.delete'],
  'fs:read': ['fs.read', 'fs.copy'],
  'fs:write': ['fs.write', 'fs.copy'],
  'bus': ['bus.publish', 'bus.subscribe', 'bus.unsubscribe'],
}

/** 检查权限列表是否允许调用指定 API 方法 */
export function hasPermissionForApi(permissions: string[], apiMethod: string): boolean {
  for (const perm of permissions) {
    const methods = PERMISSION_API_MAP[perm]
    if (methods && methods.includes(apiMethod)) {
      return true
    }
  }
  return false
}
