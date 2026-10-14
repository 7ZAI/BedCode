/**
 * 应用壳域文案（票 2026-10-10：全量 UI 下沉 —— 底部导航 + 页签容器）
 *
 * 键为域内相对键（'app.*' 前缀，与 host 域 'hub.*' / 任务域 'task.*' / 终端域
 * 'terminal.*' 并列）；activate 时经 context.i18n.registerMessages 注册，
 * 取文案经 context.i18n.t('app.*')。双语同步（zh-CN / en）。
 *
 * 视觉真源：旧宿主底部导航 `e92cc40a3^:bedcode-mobile/src/components/MobileNav.vue`
 * （票 2026-10-09 阶段 B 随旧宿主 UI 退役删除，本域逐字复刻其形态与文案口径）。
 */

export const messagesZhCN = {
  app: {
    nav: {
      connection: '连接',
      sessions: '会话',
      toolbox: '工具箱',
      settings: '设置',
      label: '主导航',
    },
    title: '远程终端',
    status: {
      disconnected: '未连接',
      connecting: '连接中…',
      connected: '已连接',
      pairing: '等待配对',
      paired: '已认证',
      error: '连接失败',
      unknown: '未知状态',
    },
    refresh: '刷新',
    reconnecting: '正在重连…',
    terminalBack: '返回会话列表',
  },
}

export const messagesEn = {
  app: {
    nav: {
      connection: 'Connection',
      sessions: 'Sessions',
      toolbox: 'Toolbox',
      settings: 'Settings',
      label: 'Main navigation',
    },
    title: 'Remote Terminal',
    status: {
      disconnected: 'Not connected',
      connecting: 'Connecting…',
      connected: 'Connected',
      pairing: 'Awaiting pairing',
      paired: 'Authenticated',
      error: 'Connection failed',
      unknown: 'Unknown state',
    },
    refresh: 'Refresh',
    reconnecting: 'Reconnecting…',
    terminalBack: 'Back to sessions',
  },
}