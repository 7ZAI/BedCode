/**
 * Plugin Permission
 *
 * 前端权限检查 — 调用 API 前快速失败
 */

/** 权限到 API 方法的映射（与 SDK Rust permission.rs 单一事实来源一致；
 *  terminal:input 与 terminal.sendInput/onOutput 已随票 15 阶段 B 退役） */
const PERMISSION_API_MAP: Record<string, string[]> = {
  'terminal:output': ['terminal-stream.forwardOutput'],
  'session:read': ['session.list', 'session.get', 'session.onStatusChange'],
  'session:write': ['session.create', 'session.stop'],
  // 票 2026-10-10 批次 C2：`ui:toolbox` / `ui:navtab` / `ui:input` 三个权限位
  // 随对应扩展点（registerToolboxPage / registerNavTab / registerTerminalToolbarItem
  // / registerTerminalView）整面退役，已从本表删除。退役的权限位在 Rust 装载期
  // 直接抛错（§5.1.3 fail-visible 形态③），不会走到前端这张表。
  'ui:route': ['ui.registerRoute', 'ui.openPage', 'ui.goBack'],
  'ui:back': ['ui.onBackPressed'],
  'ui:dialog': ['ui.showDialog'],
  'network:http': ['http.registerEndpoint'],
  'storage': ['storage.get', 'storage.set', 'storage.delete'],
  'fs:read': ['fs.read', 'fs.copy'],
  'fs:write': ['fs.write', 'fs.copy'],
  'bus': ['bus.publish', 'bus.subscribe', 'bus.unsubscribe'],
  'system:open': ['system.openFile', 'system.revealInDir', 'system.revealReceivedFileLocation', 'system.requestAllFilesAccess', 'system.openDownloadDir'],
  // peer 为 WASM-only 权限，无前端 API 方法映射；宿主在 host fn 层仲裁
  'peer': [],
  // notify（ABI v18 host-notify 域：通知/震动/声音）同为 WASM-only 权限，
  // 无前端 API 方法映射；宿主在 host fn 层仲裁
  'notify': [],
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
