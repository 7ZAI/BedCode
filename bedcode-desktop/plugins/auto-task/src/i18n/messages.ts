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
  autoExecute: string
  autoExecuteHint: string
  autoAnswer: string
  autoAnswerHint: string
  inputPlaceholder: string
  add: string
  clearQueue: string
  confirm: string
  datepickerNow: string
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
  cancelTask: string
  cancelTaskFailed: string
  activeTask: string
  clearFailed: string
  updateFailed: string
  reorderFailed: string
  modeFailed: string
  sessionFlagFailed: string
  // ==================== 任务历史视图 ====================
  historyTitle: string
  queueTitle: string
  emptyHistory: string
  emptyHistoryHint: string
  // ==================== Tab 视图 ====================
  tabsCurrent: string
  tabsRecords: string
  tabsScheduled: string
  tabsStats: string
  // ==================== 当前任务 Tab ====================
  currentTaskTitle: string
  executingTaskTitle: string
  queueCount: string
  createTaskTitle: string
  createTaskSession: string
  saveAsPresetOption: string
  createTaskPromptPlaceholder: string
  createTaskSubmit: string
  createTaskFailed: string
  noRunningSessions: string
  noRunningSessionsHint: string
  agentNotAdapted: string
  // ==================== 预设任务 ====================
  presetTitle: string
  saveAsPreset: string
  createTaskPresetHint: string
  addToQueue: string
  presetAddHint: string
  createPresetFailed: string
  addPresetFailed: string
  deletePresetFailed: string
  // ==================== 任务记录筛选 ====================
  filterStatus: string
  filterAgent: string
  filterSource: string
  sourceUser: string
  sourceQueue: string
  sourceScheduled: string
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
  // ==================== 定时任务 ====================
  scheduledSectionActive: string
  scheduledSectionFinished: string
  scheduledClearFinished: string
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
  scheduledPromptsHint: string
  scheduledCreate: string
  scheduledCreateFailed: string
  scheduledDeleteFailed: string
  scheduledReset: string
  scheduledResetFailed: string
  scheduledResetHint: string
  scheduledFormInvalid: string
  scheduledStatusCreating: string
  scheduledStatusExecuted: string
  scheduledStatusFailed: string
  scheduledStatusMissed: string
  scheduledError: string
}
