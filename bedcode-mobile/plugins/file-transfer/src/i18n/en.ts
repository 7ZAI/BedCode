/**
 * File Transfer plugin English translations
 *
 * Kept in sync with zh-CN at compile time via the plugin's messages index.
 */

import type { MessageSchema } from './messages'

export default {
  // ==================== Toolbox entry ====================
  'transfer.toolbox.title': 'File Transfer',
  'transfer.toolbox.subtitle': 'Fast file transfer over LAN',
  'transfer.toolbox.activeCount': '{count} transferring',
  'transfer.toolbox.disconnected': 'Not connected',

  // ==================== Peer status ====================
  'transfer.peer.online': 'Online',
  'transfer.peer.offline': 'Offline',
  'transfer.peer.unpaired': 'No device connected',

  // ==================== Top bar / browsing ====================
  'transfer.topbar.settings': 'Settings',
  'transfer.topbar.closeSettings': 'Back',
  'transfer.topbar.refresh': 'Refresh',
  'transfer.topbar.queryPeer': 'Re-detect peer',
  'transfer.topbar.downloadSelected': 'Download ({count} · {size})',
  'transfer.topbar.uploadFile': 'Upload file',
  'transfer.breadcrumb.home': 'Files',
  'transfer.table.empty': 'This folder is empty',
  'transfer.table.loading': 'Loading...',
  'transfer.table.dirUnavailable': 'This folder is currently unavailable',
  'transfer.table.selectAll': 'Select all',
  'transfer.table.clearSelection': 'Clear selection',

  // ==================== Task states (spec 9.3 four-color) ====================
  'transfer.task.state.queued': 'Queued',
  'transfer.task.state.transferring': 'Transferring',
  'transfer.task.state.paused': 'Paused',
  'transfer.task.state.resumable': 'Resumable',
  'transfer.task.state.completed': 'Completed',
  'transfer.task.state.failed': 'Failed',
  'transfer.task.state.rejected': 'Rejected',
  'transfer.task.state.cancelled': 'Cancelled',
  'transfer.task.pause': 'Pause',
  'transfer.task.resume': 'Resume',
  'transfer.task.cancel': 'Cancel',
  'transfer.task.retry': 'Retry',
  'transfer.task.resumeAll': 'Resume all',
  'transfer.task.download': 'Download',
  'transfer.task.upload': 'Upload',
  'transfer.task.empty': 'No transfer tasks',
  'transfer.task.progress': '{done} / {total}',
  'transfer.task.reason.duplicateName': 'A file with the same name already exists in the target folder',
  'transfer.task.reason.remoteChanged': "The remote file has changed and can't be resumed. Please start over.",
  'transfer.task.reason.dirUnavailable': 'This folder is currently unavailable',
  'transfer.task.reason.noRoots': 'The remote device has not shared any folders',
  'transfer.task.reason.localNotFound': 'Local file not found',
  'transfer.task.reason.unknown': 'Transfer failed',

  // ==================== Mini transfer bar ====================
  'transfer.minibar.noActive': 'No active transfers',
  'transfer.minibar.speed': '{speed}/s',
  'transfer.minibar.openQueue': 'View queue',

  // ==================== Queue bottom sheet ====================
  'transfer.queue.title': 'Transfer queue',
  'transfer.queue.active': '{count} active',
  'transfer.queue.rejectedChip': 'Rejected',
  'transfer.queue.failedChip': 'Failed',

  // ==================== Settings ====================
  'transfer.settings.title': 'File Transfer Settings',
  'transfer.settings.sharedRoots': 'Shared folders',
  'transfer.settings.addRoot': 'Add folder',
  'transfer.settings.pickRoot': 'Pick folder',
  'transfer.settings.addRootHint': 'Use the system picker (Android only) or enter an absolute local path',
  'transfer.settings.pickFailed': 'Could not resolve the picked folder. Enter the path manually.',
  'transfer.settings.rootDuplicate': 'This folder is already in the share list',
  'transfer.settings.addRootFailed': 'Failed to add: could not mount the shared folder. Check the path.',
  'transfer.settings.removeRoot': 'Remove',
  'transfer.settings.noRoots': 'No shared folders yet. The remote device won\'t see your files.',
  'transfer.settings.downloadDir': 'Download folder',
  'transfer.settings.noDownloadDir': 'Not set',
  'transfer.settings.downloadDirHint': 'Downloads are saved to the system Downloads folder',
  'transfer.settings.concurrency': 'Concurrency',
  'transfer.settings.concurrencyHint': 'Number of files transferred at once (1–8)',
  'transfer.settings.plainWarning': 'Files are transferred unencrypted on your local network. Only use this on trusted WiFi.',
  'transfer.settings.saved': 'Settings saved',

  // ==================== Dialog / notification ====================
  'transfer.dialog.duplicateTitle': 'Upload failed',
  'transfer.dialog.gotIt': 'Got it',
  'transfer.dialog.cancel': 'Cancel',
  'transfer.dialog.localDirPlaceholder': 'Absolute path of a local folder (or use the picker above)',
  'transfer.dialog.localPathPlaceholder': 'Absolute path of the local file (e.g. /storage/emulated/0/Download/a.mp4)',
  'transfer.dialog.uploadTitle': 'Upload file',
  'transfer.notify.doneTitle': 'Transfers complete',
  'transfer.notify.doneBody': '{count} file(s) transferred successfully',
  'transfer.notify.failedTitle': 'Some transfers failed',
  'transfer.notify.failedBody': '{count} file(s) failed. Check the queue.',

  // ==================== Errors (spec §10) ====================
  'transfer.error.duplicateName': 'Upload failed: a file with the same name already exists in the target folder',
  'transfer.error.remoteChanged': "The remote file has changed and can't be resumed. Please start over.",
  'transfer.error.dirUnavailable': 'This folder is currently unavailable',

  // ==================== Empty states ====================
  'transfer.empty.noRoots': 'The remote device has not shared any folders',
  'transfer.empty.noPeer': 'No paired device detected',
  'transfer.empty.noDownloadDir': 'Configure a download folder in Settings first',

  // ==================== Units ====================
  'transfer.size.bytes': '{value} B',
  'transfer.size.kb': '{value} KB',
  'transfer.size.mb': '{value} MB',
  'transfer.size.gb': '{value} GB',
  'transfer.time.justNow': 'Just now',
  'transfer.time.minutesAgo': '{count} min ago',
}
