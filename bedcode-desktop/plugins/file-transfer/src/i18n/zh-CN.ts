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
  'transfer.peer.unknown': '未知设备',
  'transfer.peer.switchTitle': '切换设备',

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
  'transfer.task.remove': '删除任务',
  'transfer.task.openDir': '打开本地目录',
  'transfer.task.retry': '重新排队',
  'transfer.task.resumeAll': '全部继续',
  'transfer.task.download': '下载',
  'transfer.task.upload': '上传',
  'transfer.task.empty': '暂无任务',
  'transfer.task.receivingEmpty': '暂无接收任务',
  'transfer.task.waitingApproval': '等待对方同意',
  'transfer.task.receiving': '正在接收',

  // ==================== 队列面板 ====================
  'transfer.queue.title': '传输队列',
  'transfer.queue.count': '{count} 项任务',

  // ==================== 队列分类 tabs（v2） ====================
  'transfer.queue.all': '全部',
  'transfer.queue.sending': '正在发送',
  'transfer.queue.receiving': '正在接收',
  'transfer.queue.history': '历史',

  // ==================== 批量请求应答（v2） ====================
  'transfer.batch.pendingTitle': '文件传输请求',
  'transfer.batch.acceptAll': '接受全部',
  'transfer.batch.rejectAll': '拒绝全部',
  'transfer.request.title': '文件传输请求',
  'transfer.request.body': '{name} 想向你发送 {count} 个文件（共 {size}）',
  'transfer.request.countdown': '将在 {seconds} 秒后自动拒绝',
  'transfer.request.acceptAll': '接受全部',
  'transfer.request.rejectAll': '拒绝全部',
  'transfer.toast.receiving': '{name} 正在向你上传 {count} 个文件',

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
  'transfer.settings.concurrencyHint': '同时传输的任务数量，增大可能占用更多带宽',
  'transfer.settings.plainWarning': '文件在本局域网内明文传输，请仅在受信任的 WiFi 网络中使用',
  'transfer.settings.receivingPolicy': '接收策略',
  'transfer.settings.receivingPolicyAsk': '每次询问',
  'transfer.settings.receivingPolicyAccept': '直接接收',
  'transfer.settings.receivingPolicyReject': '直接拒绝',
  'transfer.settings.receivingPolicyHint': '对端发送文件前是否需要你同意',
  'transfer.settings.approvalTimeout': '同意超时（秒）',

  // ==================== 错误（spec §10 + v2 拒绝原因） ====================
  'transfer.error.duplicateName': '无法上传：目标目录已存在同名文件',
  'transfer.error.remoteChanged': '远端文件已变化，无法续传，请重新传输',
  'transfer.error.dirUnavailable': '该目录当前不可用',
  'transfer.error.rejectedByUser': '对方拒绝了传输',
  'transfer.error.noResponse': '对方未响应，请求已超时',
  'transfer.error.policyDenied': '对方设置了直接拒绝',

  // ==================== 传输历史（v2） ====================
  'transfer.history.title': '历史',
  'transfer.history.clear': '清空历史',
  'transfer.history.empty': '暂无传输历史',
  'transfer.history.openFolder': '打开所在文件夹',
  'transfer.history.results.completed': '已完成',
  'transfer.history.results.failed': '失败',
  'transfer.history.results.rejected': '已拒绝',
  'transfer.history.results.cancelled': '已取消',

  // ==================== 空态 ====================
  'transfer.empty.noRoots': '请先在设置中配置共享目录',
  'transfer.empty.noRootsHint': '添加一个本机目录作为共享根，对端即可浏览并下载其中的文件',
  'transfer.empty.noPeer': '未检测到已配对设备',
  'transfer.empty.noPeerHint': '确保手机与电脑在同一局域网，且手机端已配对并开启共享',
  'transfer.empty.noDownloadDir': '请先在设置中配置下载目录',
  'transfer.empty.noDownloadDirHint': '选择接收文件的保存位置，之后就能从对端下载文件到本机',

  // ==================== 对端存储权限提示 ====================
  'transfer.notice.storageAccess': '对端共享的可能是 Android 顶层目录，而手机上未授予「所有文件访问权限」——在手机系统设置 → 应用 → BedCode → 允许访问所有文件 中授权后刷新即可看到内容',

  // ==================== 剩余时间 ====================
  'transfer.eta.seconds': '剩 {count} 秒',
  'transfer.eta.minutes': '剩 {count} 分 {seconds} 秒',
  'transfer.eta.hours': '剩 {count} 小时 {minutes} 分',
}

export default zhCN
