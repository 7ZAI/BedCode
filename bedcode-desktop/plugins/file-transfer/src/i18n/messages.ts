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
  'transfer.peer.unknown': string
  'transfer.peer.switchTitle': string

  // ==================== 附近设备面板 ====================
  'transfer.devices.title': string
  'transfer.devices.subtitle': string
  'transfer.devices.empty': string
  'transfer.devices.online': string
  'transfer.devices.connected': string
  'transfer.devices.connecting': string
  'transfer.devices.capNone': string
  'transfer.devices.activeCurrent': string
  'transfer.devices.setActive': string
  'transfer.devices.connect': string
  'transfer.devices.disconnect': string
  'transfer.devices.denied': string
  'transfer.devices.unreachable': string
  'transfer.devices.trustHint': string
  /** 探索发现：重新扫描同网节点（面板头部按钮） */
  'transfer.devices.scan': string
  'transfer.devices.scanning': string

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
  'transfer.task.remove': string
  'transfer.task.retry': string
  'transfer.task.resumeAll': string
  'transfer.task.download': string
  'transfer.task.upload': string
  'transfer.task.empty': string
  'transfer.task.receivingEmpty': string
  'transfer.task.waitingApproval': string
  'transfer.task.receiving': string
  'transfer.task.openDir': string

  // ==================== intent 驱动（v2.1） ====================
  'transfer.task.waitingReply': string
  'transfer.task.peerOffline': string

  // ==================== 队列面板 ====================
  'transfer.queue.title': string
  'transfer.queue.count': string

  // ==================== 队列分类 tabs（v2） ====================
  'transfer.queue.all': string
  'transfer.queue.sending': string
  'transfer.queue.receiving': string
  'transfer.queue.history': string

  // ==================== 批量请求应答（v2） ====================
  'transfer.batch.pendingTitle': string
  'transfer.batch.acceptAll': string
  'transfer.batch.rejectAll': string
  'transfer.request.title': string
  'transfer.request.body': string
  'transfer.request.countdown': string
  'transfer.request.acceptAll': string
  'transfer.request.rejectAll': string
  'transfer.toast.receiving': string

  // ==================== 首连确认（spec 决策 6） ====================
  'transfer.consent.title': string
  'transfer.consent.body': string
  'transfer.consent.namelessHint': string
  'transfer.consent.fingerprintLabel': string
  'transfer.consent.countdown': string
  'transfer.consent.accept': string
  'transfer.consent.deny': string
  'transfer.consent.close': string
  'transfer.consent.statusItem': string

  // ==================== 可信对端管理（spec 决策 8） ====================
  'transfer.trusted.title': string
  'transfer.trusted.loading': string
  'transfer.trusted.empty': string
  'transfer.trusted.loadFailed': string
  'transfer.trusted.retry': string
  'transfer.trusted.addedAt': string
  'transfer.trusted.revoke': string
  'transfer.trusted.cancel': string
  'transfer.trusted.revokeTitle': string
  'transfer.trusted.revokeBody': string
  'transfer.trusted.revokeFailed': string

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
  'transfer.settings.receivingPolicy': string
  'transfer.settings.receivingPolicyAsk': string
  'transfer.settings.receivingPolicyAccept': string
  'transfer.settings.receivingPolicyReject': string
  'transfer.settings.receivingPolicyHint': string
  'transfer.settings.approvalTimeout': string
  'transfer.settings.encryption': string
  'transfer.settings.encryptionOn': string
  'transfer.settings.encryptionOff': string
  'transfer.settings.encryptionHint': string

  // ==================== 错误（spec §10 + v2 拒绝原因） ====================
  'transfer.error.duplicateName': string
  'transfer.error.remoteChanged': string
  'transfer.error.dirUnavailable': string
  'transfer.error.rejectedByUser': string
  'transfer.error.noResponse': string
  'transfer.error.policyDenied': string

  // ==================== 传输历史（v2） ====================
  'transfer.history.title': string
  'transfer.history.clear': string
  'transfer.history.empty': string
  'transfer.history.openFolder': string
  'transfer.history.results.completed': string
  'transfer.history.results.failed': string
  'transfer.history.results.rejected': string
  'transfer.history.results.cancelled': string

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
