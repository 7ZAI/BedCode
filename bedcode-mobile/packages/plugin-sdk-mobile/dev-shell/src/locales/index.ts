/**
 * Dev Shell 界面文案（zh-CN / en）
 * -----------------------------------------------------------------------------
 * 分三块聚合，职责各归各家：
 *   · `devshell.*` —— 工作台自身（工具条 / 日志面板 / 内置应用）
 *   · `shell.*`    —— 宿主壳 UI（与宿主 `bedcode-mobile/src/locales` 下的 shell 文案同键同义）
 *   · 内置应用的文案仍走 `devshell.terminal.*`（它是 dev-shell 的一部分，不是被调试插件）
 *
 * 已随宿主壳改造迁走的 key（旧三页签骨架：nav.terminal / nav.toolbox / nav.plugins /
 * toolbox.* / plugins.* / back）整面删除——它们描述的界面已不存在，留着会让文案表
 * 与实现脱节。
 */
import { zhCNShell, zhCNDevShell } from './shell'
import { enShell, enDevShell } from './en-shell'

export const zhCN = {
  'devshell.logs.title': 'Dev Shell 日志',
  'devshell.logs.clear': '清空',
  'devshell.logs.empty': '暂无日志',
  'devshell.frame.toggle': '手机框',
  'devshell.frame.on': '手机框：开',
  'devshell.frame.off': '手机框：关',
  'devshell.theme.dark': '深色模式',
  'devshell.theme.light': '浅色模式',
  'devshell.theme.system': '跟随系统',
  'devshell.toolbar.noPlugins': '未加载插件',
  'devshell.terminal.title': '模拟终端',
  'devshell.terminal.sessions': '会话',
  'devshell.terminal.output': '输出',
  'devshell.terminal.inputPlaceholder': '输入命令（记录到模拟会话输入行）',
  'devshell.terminal.send': '发送',
  'devshell.terminal.simulateOutput': '模拟输出',
  'devshell.terminal.createSession': '新建会话',
  'devshell.terminal.stopSession': '停止会话',
  'devshell.terminal.connect': '连接',
  'devshell.terminal.disconnect': '断开',
  'devshell.terminal.authSuccess': '认证成功',
  'devshell.terminal.connected': '已连接',
  'devshell.terminal.disconnected': '未连接',
  ...zhCNDevShell,
  ...zhCNShell,
}

export const en = {
  'devshell.logs.title': 'Dev Shell Logs',
  'devshell.logs.clear': 'Clear',
  'devshell.logs.empty': 'No logs',
  'devshell.frame.toggle': 'Phone frame',
  'devshell.frame.on': 'Phone frame: on',
  'devshell.frame.off': 'Phone frame: off',
  'devshell.theme.dark': 'Dark Mode',
  'devshell.theme.light': 'Light Mode',
  'devshell.theme.system': 'Follow System',
  'devshell.toolbar.noPlugins': 'No plugin loaded',
  'devshell.terminal.title': 'Mock Terminal',
  'devshell.terminal.sessions': 'Sessions',
  'devshell.terminal.output': 'Output',
  'devshell.terminal.inputPlaceholder': 'Type a command (recorded in the mock session input log)',
  'devshell.terminal.send': 'Send',
  'devshell.terminal.simulateOutput': 'Simulate output',
  'devshell.terminal.createSession': 'New session',
  'devshell.terminal.stopSession': 'Stop session',
  'devshell.terminal.connect': 'Connect',
  'devshell.terminal.disconnect': 'Disconnect',
  'devshell.terminal.authSuccess': 'Auth success',
  'devshell.terminal.connected': 'Connected',
  'devshell.terminal.disconnected': 'Disconnected',
  ...enDevShell,
  ...enShell,
}