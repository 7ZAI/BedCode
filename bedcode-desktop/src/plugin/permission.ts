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
  'session:config',
  'ui:sidebar',
  'ui:toolbox',
  'ui:statusbar',
  'ui:dialog',
  'ui:pageToolbar',
  'ui:settings',
  'ui:input',
  'ui:fileHandler',
  'network:http',
  'storage',
  'broadcast',
  'peer',
  'ws:client',
  'ws:server',
  'auth',
  'pty:spawn',
  'pty:io',
  // task:run 为 WASM-only 权限（v20 host-task 并发任务域：execute-batch / submit /
  // status / cancel / list-jobs），无前端 API 方法映射
  'task:run',
])

/** 权限到 API 方法的映射 */
const PERMISSION_API_MAP: Record<string, string[]> = {
  'terminal:input': ['terminal.sendInput', 'terminal.onInput'],
  'terminal:output': ['terminal.onOutput'],
  'terminal:observe': ['terminal.onInputSubmitted'],
  // 终端窗口原语（票 13）同属观测面：预测初始网格 / 打开 / 关闭宿主终端窗口。
  // 窗口本体与渲染管线留宿主，插件只经本上下文触发（D2 前端收口）
  'session:read': [
    'session.list',
    'session.get',
    'session.onStatusChange',
    'session.predictTerminalSize',
    'session.openTerminal',
    'session.closeTerminal',
    'session.isTerminalOpen',
  ],
  'session:write': ['session.create', 'session.stop'],
  // session:config 为 WASM 优先权限（配置 CRUD 经插件命令通道，不直调宿主域命令）；
  // 登记三个审计名，与 host_impl 权限门同域
  'session:config': ['session.configUpsert', 'session.configGet', 'session.configDelete'],
  'ui:sidebar': ['ui.registerSidebarPanel', 'ui.registerPage'],
  'ui:toolbox': ['ui.registerToolboxPage'],
  'ui:statusbar': ['ui.registerStatusBarItem', 'ui.registerTitleBarItem'],
  'ui:dialog': ['ui.showDialog'],
  'ui:pageToolbar': ['ui.registerPageToolbarItem'],
  // ui:settings 为纯前端贡献面权限（无 WASM 宿主函数对应），只门住设置分组注册 API
  'ui:settings': ['ui.registerSettingsSection'],
  'ui:input': ['ui.registerInputExtension', 'ui.registerTerminalToolbarItem'],
  'ui:fileHandler': ['ui.registerFileHandler'],
  'network:http': ['http.registerEndpoint'],
  storage: ['storage.get', 'storage.set', 'storage.delete', 'storage.flush'],
  'system:open': ['system.revealInDir'],
  // peer 为 WASM-only 权限，无前端 API 方法映射；宿主在 host fn 层仲裁
  peer: [],
  // ws:* 为 WASM-only 权限（host-websocket 原语），无前端 API 方法映射
  'ws:client': [],
  'ws:server': [],
  // auth 为 WASM-only 权限（v15 secret-store），无前端 API 方法映射
  auth: [],
  // pty:* 为 WASM-only 权限（v16 host-pty 原语：创建域 / 数据域），无前端 API 方法映射
  'pty:spawn': [],
  'pty:io': [],
  // task:run 为 WASM-only 权限（v20 host-task 并发任务域），无前端 API 方法映射
  'task:run': [],
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
