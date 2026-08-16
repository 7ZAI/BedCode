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
  'transfer.toolbox.online': 'Peer online',
  'transfer.toolbox.activeCount': '{count} transferring',
  'transfer.toolbox.disconnected': 'Not connected',

  // ==================== Peer status ====================
  // Top bar connection status: connection-layer only
  'transfer.peer.online': 'Connected',
  'transfer.peer.offline': 'Not connected',
  // Business-layer peer sharing: used as the empty-state title; drop the
  // "Connected" prefix so it doesn't collide with the top bar's meaning.
  'transfer.peer.notSharing': 'Peer not sharing',
  'transfer.peer.unpaired': 'No device connected',
  // Connected but peer announcement not received yet: placeholder name
  'transfer.peer.unknown': 'Unknown device',

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
  'transfer.task.remove': 'Remove task',
  'transfer.task.open': 'Open',
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

  // ==================== Pull to refresh ====================
  'transfer.pull.pull': 'Pull to refresh',
  'transfer.pull.ready': 'Release to refresh',
  'transfer.pull.refreshing': 'Refreshing…',

  // ==================== Queue bottom sheet ====================
  'transfer.queue.title': 'Transfer queue',
  'transfer.queue.entry': '{count} tasks',
  'transfer.queue.active': '{count} active',
  'transfer.queue.rejectedChip': 'Rejected',
  'transfer.queue.failedChip': 'Failed',

  // ==================== Settings ====================
  'transfer.settings.title': 'File Transfer Settings',
  'transfer.settings.sharedRoots': 'Shared folders',
  'transfer.settings.addRootHint': 'Shared folders are picked with the system directory picker and stay authorized across restarts. You can browse and upload from them inside the app without the "All files access" permission.',
  'transfer.settings.pickRoot': 'Pick folder',
  'transfer.settings.picking': 'Picking…',
  'transfer.settings.pickFailed': 'Failed to add: folder selection incomplete or authorization failed. Try again.',
  'transfer.settings.pickUnsupported': 'The system directory picker is Android-only and unavailable here.',
  'transfer.settings.rootDuplicate': 'This folder is already in the share list',
  'transfer.settings.rootInvalid': 'Invalidated',
  'transfer.settings.reauthorize': 'Re-authorize',
  'transfer.settings.reauthorized': 'Re-authorized',
  'transfer.settings.freeBadge': 'No auth needed',
  'transfer.settings.specialEntryHint': 'The "No auth needed" entry is the app private downloads folder (the only auth-free shared entry, always visible).',
  'transfer.settings.removeRoot': 'Remove',
  'transfer.settings.noRoots': 'No shared folders yet. The remote device won\'t see your files.',
  'transfer.settings.downloadDir': 'Download folder',
  'transfer.settings.noDownloadDir': 'Not set',
  'transfer.settings.downloadDirHint': 'Downloads are saved to the system Downloads folder',
  'transfer.settings.concurrency': 'Concurrency',
  'transfer.settings.concurrencyHint': 'Number of files transferred at once (1–8)',
  'transfer.settings.plainWarning': 'Files are transferred unencrypted on your local network. Only use this on trusted WiFi.',
  'transfer.settings.saved': 'Settings saved',

  // ==================== Upload page (shared directories) ====================
  'transfer.upload.title': 'Upload file',
  'transfer.upload.chooseRoot': 'Choose a shared folder',
  'transfer.upload.noRoots': 'No shared folders yet. Add one in Settings first.',
  'transfer.upload.openSettings': 'Open Settings',
  'transfer.upload.emptyDir': 'This folder is empty',
  'transfer.upload.loading': 'Loading...',
  'transfer.upload.dirUnavailable': 'This folder is currently unavailable',
  'transfer.upload.rootInvalid': 'This shared folder is invalid. Re-authorize it.',
  'transfer.upload.reauthorize': 'Re-authorize',
  'transfer.upload.reauthorized': 'Re-authorized',
  'transfer.upload.backToRoots': 'Back to shared folders',
  'transfer.upload.specialBadge': 'No auth needed',
  'transfer.upload.specialEntryHint': 'App private downloads folder (no auth needed)',
  'transfer.upload.enqueueFailed': 'Enqueue failed. Try again.',
  'transfer.upload.enqueued': 'Added to the upload queue',
  'transfer.upload.offline': 'The peer is offline; cannot upload',

  // ==================== Save to… (M3 single-file destination) ====================
  'transfer.saveTo.title': 'Save to…',
  'transfer.saveTo.enqueued': 'Added to download queue; choose a location when done',
  'transfer.saveTo.saved': 'Saved to the selected location',
  'transfer.saveTo.failed': 'Save failed or cancelled; file kept in app downloads',

  // ==================== Dialog / notification ====================
  'transfer.dialog.duplicateTitle': 'Upload failed',
  'transfer.dialog.gotIt': 'Got it',
  'transfer.dialog.cancel': 'Cancel',
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
  'transfer.empty.noDownloadDir': 'Configure a download folder in Settings first',
  'transfer.empty.emptyDirHint': "No files in the peer's shared folder yet",
  'transfer.empty.notSharingHint': 'Ask the peer to enable file sharing',
  'transfer.empty.unavailableHint': 'The peer may be offline or the folder was removed',

  // ==================== Peer storage permission notice ====================
  'transfer.notice.storageAccessTitle': 'The peer may not have storage access granted',
  'transfer.notice.storageAccess': 'The shared folder lives in Android top-level storage, which requires "All files access" to read. Grant it in System settings → Apps → BedCode → Allow all files',

  // ==================== Units ====================
  'transfer.size.bytes': '{value} B',
  'transfer.size.kb': '{value} KB',
  'transfer.size.mb': '{value} MB',
  'transfer.size.gb': '{value} GB',
  'transfer.time.justNow': 'Just now',
  'transfer.time.minutesAgo': '{count} min ago',

  // ==================== v2 queue tabs / batch approval / receiving / history ====================
  'transfer.queue.all': 'All',
  'transfer.queue.sending': 'Sending',
  'transfer.queue.receiving': 'Receiving',
  'transfer.queue.history': 'History',
  'transfer.queue.emptyHistory': 'No transfer history',
  'transfer.task.waitingApproval': 'Waiting for approval',
  'transfer.task.receiving': 'Receiving',
  'transfer.batch.pendingTitle': 'File transfer request',
  'transfer.batch.acceptAll': 'Accept all',
  'transfer.batch.rejectAll': 'Reject all',
  'transfer.request.title': 'File transfer request',
  'transfer.request.body': '{name} wants to send you {count} files ({size} total)',
  'transfer.request.countdown': 'Auto-reject in {seconds}s',
  'transfer.request.acceptAll': 'Accept all',
  'transfer.request.rejectAll': 'Reject all',
  'transfer.toast.receiving': '{name} is sending you {count} files',
  'transfer.error.rejectedByUser': 'The transfer was rejected by the peer',
  'transfer.error.noResponse': 'No response from the peer; the request timed out',
  'transfer.error.policyDenied': 'The peer is set to reject incoming transfers',
  'transfer.history.title': 'History',
  'transfer.history.clear': 'Clear history',
  'transfer.history.empty': 'No transfer history',
  'transfer.history.openFolder': 'Show in folder',
  'transfer.history.results.completed': 'Completed',
  'transfer.history.results.failed': 'Failed',
  'transfer.history.results.rejected': 'Rejected',
  'transfer.history.results.cancelled': 'Cancelled',
  'transfer.settings.receivingPolicy': 'Receiving policy',
  'transfer.settings.receivingPolicyAsk': 'Ask every time',
  'transfer.settings.receivingPolicyAccept': 'Accept automatically',
  'transfer.settings.receivingPolicyReject': 'Reject automatically',
  'transfer.settings.receivingPolicyHint': 'Whether to ask before receiving files from peers',
  'transfer.settings.approvalTimeout': 'Approval timeout (s)',
}
