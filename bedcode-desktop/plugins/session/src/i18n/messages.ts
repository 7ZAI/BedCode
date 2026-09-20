/**
 * Terminal Session Center 插件 i18n 消息类型（唯一 key 来源）
 *
 * zh-CN 与 en 两个语言文件都必须实现该类型：新增/遗漏 key 在编译期即报错，
 * 保证两份语言文件 key 永远同步（spec 票 13「`session.*` 命名空间前缀落定，
 * zh-CN / en 双文件同步」）。
 *
 * 所有 key 以 `session.` 域命名，注册时经插件 ID 前缀隔离为
 * `com.bedcode.session.session.*`（`context.i18n.t('session.x')` 自动补前缀）。
 * 用 type 别名而非 interface：TS 给类型别名隐式索引签名，使 MessageSchema
 * 可直接赋给 `Record<string, unknown>`（registerMessages 入参）。
 */

export type MessageSchema = {
  // ==================== 侧边栏 ====================
  'session.sidebar.title': string

  // ==================== 页签与分组 ====================
  'session.tab.configs': string
  'session.tab.running': string
  'session.section.configs': string
  'session.section.running': string

  // ==================== 空态 ====================
  'session.empty.noConfig': string
  'session.empty.noConfigHint': string
  'session.empty.noSessions': string
  'session.empty.noSessionsHint': string

  // ==================== 按钮 ====================
  'session.button.refresh': string
  'session.button.start': string
  'session.button.stop': string
  'session.button.delete': string
  'session.button.cancel': string
  'session.button.save': string
  'session.button.create': string
  'session.button.edit': string
  'session.button.browse': string

  // ==================== 状态文案 ====================
  'session.status.starting': string
  'session.status.running': string
  'session.status.asking': string
  'session.status.error': string
  'session.status.stopped': string
  'session.status.unknown': string

  // ==================== 运行时长 ====================
  'session.time.secondsAgo': string
  'session.time.minutesSecondsAgo': string
  'session.time.hoursMinutesAgo': string

  // ==================== 终端窗口（本体留宿主，插件只触发） ====================
  'session.terminal.view': string
  'session.terminal.restart': string
  'session.terminal.opening': string
  'session.terminal.openFailed': string

  // ==================== 操作中遮罩 ====================
  'session.operating.starting': string
  'session.operating.stopping': string
  'session.operating.restarting': string
  'session.operating.stoppingAndDeleting': string
  'session.operating.processing': string

  // ==================== 确认弹窗 ====================
  'session.confirm.stopTitle': string
  'session.confirm.stopMsg': string
  'session.confirm.deleteSessionTitle': string
  'session.confirm.deleteRunningMsg': string
  'session.confirm.stopAndDelete': string
  'session.confirm.deleteConfigTitle': string
  'session.confirm.deleteConfigMsg': string

  // ==================== 结果提示 ====================
  'session.toast.started': string
  'session.toast.stopped': string
  'session.toast.restarted': string
  'session.toast.deleted': string
  'session.toast.configDeleted': string
  'session.toast.configUpdated': string
  'session.toast.configCreated': string
  'session.toast.listRefreshed': string

  // ==================== 错误提示 ====================
  'session.error.loadFailed': string
  'session.error.startFailed': string
  'session.error.stopFailed': string
  'session.error.restartFailed': string
  'session.error.deleteFailed': string
  'session.error.saveFailed': string
  'session.error.notRunning': string

  // ==================== 配置表单 ====================
  'session.form.title.new': string
  'session.form.title.edit': string
  'session.form.name': string
  'session.form.namePlaceholder': string
  'session.form.environment': string
  'session.form.wslDistro': string
  'session.form.wslDistroPlaceholder': string
  'session.form.wslInitializing': string
  'session.form.wslNotDetected': string
  'session.form.wslDetectFailed': string
  'session.form.workingDir': string
  'session.form.command': string
  'session.form.commandPlaceholder': string
  'session.form.commandHelp': string
  'session.form.customCommand': string
  'session.form.commandPreset.claude': string
  'session.form.commandPreset.codex': string
  'session.form.commandPreset.pi': string
  'session.form.commandPreset.opencode': string
  'session.form.commandPreset.custom': string
  'session.form.windowsNative': string
  'session.form.linuxNative': string

  // ==================== 设备与配对域（票 14，`pairing.` 命名空间） ====================
  // 文案来源：宿主 `desktop.device.*` / `desktop.sidebar.*` / `settings.pairing.*`
  // 与 `common.*`（逐字复制，保证界面不变），注册时经插件 ID 前缀隔离。

  /** 侧边栏：设备与配对目录项 */
  'pairing.sidebar.title': string
  /** 侧边栏：连接历史目录项 */
  'pairing.history.sidebar.title': string
  /** 设置页贡献分组标题（宿主按 `${pluginId}.${titleKey}` 解析） */
  'pairing.settings.title': string

  // ==================== 网络信息条 ====================
  'pairing.network.title': string
  'pairing.network.ipv4Address': string
  'pairing.network.websocketPort': string
  'pairing.network.noIpv4': string
  'pairing.network.notSelected': string

  // ==================== 页签 ====================
  'pairing.tab.pairing': string
  'pairing.tab.devices': string

  // ==================== 配对码卡 ====================
  'pairing.code.title': string
  'pairing.code.generate': string
  'pairing.code.hint': string
  'pairing.code.placeholder': string
  'pairing.code.request': string
  'pairing.code.generateFailed': string
  'pairing.code.generateFailedNoCode': string

  // ==================== QR 卡 ====================
  'pairing.qr.title': string
  'pairing.qr.generate': string
  'pairing.qr.hint': string
  'pairing.qr.singleUse': string
  'pairing.qr.placeholder': string

  // ==================== 设备列表 ====================
  'pairing.device.sectionOnline': string
  'pairing.device.sectionOffline': string
  'pairing.device.connected': string
  'pairing.device.offline': string
  'pairing.device.pairedAt': string
  'pairing.device.lastSeen': string
  'pairing.device.connectCount': string
  'pairing.device.historyView': string
  'pairing.device.confirmRemove': string
  'pairing.device.confirmRemoveMsg': string
  'pairing.device.removed': string

  // ==================== 连接历史 ====================
  'pairing.history.title': string
  'pairing.history.back': string
  'pairing.history.empty': string
  'pairing.history.clear': string
  'pairing.history.cleared': string
  'pairing.history.clearConfirm': string
  'pairing.history.loadFailed': string
  'pairing.history.statistics': string
  'pairing.history.method.pairingCode': string
  'pairing.history.method.qr': string
  'pairing.history.method.biometric': string
  'pairing.history.method.jwt': string
  'pairing.history.method.unknown': string
  'pairing.history.result.success': string
  'pairing.history.result.failed': string

  // ==================== 设置分组（配对码 / QR 有效期） ====================
  'pairing.settings.qrValidity': string
  'pairing.settings.qrValidityDesc': string
  'pairing.settings.pairingCodeTtl': string
  'pairing.settings.pairingCodeTtlDesc': string
  'pairing.settings.saved': string
  'pairing.settings.saveFailed': string

  // ==================== 通用（宿主 common.* 副本，逐字一致） ====================
  'pairing.button.refresh': string
  'pairing.button.cancel': string
  'pairing.button.remove': string
  'pairing.button.clear': string
  'pairing.time.seconds': string
  'pairing.empty.noData': string
  'pairing.status.unknown': string
  'pairing.status.loading': string

  // ==================== 结果提示与错误 ====================
  'pairing.toast.listRefreshed': string
  'pairing.error.loadFailed': string
  'pairing.error.qrFailed': string
  'pairing.error.revokeFailed': string

  // ==================== 任务域（票 17，`task.` 命名空间） ====================
  // 文案来源：`com.bedcode.auto-task` 插件扁平 key 全部加 `task.` 前缀后并入
  // （spec D6：插件侧扁平 key 必须先加命名空间前缀，否则同注册表后写覆盖）。

  // ==================== 自动任务弹窗 ====================
  'task.title': string
  'task.idle': string
  'task.inProgress': string
  'task.asking': string
  'task.completed': string
  'task.interrupted': string
  'task.pending': string
  'task.autoExecute': string
  'task.autoExecuteHint': string
  'task.autoAnswer': string
  'task.autoAnswerHint': string
  'task.inputPlaceholder': string
  'task.add': string
  'task.clearQueue': string
  'task.confirm': string
  'task.datepickerNow': string
  'task.close': string
  'task.loading': string
  'task.emptyQueue': string
  'task.emptyHint': string
  'task.noSession': string
  'task.moveUp': string
  'task.moveDown': string
  'task.edit': string
  'task.save': string
  'task.cancel': string
  'task.delete': string
  'task.clearConfirm': string
  'task.loadFailed': string
  'task.addFailed': string
  'task.removeFailed': string
  'task.cancelTask': string
  'task.cancelTaskFailed': string
  'task.activeTask': string
  'task.clearFailed': string
  'task.updateFailed': string
  'task.reorderFailed': string
  'task.modeFailed': string
  'task.sessionFlagFailed': string
  // ==================== 任务历史视图 ====================
  'task.historyTitle': string
  'task.queueTitle': string
  'task.emptyHistory': string
  'task.emptyHistoryHint': string
  // ==================== Tab 视图 ====================
  'task.tabsCurrent': string
  'task.tabsRecords': string
  'task.tabsScheduled': string
  'task.tabsStats': string
  // ==================== 当前任务 Tab ====================
  'task.currentTaskTitle': string
  'task.executingTaskTitle': string
  'task.queueCount': string
  'task.createTaskTitle': string
  'task.createTaskSession': string
  'task.saveAsPresetOption': string
  'task.createTaskPromptPlaceholder': string
  'task.createTaskSubmit': string
  'task.createTaskFailed': string
  'task.noRunningSessions': string
  'task.noRunningSessionsHint': string
  'task.agentNotAdapted': string
  // ==================== 预设任务 ====================
  'task.presetTitle': string
  'task.saveAsPreset': string
  'task.createTaskPresetHint': string
  'task.addToQueue': string
  'task.presetAddHint': string
  'task.createPresetFailed': string
  'task.addPresetFailed': string
  'task.deletePresetFailed': string
  // ==================== 任务记录筛选 ====================
  'task.filterStatus': string
  'task.filterAgent': string
  'task.filterSource': string
  'task.sourceUser': string
  'task.sourceQueue': string
  'task.sourceScheduled': string
  'task.filterSince': string
  'task.filterUntil': string
  'task.filterReset': string
  // ==================== 统计条 ====================
  'task.statsTitle': string
  'task.statsTotal': string
  'task.statsSuccessRate': string
  'task.statsAvgDuration': string
  'task.statsCompleted': string
  'task.statsTerminal': string
  'task.durationSeconds': string
  'task.durationMinutes': string
  'task.durationHours': string
  // ==================== 分页 ====================
  'task.paginationRange': string
  'task.paginationPrev': string
  'task.paginationNext': string
  // ==================== 行内详情 ====================
  'task.detailAgent': string
  'task.detailSource': string
  'task.detailCreated': string
  'task.detailStarted': string
  'task.detailCompleted': string
  'task.detailWorkingDir': string
  'task.detailExitReason': string
  'task.detailDescription': string
  // ==================== 定时任务 ====================
  'task.scheduledSectionActive': string
  'task.scheduledSectionFinished': string
  'task.scheduledClearFinished': string
  'task.scheduledEmpty': string
  'task.scheduledEmptyHint': string
  'task.scheduledNew': string
  'task.scheduledName': string
  'task.scheduledConfig': string
  'task.scheduledConfigPlaceholder': string
  'task.scheduledTriggerAt': string
  'task.scheduledUtcHint': string
  'task.scheduledPrompts': string
  'task.scheduledPromptPlaceholder': string
  'task.scheduledPromptsHint': string
  'task.scheduledCreate': string
  'task.scheduledCreateFailed': string
  'task.scheduledDeleteFailed': string
  'task.scheduledReset': string
  'task.scheduledResetFailed': string
  'task.scheduledResetHint': string
  'task.scheduledFormInvalid': string
  'task.scheduledStatusCreating': string
  'task.scheduledStatusExecuted': string
  'task.scheduledStatusFailed': string
  'task.scheduledStatusMissed': string
  'task.scheduledError': string
}
