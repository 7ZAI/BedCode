import type { MessageSchema } from './messages'

/**
 * English translations
 *
 * Kept in a separate file so the two locales stay in sync at compile time.
 */
const en: MessageSchema = {
  // ==================== Sidebar ====================
  'transfer.sidebar.title': 'File Transfer',

  // ==================== Peer pill ====================
  'transfer.peer.online': 'Connected',
  'transfer.peer.offline': 'Not connected',
  'transfer.peer.notSharing': 'Connected · peer not sharing',
  'transfer.peer.unpaired': 'No device connected',
  'transfer.peer.noSharedRoots': "Peer hasn't shared any folders yet",
  'transfer.peer.unknown': 'Unknown device',
  'transfer.peer.switchTitle': 'Switch device',

  // ==================== Top bar ====================
  'transfer.topbar.sendToPhone': 'Send to phone…',
  'transfer.topbar.downloadSelected': 'Download selected ({count})',
  'transfer.topbar.refresh': 'Refresh',
  'transfer.topbar.settings': 'Settings',
  'transfer.topbar.closeSettings': 'Back',

  // ==================== File table ====================
  'transfer.table.name': 'Name',
  'transfer.table.size': 'Size',
  'transfer.table.modified': 'Modified',
  'transfer.table.empty': 'This folder is empty',
  'transfer.table.loading': 'Loading...',
  'transfer.breadcrumb.home': 'Files',

  // ==================== Task states ====================
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
  'transfer.task.openDir': 'Open local folder',
  'transfer.task.retry': 'Retry',
  'transfer.task.resumeAll': 'Resume all',
  'transfer.task.download': 'Download',
  'transfer.task.upload': 'Upload',
  'transfer.task.empty': 'No tasks',
  'transfer.task.receivingEmpty': 'Nothing being received',
  'transfer.task.waitingApproval': 'Waiting for approval',
  'transfer.task.receiving': 'Receiving',

  // ==================== Queue panel ====================
  'transfer.queue.title': 'Transfer queue',
  'transfer.queue.count': '{count} tasks',

  // ==================== Queue tabs (v2) ====================
  'transfer.queue.all': 'All',
  'transfer.queue.sending': 'Sending',
  'transfer.queue.receiving': 'Receiving',
  'transfer.queue.history': 'History',

  // ==================== Batch request (v2) ====================
  'transfer.batch.pendingTitle': 'File transfer request',
  'transfer.batch.acceptAll': 'Accept all',
  'transfer.batch.rejectAll': 'Reject all',
  'transfer.request.title': 'File transfer request',
  'transfer.request.body': '{name} wants to send you {count} files ({size} total)',
  'transfer.request.countdown': 'Auto-reject in {seconds}s',
  'transfer.request.acceptAll': 'Accept all',
  'transfer.request.rejectAll': 'Reject all',
  'transfer.toast.receiving': '{name} is sending you {count} files',

  // ==================== Queue summary ====================
  'transfer.summary.active': '{count} active',
  'transfer.summary.queued': '{count} queued',
  'transfer.summary.failed': '{count} failed',
  'transfer.summary.rejected': '{count} rejected',
  'transfer.summary.speed': '{speed}/s total',

  // ==================== Settings ====================
  'transfer.settings.sharedRoots': 'Shared folders',
  'transfer.settings.addRoot': 'Add folder',
  'transfer.settings.removeRoot': 'Remove',
  'transfer.settings.noRoots': 'No shared folders yet',
  'transfer.settings.downloadDir': 'Download folder',
  'transfer.settings.noDownloadDir': 'Not set',
  'transfer.settings.chooseDir': 'Choose folder',
  'transfer.settings.concurrency': 'Concurrency',
  'transfer.settings.concurrencyHint': 'Number of simultaneous transfers; increasing it may use more bandwidth',
  'transfer.settings.plainWarning': 'Files are transferred unencrypted on your local network. Only use this on trusted WiFi.',
  'transfer.settings.receivingPolicy': 'Receiving policy',
  'transfer.settings.receivingPolicyAsk': 'Ask every time',
  'transfer.settings.receivingPolicyAccept': 'Accept automatically',
  'transfer.settings.receivingPolicyReject': 'Reject automatically',
  'transfer.settings.receivingPolicyHint': 'Whether to ask before receiving files from peers',
  'transfer.settings.approvalTimeout': 'Approval timeout (s)',

  // ==================== Errors (spec §10 + v2 reject reasons) ====================
  'transfer.error.duplicateName': 'Upload failed: a file with the same name already exists in the target folder',
  'transfer.error.remoteChanged': "The remote file has changed and can't be resumed. Please start over.",
  'transfer.error.dirUnavailable': 'This folder is currently unavailable',
  'transfer.error.rejectedByUser': 'The transfer was rejected by the peer',
  'transfer.error.noResponse': 'No response from the peer; the request timed out',
  'transfer.error.policyDenied': 'The peer is set to reject incoming transfers',

  // ==================== Transfer history (v2) ====================
  'transfer.history.title': 'History',
  'transfer.history.clear': 'Clear history',
  'transfer.history.empty': 'No transfer history',
  'transfer.history.openFolder': 'Show in folder',
  'transfer.history.results.completed': 'Completed',
  'transfer.history.results.failed': 'Failed',
  'transfer.history.results.rejected': 'Rejected',
  'transfer.history.results.cancelled': 'Cancelled',

  // ==================== Empty states ====================
  'transfer.empty.noRoots': 'Configure shared folders in Settings first',
  'transfer.empty.noRootsHint': 'Add a local folder as a shared root so your peer can browse and download files from it',
  'transfer.empty.noPeer': 'No paired device detected',
  'transfer.empty.noPeerHint': 'Make sure your phone and computer are on the same network and the phone is paired with sharing enabled',
  'transfer.empty.noDownloadDir': 'Configure a download folder in Settings first',
  'transfer.empty.noDownloadDirHint': 'Choose where received files are saved, then you can download files from your peer to this device',

  // ==================== Peer storage permission notice ====================
  'transfer.notice.storageAccess': 'The peer may share an Android top-level folder, but "All files access" is not granted on the phone. Grant it in System settings → Apps → BedCode → Allow all files, then refresh to see the contents',

  // ==================== ETA ====================
  'transfer.eta.seconds': '{count}s left',
  'transfer.eta.minutes': '{count}m {seconds}s left',
  'transfer.eta.hours': '{count}h {minutes}m left',
}

export default en
