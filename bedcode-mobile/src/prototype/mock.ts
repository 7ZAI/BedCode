/**
 * PROTOTYPE — 一次性静态 mock 数据（原型专用，勿在生产代码引用）
 *
 * 供 /prototype/mobile-ui 三个设计变体共享，模拟移动端全部页面的真实数据形态。
 */

// ==================== SVG icon paths（heroicons outline 风格） ====================

export const I = {
  monitor:
    'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z',
  terminal:
    'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
  toolbox:
    'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z',
  gear: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z',
  gearCenter: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z',
  puzzle:
    'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z',
  wifi: 'M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0',
  bell: 'M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9',
  shield:
    'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z',
  palette:
    'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01',
  info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  qr: 'M3 3h6v6H3V3zm12 0h6v6h-6V3zM3 15h6v6H3v-6zm12 3h3m3 0h-3m0 3v-3m-9-9h.01M15 15h.01M21 21h.01',
  clock: 'M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z',
  play: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664zM21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  chevronR: 'M9 5l7 7-7 7',
  back: 'M15 19l-7-7 7-7',
  plus: 'M12 4v16m8-8H4',
  x: 'M6 18L18 6M6 6l12 12',
  bolt: 'M13 10V3L4 14h7v7l9-11h-7z',
  folder: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
  refresh:
    'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15',
  tasks:
    'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4',
  unlink:
    'M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5M10.172 13.828a4 4 0 010-5.656l3-3a4 4 0 115.656 5.656l-1.5 1.5',
  search: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z',
} as const

// ==================== 数据模型 ====================

export interface SessionConfigMock {
  id: string
  name: string
  env: 'wsl' | 'win'
  path: string
  running: boolean
  runningSessionName?: string
}

export interface SessionMock {
  id: string
  name: string
  type: string
  status: 'running' | 'waiting' | 'stopped'
  elapsed: string
  task?: string
}

export interface PluginMock {
  id: string
  name: string
  author: string
  version: string
  builtin: boolean
  enabled: boolean
  desc: string
  perms: number
  size: string
  chips: number
}

export const currentDevice = {
  name: 'DESKTOP-Kael-Dev',
  address: '192.168.31.42:8420',
  uptime: '2h 47m',
  os: 'Windows 11 · WSL2',
}

export const sessionConfigs: SessionConfigMock[] = [
  { id: 'c1', name: 'Claude Code · main', env: 'wsl', path: '~/dev/bedcode', running: true, runningSessionName: 'claude-code-main' },
  { id: 'c2', name: 'PowerShell · ops', env: 'win', path: 'D:\\ops\\deploy', running: false },
  { id: 'c3', name: 'zsh · scratch', env: 'wsl', path: '~/scratch', running: false },
]

export const sessions: SessionMock[] = [
  { id: 's1', name: 'claude-code-main', type: 'Claude Code', status: 'running', elapsed: '2:47:13', task: '任务执行中' },
  { id: 's2', name: 'build-watch', type: 'PTY', status: 'running', elapsed: '0:32:08' },
  { id: 's3', name: 'deploy-uat', type: 'PTY', status: 'waiting', elapsed: '0:08:51', task: '等待输入' },
  { id: 's4', name: 'log-tail', type: 'PTY', status: 'stopped', elapsed: '1:12:40' },
]

export const connectionHistory = [
  { name: 'DESKTOP-Kael-Dev', address: '192.168.31.42:8420', last: '2 小时前' },
  { name: 'MacBook-Pro-M4', address: '192.168.31.17:8420', last: '昨天 23:41' },
  { name: 'homelab-nixos', address: '10.0.4.2:8420', last: '3 天前' },
]

export const plugins: PluginMock[] = [
  {
    id: 'auto-task', name: 'Auto Task', author: 'BedCode', version: '1.2.0', builtin: true, enabled: true,
    desc: '预设任务自动化：定时执行、条件触发与结果通知。', perms: 5, size: '128 KB', chips: 4,
  },
  {
    id: 'git-glance', name: 'Git Glance', author: 'R. Okonkwo', version: '0.9.3', builtin: false, enabled: true,
    desc: '在工具箱中查看分支状态、最近提交与未推送变更。', perms: 3, size: '86 KB', chips: 2,
  },
  {
    id: 'term-themes', name: 'Terminal Themes', author: 'M. Ishikawa', version: '2.1.4', builtin: false, enabled: false,
    desc: '终端配色方案扩展，支持导入 iTerm2 / Dracula 主题。', perms: 1, size: '42 KB', chips: 1,
  },
]

/** 终端预览行（B 变体大卡片用） */
export const terminalPreview = [
  '$ claude "fix the flaky ws reconnect test"',
  '  ✓ src/connection/reconnect.ts (3 edits)',
  '  ✓ src/connection/__tests__/reconnect_test.ts',
  '  Running npm run test:run …',
  '  Tests  47 passed (47)',
]

/** 底部导航项（含模拟插件 nav tab，验证"插件扩展位可点"约束） */
export const navTabs = [
  { key: 0, label: '连接', icon: I.monitor },
  { key: 1, label: '会话', icon: I.terminal },
  { key: 2, label: '工具箱', icon: I.toolbox },
  { key: 3, label: '设置', icon: I.gear },
  { key: 4, label: '插件页', icon: I.puzzle, plugin: true },
]

/** 设置分类入口 */
export const settingsCategories = [
  { key: 'connection', label: '连接设置', icon: I.wifi, tone: 'cyan' },
  { key: 'notification', label: '通知', icon: I.bell, tone: 'amber' },
  { key: 'authentication', label: '认证与安全', icon: I.shield, tone: 'emerald' },
  { key: 'appearance', label: '外观', icon: I.palette, tone: 'violet' },
] as const
