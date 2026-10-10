/**
 * 设置域文案（票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 键为域内相对键（'settings.*'）。双语同步（zh-CN / en）。
 * 文案真源参考旧宿主 locales/{zh-CN,en}/settings.ts 的既有措辞
 * （`e92cc40a3^` 版本），保持用户跨形态切换时的认知一致。
 */

export const messagesZhCN = {
  settings: {
    title: '设置',
    loading: '正在加载设置…',
    loadFailed: '设置加载失败，已使用默认值',
    saveFailed: '设置保存失败',
    group: {
      connection: '连接',
      notifications: '通知',
      terminal: '终端',
      auth: '认证',
      platform: '平台设置',
    },
    autoReconnect: '断线自动重连',
    keepAlive: '保持会话连接',
    defaultPort: '默认端口',
    notifyOnWaiting: '会话等待输入时通知',
    notifyOnConnection: '连接建立时通知',
    notifyInBackground: '后台运行时通知',
    vibrate: '振动',
    soundOnTaskComplete: '任务完成提示音',
    maxOpenTerminals: '最大同时打开终端数',
    preferredAuthMethod: '优先认证方式',
    authMethod: {
      pairingCode: '配对码',
      biometric: '生物识别',
    },
    actions: {
      reset: '重置设置',
      resetConfirm: '确定要把业务设置恢复为默认值吗？',
      resetDone: '已重置为默认值',
    },
    platformHint: '外观、语言、关于与出站授权等平台设置请在「我的」页调整',
  },
}

export const messagesEn = {
  settings: {
    title: 'Settings',
    loading: 'Loading settings…',
    loadFailed: 'Failed to load settings, using defaults',
    saveFailed: 'Failed to save settings',
    group: {
      connection: 'Connection',
      notifications: 'Notifications',
      terminal: 'Terminal',
      auth: 'Authentication',
      platform: 'Platform settings',
    },
    autoReconnect: 'Reconnect automatically',
    keepAlive: 'Keep sessions alive',
    defaultPort: 'Default port',
    notifyOnWaiting: 'Notify when a session waits for input',
    notifyOnConnection: 'Notify when connected',
    notifyInBackground: 'Notify while running in background',
    vibrate: 'Vibrate',
    soundOnTaskComplete: 'Sound on task completion',
    maxOpenTerminals: 'Max simultaneous terminals',
    preferredAuthMethod: 'Preferred sign-in method',
    authMethod: {
      pairingCode: 'Pairing code',
      biometric: 'Biometrics',
    },
    actions: {
      reset: 'Reset settings',
      resetConfirm: 'Restore business settings to their default values?',
      resetDone: 'Settings restored to defaults',
    },
    platformHint: 'Appearance, language, about and egress settings live under the Me tab',
  },
}