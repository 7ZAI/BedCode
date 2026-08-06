/**
 * Auto Task 插件 i18n 消息类型（唯一 key 来源）
 *
 * zh-CN 与 en 两个语言文件都必须实现该接口：
 * 新增/遗漏 key 在编译期即报错，保证两个语言文件的 key 永远同步。
 */

export interface MessageSchema {
  // ==================== 自动任务弹窗 ====================
  title: string
  idle: string
  inProgress: string
  asking: string
  completed: string
  interrupted: string
  pending: string
  autoMode: string
  autoModeHint: string
  inputPlaceholder: string
  add: string
  clearQueue: string
  confirm: string
  close: string
  loading: string
  emptyQueue: string
  emptyHint: string
  noSession: string
  moveUp: string
  moveDown: string
  edit: string
  save: string
  cancel: string
  delete: string
  clearConfirm: string
  loadFailed: string
  addFailed: string
  removeFailed: string
  clearFailed: string
  updateFailed: string
  reorderFailed: string
  modeFailed: string
  // ==================== 任务历史视图 ====================
  historyTitle: string
  queueTitle: string
  historySectionTitle: string
  emptyHistory: string
  emptyHistoryHint: string
  // ==================== 双 Tab 视图 ====================
  tabsRecords: string
  tabsScheduled: string
  tabsStats: string
  // ==================== 任务记录筛选 ====================
  filterStatus: string
  filterAgent: string
  filterSource: string
  filterSince: string
  filterUntil: string
  filterReset: string
  // ==================== 统计条 ====================
  statsTitle: string
  statsTotal: string
  statsSuccessRate: string
  statsAvgDuration: string
  statsCompleted: string
  statsTerminal: string
  durationSeconds: string
  durationMinutes: string
  durationHours: string
  // ==================== 分页 ====================
  paginationRange: string
  paginationPrev: string
  paginationNext: string
  // ==================== 行内详情 ====================
  detailAgent: string
  detailSource: string
  detailCreated: string
  detailStarted: string
  detailCompleted: string
  detailWorkingDir: string
  detailExitReason: string
  detailDescription: string
  // ==================== 当前任务 ====================
  currentTaskTitle: string
  // ==================== 定时任务 ====================
  scheduledEmpty: string
  scheduledEmptyHint: string
  scheduledNew: string
  scheduledName: string
  scheduledConfig: string
  scheduledConfigPlaceholder: string
  scheduledTriggerAt: string
  scheduledUtcHint: string
  scheduledPrompts: string
  scheduledPromptPlaceholder: string
  scheduledAddPrompt: string
  scheduledRemovePrompt: string
  scheduledCreate: string
  scheduledCreateFailed: string
  scheduledDeleteFailed: string
  scheduledFormInvalid: string
  scheduledStatusCreating: string
  scheduledStatusExecuted: string
  scheduledStatusFailed: string
  scheduledStatusMissed: string
  scheduledError: string
}
