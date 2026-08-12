/**
 * Plugin Permission Check
 *
 * 前端快速失败 — 调用 Rust API 前先检查权限
 * Rust 端做最终仲裁，前端检查仅用于 UI 反馈和避免无效 invoke
 */

/** 合法权限列表 */
const VALID_PERMISSIONS = new Set([
  'terminal:input',
  'terminal:output',
  'terminal:observe',
  'session:read',
  'session:write',
  'ui:sidebar',
  'ui:toolbox',
  'ui:statusbar',
  'ui:pageToolbar',
  'ui:input',
  'ui:fileHandler',
  'network:http',
  'storage',
  'broadcast',
  'fileservice',
  'transfer',
])

/** 权限到 API 方法的映射 */
const PERMISSION_API_MAP: Record<string, string[]> = {
  'terminal:input': ['terminal.sendInput', 'terminal.onInput'],
  'terminal:output': ['terminal.onOutput'],
  'terminal:observe': ['terminal.onInputSubmitted'],
  'session:read': ['session.list', 'session.get', 'session.onStatusChange'],
  'session:write': ['session.create', 'session.stop'],
  'ui:sidebar': ['ui.registerSidebarPanel'],
  'ui:toolbox': ['ui.registerToolboxPage'],
  'ui:statusbar': ['ui.registerStatusBarItem', 'ui.registerTitleBarItem'],
  'ui:pageToolbar': ['ui.registerPageToolbarItem'],
  'ui:input': ['ui.registerInputExtension', 'ui.registerTerminalToolbarItem'],
  'ui:fileHandler': ['ui.registerFileHandler'],
  'network:http': ['http.registerEndpoint'],
  'storage': ['storage.get', 'storage.set', 'storage.delete', 'storage.flush'],
  'fileservice': [
    'fileService.mount',
    'fileService.unmount',
    'fileService.updateRoots',
    'fileService.getPeer',
    'fileService.pickDirectory',
    'fileService.pickFiles',
  ],
  'system:open': ['system.revealInDir'],
  // transfer 为 WASM-only 权限，无前端 API 方法映射；宿主在 host fn 层仲裁
  'transfer': [],
}

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

/** 过滤非法权限 */
export function filterValidPermissions(permissions: string[]): string[] {
  const result = permissions.filter(isValidPermission)
  if (!result.includes('storage')) {
    result.push('storage')
  }
  return result
}
