/**
 * Auto Task 插件 i18n 消息
 *
 * 独立文件维护，构建期由 Vite 打包内联进 bundle，无运行时文件读取。
 * 经 context.i18n.registerMessages 注册到宿主 vue-i18n（自动加插件 id 前缀，
 * 组件内经 context.i18n.t('key') 访问，无需完整前缀）。
 */

/** 中文（默认）翻译 */
const zhCN: Record<string, string> = {
  title: '自动任务',
  clear: '清空',
  addFromPreset: '从预设添加',
  inputPlaceholder: '输入指令添加到队列...',
  emptyQueue: '暂无自动任务',
  emptyHint: '开启自动执行后，队列中的任务将依次自动执行',
  addFailed: '添加任务失败',
  removeFailed: '删除任务失败',
  clearFailed: '清空队列失败',
  loadFailed: '加载任务队列失败',
  updateFailed: '保存失败',
  reorderFailed: '排序失败',
  modeFailed: '切换自动模式失败',
  clearConfirm: '确定清空全部待执行任务？',
  confirm: '确认',
  cancel: '取消',
  idle: '空闲',
  inProgress: '执行中',
  asking: '等待输入',
  completed: '已完成',
  interrupted: '已中断',
  pending: '待执行',
  unused: '未使用',
  autoExecute: '自动执行',
  autoExecuteHint: '开启后，添加的任务将自动依次执行',
  autoAnswer: '自动应答',
  autoAnswerHint: '开启后，Agent 的提问将自动回答',
  moveUp: '上移',
  moveDown: '下移',
  edit: '编辑',
  delete: '删除',
  loading: '加载中...',
}

/** 英文翻译 */
const en: Record<string, string> = {
  title: 'Auto Task',
  clear: 'Clear',
  addFromPreset: 'Add from Preset',
  inputPlaceholder: 'Enter command to add to queue...',
  emptyQueue: 'No auto tasks',
  emptyHint: 'Enable auto-execute to run queued tasks sequentially',
  addFailed: 'Failed to add task',
  removeFailed: 'Failed to remove task',
  clearFailed: 'Failed to clear queue',
  loadFailed: 'Failed to load task queue',
  updateFailed: 'Failed to save',
  reorderFailed: 'Failed to reorder',
  modeFailed: 'Failed to toggle auto mode',
  clearConfirm: 'Clear all pending tasks?',
  confirm: 'Confirm',
  cancel: 'Cancel',
  idle: 'Idle',
  inProgress: 'In Progress',
  asking: 'Awaiting Input',
  completed: 'Completed',
  interrupted: 'Interrupted',
  pending: 'Pending',
  unused: 'Unused',
  autoExecute: 'Auto Execute',
  autoExecuteHint: 'When enabled, added tasks will auto-execute sequentially',
  autoAnswer: 'Auto Answer',
  autoAnswerHint: 'When enabled, Agent questions will be auto-answered',
  moveUp: 'Move Up',
  moveDown: 'Move Down',
  edit: 'Edit',
  delete: 'Delete',
  loading: 'Loading...',
}

export const messages: Record<string, Record<string, string>> = {
  'zh-CN': zhCN,
  en,
}
