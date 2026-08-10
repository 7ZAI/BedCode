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
  'transfer.task.retry': 'Retry',
  'transfer.task.resumeAll': 'Resume all',
  'transfer.task.download': 'Download',
  'transfer.task.upload': 'Upload',

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

  // ==================== Errors (spec §10) ====================
  'transfer.error.duplicateName': 'Upload failed: a file with the same name already exists in the target folder',
  'transfer.error.remoteChanged': "The remote file has changed and can't be resumed. Please start over.",
  'transfer.error.dirUnavailable': 'This folder is currently unavailable',

  // ==================== Empty states ====================
  'transfer.empty.noRoots': 'Configure shared folders in Settings first',
  'transfer.empty.noRootsHint': 'Add a local folder as a shared root so your peer can browse and download files from it',
  'transfer.empty.noPeer': 'No paired device detected',
  'transfer.empty.noPeerHint': 'Make sure your phone and computer are on the same network and the phone is paired with sharing enabled',
  'transfer.empty.noDownloadDir': 'Configure a download folder in Settings first',
  'transfer.empty.noDownloadDirHint': 'Choose where received files are saved, then you can download files from your peer to this device',

  // ==================== ETA ====================
  'transfer.eta.seconds': '{count}s left',
  'transfer.eta.minutes': '{count}m {seconds}s left',
  'transfer.eta.hours': '{count}h {minutes}m left',
}

export default en
