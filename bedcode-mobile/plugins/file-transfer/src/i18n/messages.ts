/**
 * File Transfer 插件 i18n 消息 schema（编译期 key 强制）
 *
 * 与桌面端对齐：zh-CN / en 均标注为 MessageSchema，
 * 新增 key 必须同时出现在本接口与两个语言文件中，否则编译失败。
 */
export interface MessageSchema {
  // ==================== 工具箱入口 ====================
  'transfer.toolbox.title': string
  'transfer.toolbox.subtitle': string
  'transfer.toolbox.activeCount': string
  'transfer.toolbox.disconnected': string
  'transfer.toolbox.online': string

  // ==================== 对端状态 ====================
  // 顶栏连接状态：仅表达 WS 控制面是否已建立
  'transfer.peer.online': string
  'transfer.peer.offline': string
  // 业务层对端共享：作为页面空态标题使用，不再含「已连接」前缀
  'transfer.peer.notSharing': string
  'transfer.peer.unpaired': string
  'transfer.peer.unknown': string

  // ==================== 顶栏 / 浏览 ====================
  'transfer.topbar.settings': string
  'transfer.topbar.closeSettings': string
  'transfer.topbar.refresh': string
  'transfer.topbar.queryPeer': string
  'transfer.topbar.downloadSelected': string
  'transfer.topbar.uploadFile': string
  'transfer.breadcrumb.home': string
  'transfer.table.empty': string
  'transfer.table.loading': string
  'transfer.table.dirUnavailable': string
  'transfer.table.selectAll': string
  'transfer.table.clearSelection': string

  // ==================== 任务状态（spec 9.3 四色） ====================
  'transfer.task.state.queued': string
  'transfer.task.state.transferring': string
  'transfer.task.state.paused': string
  'transfer.task.state.resumable': string
  'transfer.task.state.completed': string
  'transfer.task.state.failed': string
  'transfer.task.state.rejected': string
  'transfer.task.state.cancelled': string
  'transfer.task.pause': string
  'transfer.task.resume': string
  'transfer.task.cancel': string
  'transfer.task.retry': string
  'transfer.task.resumeAll': string
  'transfer.task.download': string
  'transfer.task.upload': string
  'transfer.task.empty': string
  'transfer.task.progress': string
  'transfer.task.reason.duplicateName': string
  'transfer.task.reason.remoteChanged': string
  'transfer.task.reason.dirUnavailable': string
  'transfer.task.reason.noRoots': string
  'transfer.task.reason.localNotFound': string
  'transfer.task.reason.unknown': string

  // ==================== 迷你传输条 ====================
  'transfer.minibar.noActive': string
  'transfer.minibar.speed': string
  'transfer.minibar.openQueue': string

  // ==================== 队列 bottom sheet ====================
  'transfer.queue.title': string
  'transfer.queue.active': string
  'transfer.queue.rejectedChip': string
  'transfer.queue.failedChip': string

  // ==================== 设置 ====================
  'transfer.settings.title': string
  'transfer.settings.sharedRoots': string
  'transfer.settings.addRoot': string
  'transfer.settings.pickRoot': string
  'transfer.settings.picking': string
  'transfer.settings.addRootHint': string
  'transfer.settings.scopedStorageHint': string
  'transfer.settings.grantAllFilesAccess': string
  'transfer.settings.granting': string
  'transfer.settings.allFilesAccessGranted': string
  'transfer.settings.allFilesAccessUnavailable': string
  'transfer.settings.removeRoot': string
  'transfer.settings.noRoots': string
  'transfer.settings.downloadDir': string
  'transfer.settings.noDownloadDir': string
  'transfer.settings.downloadDirHint': string
  'transfer.settings.concurrency': string
  'transfer.settings.concurrencyHint': string
  'transfer.settings.plainWarning': string
  'transfer.settings.saved': string

  // ==================== 对话框 / 通知 ====================
  'transfer.dialog.duplicateTitle': string
  'transfer.dialog.gotIt': string
  'transfer.dialog.cancel': string
  'transfer.dialog.localPathPlaceholder': string
  'transfer.dialog.uploadTitle': string
  'transfer.notify.doneTitle': string
  'transfer.notify.doneBody': string
  'transfer.notify.failedTitle': string
  'transfer.notify.failedBody': string

  // ==================== 错误（spec §10） ====================
  'transfer.error.duplicateName': string
  'transfer.error.remoteChanged': string
  'transfer.error.dirUnavailable': string

  // ==================== 空态 ====================
  'transfer.empty.noRoots': string
  'transfer.empty.noDownloadDir': string
  'transfer.empty.emptyDirHint': string
  'transfer.empty.notSharingHint': string
  'transfer.empty.unavailableHint': string

  // ==================== 对端存储权限提示 ====================
  'transfer.notice.storageAccessTitle': string
  'transfer.notice.storageAccess': string

  // ==================== 单位 ====================
  'transfer.size.bytes': string
  'transfer.size.kb': string
  'transfer.size.mb': string
  'transfer.size.gb': string
  'transfer.time.justNow': string
  'transfer.time.minutesAgo': string
}
