/**
 * File Transfer 插件中文翻译（默认）
 *
 * 经宿主 context.i18n.registerMessages 注册，key 自动加 `com.bedcode.file-transfer.` 前缀。
 * 规范：{domain}.{section}.{key}，spec §10 四键在错误/设置区。
 */

export default {
  // ==================== 工具箱入口 ====================
  'transfer.toolbox.title': '文件传输',
  'transfer.toolbox.subtitle': '内网高速互传文件',
  'transfer.toolbox.activeCount': '{count} 传输中',
  'transfer.toolbox.disconnected': '未连接',

  // ==================== 对端状态 ====================
  'transfer.peer.online': '在线',
  'transfer.peer.offline': '离线',
  'transfer.peer.unpaired': '未连接设备',

  // ==================== 顶栏 / 浏览 ====================
  'transfer.topbar.settings': '设置',
  'transfer.topbar.closeSettings': '返回',
  'transfer.topbar.refresh': '刷新',
  'transfer.topbar.downloadSelected': '下载到手机 ({count} 项 · {size})',
  'transfer.topbar.uploadFile': '上传文件',
  'transfer.breadcrumb.home': '文件',
  'transfer.table.empty': '此目录为空',
  'transfer.table.loading': '加载中...',
  'transfer.table.dirUnavailable': '该目录当前不可用',
  'transfer.table.selectAll': '全选',
  'transfer.table.clearSelection': '取消选择',

  // ==================== 任务状态（spec 9.3 四色） ====================
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
  'transfer.task.empty': '暂无传输任务',
  'transfer.task.progress': '{done} / {total}',
  'transfer.task.reason.duplicateName': '目标目录已存在同名文件，无法上传',
  'transfer.task.reason.remoteChanged': '远端文件已变化，无法续传，请重新传输',
  'transfer.task.reason.dirUnavailable': '该目录当前不可用',
  'transfer.task.reason.noRoots': '对端尚未设置共享目录',
  'transfer.task.reason.localNotFound': '本地文件不存在',
  'transfer.task.reason.unknown': '传输失败',

  // ==================== 迷你传输条 ====================
  'transfer.minibar.noActive': '没有正在进行的任务',
  'transfer.minibar.speed': '{speed}/s',
  'transfer.minibar.openQueue': '查看队列',

  // ==================== 队列 bottom sheet ====================
  'transfer.queue.title': '传输队列',
  'transfer.queue.active': '{count} 传输中',
  'transfer.queue.rejectedChip': '同名被拒',
  'transfer.queue.failedChip': '失败',

  // ==================== 设置 ====================
  'transfer.settings.title': '文件传输设置',
  'transfer.settings.sharedRoots': '共享目录',
  'transfer.settings.addRoot': '添加目录',
  'transfer.settings.addRootHint': '输入本地目录绝对路径（移动端无目录选择器，需手动输入）',
  'transfer.settings.removeRoot': '移除',
  'transfer.settings.noRoots': '尚未添加共享目录，对端将看不到你的文件',
  'transfer.settings.downloadDir': '下载目录',
  'transfer.settings.noDownloadDir': '未设置',
  'transfer.settings.downloadDirHint': '下载固定保存到系统下载目录',
  'transfer.settings.concurrency': '并发数',
  'transfer.settings.concurrencyHint': '同时传输的文件数（1–8）',
  'transfer.settings.plainWarning': '文件在本局域网内明文传输，请仅在受信任的 WiFi 网络中使用',
  'transfer.settings.saved': '设置已保存',

  // ==================== 对话框 / 通知 ====================
  'transfer.dialog.duplicateTitle': '无法上传',
  'transfer.dialog.gotIt': '知道了',
  'transfer.dialog.cancel': '取消',
  'transfer.dialog.localPathPlaceholder': '本地文件绝对路径（如 /storage/emulated/0/Download/a.mp4）',
  'transfer.dialog.uploadTitle': '上传文件',
  'transfer.notify.doneTitle': '传输完成',
  'transfer.notify.doneBody': '已成功传输 {count} 个文件',
  'transfer.notify.failedTitle': '有传输失败',
  'transfer.notify.failedBody': '{count} 个文件传输失败，请查看队列',

  // ==================== 错误（spec §10） ====================
  'transfer.error.duplicateName': '无法上传：目标目录已存在同名文件',
  'transfer.error.remoteChanged': '远端文件已变化，无法续传，请重新传输',
  'transfer.error.dirUnavailable': '该目录当前不可用',

  // ==================== 空态 ====================
  'transfer.empty.noRoots': '对端尚未设置共享目录',
  'transfer.empty.noPeer': '未检测到已配对设备',
  'transfer.empty.noDownloadDir': '请先在设置中配置下载目录',

  // ==================== 单位 ====================
  'transfer.size.bytes': '{value} B',
  'transfer.size.kb': '{value} KB',
  'transfer.size.mb': '{value} MB',
  'transfer.size.gb': '{value} GB',
  'transfer.time.justNow': '刚刚',
  'transfer.time.minutesAgo': '{count} 分钟前',
}
