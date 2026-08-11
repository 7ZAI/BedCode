/**
 * File Transfer 插件 i18n 消息类型（唯一 key 来源）
 *
 * zh-CN 与 en 两个语言文件都必须实现该接口：
 * 新增/遗漏 key 在编译期即报错，保证两个语言文件的 key 永远同步。
 * 所有 key 以 `transfer.` 域命名，注册时经插件 ID 前缀隔离为
 * `com.bedcode.file-transfer.transfer.*`。
 */

export interface MessageSchema {
  // ==================== 侧边栏 ====================
  'transfer.sidebar.title': string

  // ==================== 对端 pill ====================
  'transfer.peer.online': string
  'transfer.peer.offline': string
  'transfer.peer.notSharing': string
  'transfer.peer.unpaired': string
  'transfer.peer.noSharedRoots': string
  'transfer.peer.switchTitle': string

  // ==================== 顶栏 ====================
  'transfer.topbar.sendToPhone': string
  'transfer.topbar.downloadSelected': string
  'transfer.topbar.refresh': string
  'transfer.topbar.settings': string
  'transfer.topbar.closeSettings': string

  // ==================== 目录表格 ====================
  'transfer.table.name': string
  'transfer.table.size': string
  'transfer.table.modified': string
  'transfer.table.empty': string
  'transfer.table.loading': string
  'transfer.breadcrumb.home': string

  // ==================== 任务状态 ====================
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

  // ==================== 队列面板 ====================
  'transfer.queue.title': string
  'transfer.queue.count': string

  // ==================== 队列汇总 ====================
  'transfer.summary.active': string
  'transfer.summary.queued': string
  'transfer.summary.failed': string
  'transfer.summary.rejected': string
  'transfer.summary.speed': string

  // ==================== 设置 ====================
  'transfer.settings.sharedRoots': string
  'transfer.settings.addRoot': string
  'transfer.settings.removeRoot': string
  'transfer.settings.noRoots': string
  'transfer.settings.downloadDir': string
  'transfer.settings.noDownloadDir': string
  'transfer.settings.chooseDir': string
  'transfer.settings.concurrency': string
  'transfer.settings.concurrencyHint': string
  'transfer.settings.plainWarning': string

  // ==================== 错误（spec §10） ====================
  'transfer.error.duplicateName': string
  'transfer.error.remoteChanged': string
  'transfer.error.dirUnavailable': string

  // ==================== 空态 ====================
  'transfer.empty.noRoots': string
  'transfer.empty.noRootsHint': string
  'transfer.empty.noPeer': string
  'transfer.empty.noPeerHint': string
  'transfer.empty.noDownloadDir': string
  'transfer.empty.noDownloadDirHint': string

  // ==================== 对端存储权限提示 ====================
  'transfer.notice.storageAccess': string

  // ==================== 剩余时间 ====================
  'transfer.eta.seconds': string
  'transfer.eta.minutes': string
  'transfer.eta.hours': string
}
