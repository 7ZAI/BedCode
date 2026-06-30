export default {
  settings: {
    title: '设置',
    network: {
      title: '网络设置',
      websocketPort: 'WebSocket 端口',
    },
    session: {
      title: '会话默认设置',
      defaultEnvironment: '默认执行环境',
      defaultCommand: '默认启动命令',
    },
    qr: {
      title: '二维码设置',
      validity: '二维码有效期',
      validityDesc: '设置二维码的有效时间（60-3600秒）',
    },
    ui: {
      title: '界面设置',
      terminalFontSize: '终端字体大小',
    },
    appearance: {
      title: '外观',
      theme: '主题',
      lightMode: '浅色模式',
      darkMode: '深色模式',
      followSystem: '跟随系统',
      language: '语言',
      fontSize: '字体大小',
      fontSmall: '小',
      fontMedium: '中',
      fontLarge: '大',
      terminalCacheCount: '终端缓存数量',
    },
    connection: {
      title: '连接设置',
      autoReconnect: '自动重连',
      keepAlive: '保持连接',
      reconnectInterval: '重连间隔 (秒)',
      defaultPort: '默认端口',
    },
    notification: {
      title: '通知设置',
      notifyOnWaiting: '等待输入时通知',
      notifyOnConnection: '连接状态变化通知',
      vibrate: '振动反馈',
      notifyInBackground: '后台运行时通知',
      soundOnTaskComplete: '任务完成提示音',
    },
    about: {
      title: '关于',
      githubRepo: 'GitHub 仓库',
      checkUpdate: '检查更新',
      alreadyLatest: '已是最新版本',
    },
    actions: {
      resetSettings: '重置设置',
      clearAllData: '清除所有数据',
      clearDataConfirm: '确定要清除所有数据吗？此操作无法撤销。',
    },
    browser: {
      confirmOpen: '确定要在浏览器中打开此链接吗？',
    },
    shortcuts: {
      title: '快捷键设置',
      description: '配置终端快捷键面板的显示按键',
    },
  },
}
