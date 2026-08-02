import type { MessageSchema } from './messages'

/**
 * 中文（默认）翻译
 *
 * 独立文件维护，构建期由 Vite 打包内联进 bundle，无运行时文件读取。
 */
const zhCN: MessageSchema = {
  title: '自动任务',
  idle: '空闲',
  inProgress: '执行中',
  asking: '等待输入',
  completed: '已完成',
  interrupted: '已中断',
  pending: '待执行',
  autoMode: '自动模式',
  autoModeHint: '队列非空时自动批准权限请求',
  inputPlaceholder: '输入任务内容，回车添加，Shift+回车换行',
  add: '添加',
  clearQueue: '清空',
  confirm: '确认',
  close: '关闭',
  loading: '加载中...',
  emptyQueue: '队列为空',
  emptyHint: '添加任务后将在当前任务完成后自动执行',
  noSession: '请先在终端窗口中打开会话',
  moveUp: '上移',
  moveDown: '下移',
  edit: '编辑',
  save: '保存',
  cancel: '取消',
  delete: '删除',
  clearConfirm: '确定清空全部待执行任务？',
  loadFailed: '加载失败',
  addFailed: '添加任务失败',
  removeFailed: '删除任务失败',
  clearFailed: '清空队列失败',
  updateFailed: '保存失败',
  reorderFailed: '排序失败',
  modeFailed: '切换自动模式失败',
  historyTitle: '任务历史',
  queueTitle: '待执行队列 ({count})',
  historySectionTitle: '历史记录',
  emptyHistory: '暂无任务记录',
  emptyHistoryHint: '启动会话后任务将自动记录',
}

export default zhCN
