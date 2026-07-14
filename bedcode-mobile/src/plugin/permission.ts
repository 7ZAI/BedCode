/**
 * Plugin Permission
 *
 * 前端权限检查 — 调用 API 前快速失败
 */

/** 合法权限列表 */
const VALID_PERMISSIONS = new Set([
  'terminal:input',
  'terminal:output',
  'session:read',
  'session:write',
  'ui:toolbox',
  'ui:navTab',
  'ui:terminalToolbar',
  'ui:settings',
  'storage',
])

/** 权限到 API 方法的映射 */
const PERMISSION_API_MAP: Record<string, string[]> = {
  'terminal:input': ['terminal.sendInput'],
  'terminal:output': ['terminal.onOutput'],
  'session:read': ['session.list', 'session.get', 'session.onStatusChange'],
  'session:write': ['session.create', 'session.stop'],
  'ui:toolbox': ['ui.registerToolboxPage'],
  'ui:navTab': ['ui.registerNavTab'],
  'ui:terminalToolbar': ['ui.registerTerminalToolbarItem'],
  'ui:settings': ['ui.registerSettingsSection'],
  'storage': ['storage.get', 'storage.set', 'storage.delete'],
}

/** 验证权限列表是否合法，返回不合法的权限 */
export function validatePermissions(permissions: string[]): string[] {
  return permissions.filter(p => !VALID_PERMISSIONS.has(p))
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
