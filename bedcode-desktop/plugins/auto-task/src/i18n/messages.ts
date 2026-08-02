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
}
