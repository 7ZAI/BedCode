/**
 * 插件权限词汇表 —— 生成物，勿手改
 *
 * 真源：`packages/plugin-sdk-desktop/rust/src/permission.rs` 的
 * `VALID_PERMISSIONS` / `PERMISSION_API_MAP`。
 * 重跑：`cd bedcode-desktop/packages/plugin-sdk-desktop && pnpm run gen:permissions`
 * 锁定：`src-tauri/src/plugin/permission.rs` 的词汇漂移锁（集合相等断言）
 *
 * 消费方：`src/plugin/permission.ts`（前端快速失败面）
 */

/** 合法权限词汇（与 SDK VALID_PERMISSIONS 逐字一致） */
export const GENERATED_VALID_PERMISSIONS: readonly string[] = [
  'terminal:input',
  'terminal:output',
  'terminal:observe',
  'session:read',
  'session:write',
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
  'database:main',
  'fs:read',
  'fs:write',
  'broadcast',
  'timer:schedule',
  'process:run',
  'app:cli',
  'peer',
  'mdns',
  'ws:client',
  'ws:server',
  'auth',
  'pty:spawn',
  'pty:io',
  'task:run',
]

/** 权限 → API 方法名；空数组 = WASM-only 权限（无前端 context 方法可门） */
export const GENERATED_PERMISSION_API_MAP: Record<string, readonly string[]> = {
  'terminal:input': ['terminal.sendInput', 'terminal.onInput'],
  'terminal:output': ['terminal.onOutput'],
  'terminal:observe': ['terminal.onInputSubmitted'],
  'session:read': ['session.list', 'session.get', 'session.onStatusChange', 'session.predictTerminalSize', 'session.openTerminal', 'session.closeTerminal', 'session.isTerminalOpen'],
  'session:write': ['session.create', 'session.stop'],
  'ui:sidebar': ['ui.registerSidebarPanel', 'ui.registerPage'],
  'ui:toolbox': ['ui.registerToolboxPage'],
  'ui:statusbar': ['ui.registerStatusBarItem', 'ui.registerTitleBarItem'],
  'ui:dialog': ['ui.showDialog'],
  'ui:pageToolbar': ['ui.registerPageToolbarItem'],
  'ui:settings': ['ui.registerSettingsSection'],
  'ui:input': ['ui.registerInputExtension', 'ui.registerTerminalToolbarItem'],
  'ui:fileHandler': ['ui.registerFileHandler'],
  'network:http': ['http.registerEndpoint'],
  'storage': ['storage.get', 'storage.set', 'storage.delete', 'storage.flush'],
  'database:main': [],
  'fs:read': ['fs.read', 'fs.copy'],
  'fs:write': ['fs.write', 'fs.copy'],
  'broadcast': ['broadcast.sync'],
  'timer:schedule': ['timer.register'],
  'process:run': ['process.run', 'process.kill'],
  'app:cli': ['app.cliInstall', 'app.cliUninstall'],
  'peer': ['peer.listDevices', 'peer.dial', 'peer.disconnect', 'peer.respondConsent', 'peer.listTrusted', 'peer.revokeTrusted', 'peer.sendFiles', 'peer.listTransfers', 'peer.cancelTransfer', 'peer.retryTransfer', 'peer.clearTransferHistory', 'peer.listReceiving', 'peer.respondTransfer', 'peer.cancelReceiving', 'peer.clearReceivingHistory', 'peer.getReceiveSettings', 'peer.setReceivePolicy', 'peer.listSharedDirectories', 'peer.removeSharedDirectory', 'peer.addSharedDirectory', 'peer.listSharedRoots', 'peer.browseDirectory', 'peer.pullFiles', 'peer.pickFiles', 'peer.dialEndpoint', 'peer.close', 'peer.setSharedRoots'],
  'mdns': ['mdns.browse', 'mdns.stopBrowse', 'mdns.advertise', 'mdns.stopAdvertise', 'mdns.isAdvertising'],
  'ws:client': ['ws.connect', 'ws.sendText', 'ws.sendBinary', 'ws.close', 'ws.isConnected'],
  'ws:server': ['ws.registerEndpoint', 'ws.sendTextToClient', 'ws.sendBinaryToClient', 'ws.broadcastText', 'ws.broadcastBinary', 'ws.closeClient', 'ws.unregisterEndpoint', 'ws.listClients', 'ws.listEndpoints'],
  'auth': [],
  'pty:spawn': ['pty.spawn', 'pty.kill'],
  'pty:io': ['pty.write', 'pty.resize', 'pty.ringFetch', 'pty.isRunning'],
  'task:run': [],
}
