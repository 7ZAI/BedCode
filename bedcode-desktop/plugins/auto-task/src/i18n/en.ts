import type { MessageSchema } from './messages'

/**
 * English translation
 *
 * Maintained in a standalone file; inlined into the bundle at build time by Vite.
 */
const en: MessageSchema = {
  title: 'Auto Tasks',
  idle: 'Idle',
  inProgress: 'In Progress',
  asking: 'Waiting for Input',
  completed: 'Completed',
  interrupted: 'Interrupted',
  pending: 'Pending',
  autoMode: 'Auto Mode',
  autoModeHint: 'Auto-approve permission requests while queue is active',
  inputPlaceholder: 'Enter a task — Enter to add, Shift+Enter for newline',
  add: 'Add',
  clearQueue: 'Clear',
  confirm: 'Confirm',
  close: 'Close',
  loading: 'Loading...',
  emptyQueue: 'Queue is empty',
  emptyHint: 'Tasks run automatically after the current task finishes',
  noSession: 'Open a session in the terminal window first',
  moveUp: 'Move Up',
  moveDown: 'Move Down',
  edit: 'Edit',
  save: 'Save',
  cancel: 'Cancel',
  delete: 'Delete',
  clearConfirm: 'Clear all pending tasks?',
  loadFailed: 'Failed to load',
  addFailed: 'Failed to add task',
  removeFailed: 'Failed to remove task',
  clearFailed: 'Failed to clear queue',
  updateFailed: 'Failed to save',
  reorderFailed: 'Failed to reorder',
  modeFailed: 'Failed to switch auto mode',
  historyTitle: 'Task History',
  queueTitle: 'Pending Queue ({count})',
  historySectionTitle: 'History',
  emptyHistory: 'No task records yet',
  emptyHistoryHint: 'Tasks are recorded automatically after a session starts',
}

export default en
