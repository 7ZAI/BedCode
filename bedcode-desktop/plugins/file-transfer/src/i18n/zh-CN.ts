import type { MessageSchema } from './messages'

/**
 * 中文（默认）翻译
 *
 * 独立文件维护，构建期由 Vite 打包内联进 bundle，无运行时文件读取。
 */
const zhCN: MessageSchema = {
  // ==================== 侧边栏 ====================
  'transfer.sidebar.title': '文件传输',

  // ==================== 对端 pill ====================
  'transfer.peer.online': '已连接',
  'transfer.peer.offline': '未连接',
  'transfer.peer.notSharing': '已连接 · 对端未共享',
  'transfer.peer.unpaired': '未连接设备',
  'transfer.peer.noSharedRoots': '对方尚未设置共享目录',

  // ==================== 顶栏 ====================
  'transfer.topbar.sendToPhone': '发送到手机…',
  'transfer.topbar.downloadSelected': '下载所选 ({count})',
  'transfer.topbar.refresh': '刷新',
  'transfer.topbar.settings': '设置',
  'transfer.topbar.closeSettings': '返回',

  // ==================== 目录表格 ====================
  'transfer.table.name': '名称',
  'transfer.table.size': '大小',
  'transfer.table.modified': '修改时间',
  'transfer.table.empty': '此目录为空',
  'transfer.table.loading': '加载中...',
  'transfer.breadcrumb.home': '文件',

  // ==================== 任务状态 ====================
  'transfer.task.state.queued': '排队',
  'transfer.task.state.transferring': '传输中',
  'transfer.task.state.paused': '已暂停',
  'transfer.task.state.resumable': '可恢复',
  'transfer.task.state.completed': '已完成',
  'transfer.task.state.failed': '失败',
  'transfer.task.state.rejected': '同名被拒',
  'transfer.task.state.cancelled': '已取消',
  'transfer.task.pause': '暂停',
  'transfer.task.resume': '恢复',
  'transfer.task.cancel': '取消',
  'transfer.task.retry': '重新排队',
  'transfer.task.resumeAll': '全部继续',
  'transfer.task.download': '下载',
  'transfer.task.upload': '上传',

  // ==================== 队列汇总 ====================
  'transfer.summary.active': '{count} 传输中',
  'transfer.summary.queued': '{count} 排队',
  'transfer.summary.failed': '{count} 失败',
  'transfer.summary.rejected': '{count} 同名被拒',
  'transfer.summary.speed': '合计 {speed}/s',

  // ==================== 设置 ====================
  'transfer.settings.sharedRoots': '共享目录',
  'transfer.settings.addRoot': '添加目录',
  'transfer.settings.removeRoot': '移除',
  'transfer.settings.noRoots': '尚未添加共享目录',
  'transfer.settings.downloadDir': '下载目录',
  'transfer.settings.noDownloadDir': '未设置',
  'transfer.settings.chooseDir': '选择目录',
  'transfer.settings.concurrency': '并发数',
  'transfer.settings.plainWarning': '文件在本局域网内明文传输，请仅在受信任的 WiFi 网络中使用',

  // ==================== 错误（spec §10） ====================
  'transfer.error.duplicateName': '无法上传：目标目录已存在同名文件',
  'transfer.error.remoteChanged': '远端文件已变化，无法续传，请重新传输',
  'transfer.error.dirUnavailable': '该目录当前不可用',

  // ==================== 空态 ====================
  'transfer.empty.noRoots': '请先在设置中配置共享目录',
  'transfer.empty.noPeer': '未检测到已配对设备',
  'transfer.empty.noDownloadDir': '请先在设置中配置下载目录',

  // ==================== 剩余时间 ====================
  'transfer.eta.seconds': '剩 {count} 秒',
  'transfer.eta.minutes': '剩 {count} 分 {seconds} 秒',
  'transfer.eta.hours': '剩 {count} 小时 {minutes} 分',
}

export default zhCN
