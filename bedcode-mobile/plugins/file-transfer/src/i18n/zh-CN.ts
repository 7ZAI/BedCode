/**
 * File Transfer 插件中文翻译（默认）
 *
 * 经宿主 context.i18n.registerMessages 注册，key 自动加 `com.bedcode.file-transfer.` 前缀。
 * 规范：{domain}.{section}.{key}，spec §10 四键在错误/设置区。
 */

import type { MessageSchema } from './messages'

export default {
  // ==================== 工具箱入口 ====================
  'transfer.toolbox.title': '文件传输',
  'transfer.toolbox.subtitle': '内网高速互传文件',
  'transfer.toolbox.online': '对端已连接',
  'transfer.toolbox.activeCount': '{count} 传输中',
  'transfer.toolbox.disconnected': '未连接',

  // ==================== 对端状态 ====================
  // 顶栏连接状态：纯连接层语义
  'transfer.peer.online': '已连接',
  'transfer.peer.offline': '未连接',
  // 业务层对端共享：作为页面空态标题，去掉「已连接」前缀避免与顶栏语义混杂
  'transfer.peer.notSharing': '对端未共享',
  'transfer.peer.unpaired': '未连接设备',
  // 已连接但尚未收到对端公告（未共享）：无可辨识信息时的占位名
  'transfer.peer.unknown': '未知设备',

  // ==================== 附近设备面板 ====================
  'transfer.devices.title': '附近设备',
  'transfer.devices.subtitle': '同一网络内的 BedCode 节点',
  'transfer.devices.empty': '未发现附近设备；请确认对方已打开 BedCode 并接入同一网络',
  'transfer.devices.online': '在线',
'transfer.devices.recentSeen': '最近可见',
  'transfer.devices.connected': '已连接',
  'transfer.devices.connecting': '连接中…',
  'transfer.devices.capNone': '不支持传输',
  'transfer.devices.activeCurrent': '当前设备',
  'transfer.devices.setActive': '设为当前',
  'transfer.devices.connect': '连接',
  'transfer.devices.disconnect': '断开',
  'transfer.devices.denied': '对方拒绝了连接',
  'transfer.devices.unreachable': '无法连接，设备可能已离线',
  'transfer.devices.trustHint': '首次连接需对方确认；信任管理见 设置 → 可信对端',
  'transfer.devices.scan': '探索发现',
  'transfer.devices.scanning': '探索中…',

  // ==================== 顶栏 / 浏览 ====================
  'transfer.topbar.settings': '设置',
  'transfer.topbar.closeSettings': '返回',
  'transfer.topbar.refresh': '刷新',
  'transfer.topbar.queryPeer': '重新检测对端',
  'transfer.topbar.downloadSelected': '下载到手机 ({count} 项 · {size})',
  'transfer.topbar.uploadFile': '上传文件',
  'transfer.topbar.connectDevice': '发起连接（附近设备）',
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
  'transfer.task.state.interrupted': '已中断（应用重启）',
  'transfer.task.pause': '暂停',
  'transfer.task.resume': '恢复',
  'transfer.task.cancel': '取消',
  'transfer.task.remove': '删除任务',
  'transfer.task.open': '打开',
  'transfer.task.retry': '重新排队',
  'transfer.task.resumeAll': '全部恢复',
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

  // ==================== 下拉刷新 ====================
  'transfer.pull.pull': '下拉刷新',
  'transfer.pull.ready': '释放立即刷新',
  'transfer.pull.refreshing': '正在刷新…',

  // ==================== 队列 bottom sheet ====================
  'transfer.queue.title': '传输队列',
  'transfer.queue.entry': '{count} 项任务',
  'transfer.queue.active': '{count} 传输中',
  'transfer.queue.rejectedChip': '同名被拒',
  'transfer.queue.failedChip': '失败',

  // ==================== 设置 ====================
  'transfer.settings.title': '文件传输设置',
  'transfer.settings.sharedRoots': '共享目录',
  'transfer.settings.pickRoot': '选择目录',
  'transfer.settings.picking': '选择中…',
  'transfer.settings.pickFailed': '添加失败：目录选择未完成或授权失败，请重试',
  'transfer.settings.pickUnsupported': '当前平台不支持系统目录选择器（仅 Android），无法添加共享目录',
  'transfer.settings.rootDuplicate': '该目录已在共享列表中',
  'transfer.settings.rootInvalid': '已失效',
  'transfer.settings.reauthorize': '重新授权',
  'transfer.settings.reauthorized': '已重新授权',
  'transfer.settings.freeBadge': '免授权',
  'transfer.settings.specialEntryHint': '「免授权」条目为应用私有下载目录（唯一免授权的共享条目，始终可见）',
  'transfer.settings.removeRoot': '移除',
  'transfer.settings.noRoots': '尚未添加共享目录，对端将看不到你的文件',
  'transfer.settings.downloadDir': '下载目录',
  'transfer.settings.noDownloadDir': '未设置',
  'transfer.settings.downloadDirHint': '下载固定保存到系统下载目录',
  'transfer.settings.concurrency': '并发数',
  'transfer.settings.concurrencyHint': '同时传输的文件数（1–8）',
  'transfer.settings.saved': '设置已保存',

  // ==================== 上传页（共享目录） ====================
  'transfer.upload.title': '上传文件',
  'transfer.upload.chooseRoot': '选择共享目录',
  'transfer.upload.noRoots': '尚未添加共享目录，请先在设置中添加',
  'transfer.upload.openSettings': '去设置',
  'transfer.upload.emptyDir': '此目录为空',
  'transfer.upload.loading': '加载中...',
  'transfer.upload.dirUnavailable': '该目录当前不可用',
  'transfer.upload.rootInvalid': '共享目录已失效，请重新授权',
  'transfer.upload.reauthorize': '重新授权',
  'transfer.upload.reauthorized': '已重新授权',
  'transfer.upload.backToRoots': '返回共享目录',
  'transfer.upload.specialBadge': '免授权',
  'transfer.upload.specialEntryHint': '应用私有下载目录（免授权）',
  'transfer.upload.enqueueFailed': '入队失败，请重试',
  'transfer.upload.enqueued': '已加入上传队列',
  'transfer.upload.offline': '对端未连接，无法上传',

  // ==================== 「保存到…」（M3 单文件目标） ====================
  'transfer.saveTo.title': '保存到…',
  'transfer.saveTo.enqueued': '已加入下载队列，完成后可选择保存位置',
  'transfer.saveTo.saved': '已保存到所选位置',
  'transfer.saveTo.failed': '保存失败或已取消，文件保留在应用下载目录',

  // ==================== 对话框 / 通知 ====================
  'transfer.dialog.duplicateTitle': '无法上传',
  'transfer.dialog.gotIt': '知道了',
  'transfer.dialog.cancel': '取消',
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
  'transfer.empty.noDownloadDir': '请先在设置中配置下载目录',
  'transfer.empty.emptyDirHint': '对端共享目录中还没有文件',
  'transfer.empty.notSharingHint': '请在对端设备上开启文件共享',
  'transfer.empty.unavailableHint': '对端可能已断开连接或目录被移除',

  // ==================== 对端存储权限提示 ====================
  'transfer.notice.storageAccessTitle': '对端可能未授予存储访问权限',
  'transfer.notice.storageAccess': '共享的目录位于 Android 顶层存储，需要「所有文件访问权限」才能读取。请在手机系统设置 → 应用 → BedCode → 允许访问所有文件 中授权',

  // ==================== 单位 ====================
  'transfer.size.bytes': '{value} B',
  'transfer.size.kb': '{value} KB',
  'transfer.size.mb': '{value} MB',
  'transfer.size.gb': '{value} GB',
  'transfer.time.justNow': '刚刚',
  'transfer.time.minutesAgo': '{count} 分钟前',

  // ==================== v2 队列 4 tab / 批量批准 / 接收 / 历史 ====================
  'transfer.queue.all': '全部',
  'transfer.queue.sending': '正在发送',
  'transfer.queue.receiving': '正在接收',
  'transfer.queue.history': '历史',
  'transfer.queue.emptyHistory': '暂无传输历史',
  'transfer.task.waitingApproval': '等待对方同意',
  'transfer.task.receiving': '正在接收',
  'transfer.batch.pendingTitle': '文件传输请求',
  'transfer.batch.acceptAll': '接受全部',
  'transfer.batch.rejectAll': '拒绝全部',
  'transfer.request.title': '文件传输请求',
  'transfer.request.body': '{name} 想向你发送 {count} 个文件（共 {size}）',
  'transfer.request.countdown': '将在 {seconds} 秒后自动拒绝',
  'transfer.request.acceptAll': '接受全部',
  'transfer.request.rejectAll': '拒绝全部',
  'transfer.toast.receiving': '{name} 正在向你上传 {count} 个文件',
  'transfer.error.rejectedByUser': '对方拒绝了传输',
  'transfer.error.noResponse': '对方未响应，请求已超时',
  'transfer.error.policyDenied': '对方设置了直接拒绝',
  'transfer.history.title': '历史',
  'transfer.history.clear': '清空历史',
  'transfer.history.empty': '暂无传输历史',
  'transfer.history.openFolder': '打开所在文件夹',
  'transfer.history.results.completed': '已完成',
  'transfer.history.results.failed': '失败',
  'transfer.history.results.rejected': '已拒绝',
  'transfer.history.results.cancelled': '已取消',
  'transfer.settings.receivingPolicy': '接收策略',
  'transfer.settings.receivingPolicyAsk': '每次询问',
  'transfer.settings.receivingPolicyAccept': '直接接收',
  'transfer.settings.receivingPolicyReject': '直接拒绝',
  'transfer.settings.receivingPolicyHint': '对端发送文件前是否需要你同意',
  'transfer.settings.encryption': '传输加密',
  'transfer.settings.encryptionOn': '开',
  'transfer.settings.encryptionOff': '关',
  'transfer.settings.encryptionHint': '开启后，传输内容将自动加密保护，防止被第三方截取',
  'transfer.settings.approvalTimeout': '同意超时（秒）',

  // ==================== 首连确认（全局对话框，spec 决策 7） ====================
  'transfer.consent.title': '连接请求',
  'transfer.consent.body': '{name} 请求连接你的设备以互传文件',
  'transfer.consent.fingerprint': '设备指纹：{fingerprint}',
  'transfer.consent.timeoutHint': '{seconds} 秒内未处理将自动拒绝',
  'transfer.consent.namelessHint': '无法确认对方设备名，请核对指纹后再决定',
  'transfer.consent.trust': '信任',
  'transfer.consent.deny': '拒绝',
  'transfer.consent.autoTrustedToast': '已自动信任配对设备 {name}',

  // ==================== 可信对端管理（spec 决策 8） ====================
  'transfer.trusted.title': '可信对端',
  'transfer.trusted.loading': '加载中...',
  'transfer.trusted.empty': '暂无可信对端；首次连接经确认后建立信任',
  'transfer.trusted.loadFailed': '可信对端列表加载失败',
  'transfer.trusted.retry': '重试',
  'transfer.trusted.addedAt': '加入时间：{time}',
  'transfer.trusted.revoke': '撤销',
  'transfer.trusted.cancel': '取消',
  'transfer.trusted.revokeTitle': '撤销信任',
  'transfer.trusted.revokeBody': '撤销后，{name} 再次连接时需要重新经过你的确认。确定撤销对该设备的信任吗？',
  'transfer.trusted.revokeFailed': '撤销失败，请稍后重试',
  'transfer.trusted.revokedToast': '已撤销对 {name} 的信任',
} satisfies MessageSchema
