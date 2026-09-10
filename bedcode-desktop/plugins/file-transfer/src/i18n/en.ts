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
  'transfer.peer.switchTitle': 'Nearby devices (initiate connection)',

  // ==================== Nearby devices panel ====================
  'transfer.devices.title': 'Nearby Devices',
  'transfer.devices.subtitle': 'BedCode nodes on this network',
  'transfer.devices.empty':
    'No nearby devices found; make sure the other device has BedCode open and is on the same network',
  'transfer.devices.online': 'Online',
  'transfer.devices.recentSeen': 'Recently seen',
  'transfer.devices.connected': 'Connected',
  'transfer.devices.connecting': 'Connecting…',
  'transfer.devices.capNone': 'No transfer',
  'transfer.devices.activeCurrent': 'Current',
  'transfer.devices.setActive': 'Set active',
  'transfer.devices.connect': 'Connect',
  'transfer.devices.disconnect': 'Disconnect',
  'transfer.devices.denied': 'Connection rejected by the other device',
  'transfer.devices.unreachable': 'Cannot connect; the device may be offline',
  'transfer.devices.trustHint':
    'First-time connections need approval on the other side; manage trust in Settings → Trusted peers',
  'transfer.devices.scan': 'Discover',
  'transfer.devices.scanning': 'Discovering…',

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
  'transfer.task.state.interrupted': 'Interrupted (app restarted)',
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

  // ==================== Intent-driven (v2.1) ====================
  'transfer.task.waitingReply': 'Waiting for reply',
  'transfer.task.peerOffline': 'Peer offline, task paused',

  // ==================== Queue panel ====================
  'transfer.queue.title': 'Transfer queue',
  'transfer.queue.count': '{count} tasks',

  // ==================== Queue tabs (v2) ====================
  'transfer.queue.all': 'All',
  'transfer.queue.sending': 'Sending',
  'transfer.queue.receiving': 'Receiving',
  'transfer.queue.history': 'History',

  // ==================== Batch request (v2) ====================
  'transfer.request.title': 'File transfer request',
  'transfer.request.body': '{name} wants to send you {count} files ({size} total)',
  'transfer.request.countdown': 'Auto-reject in {seconds}s',
  'transfer.request.acceptAll': 'Accept all',
  'transfer.request.rejectAll': 'Reject all',
  'transfer.toast.receiving': '{name} is sending you {count} files',

  // ==================== First-connect consent (spec decision 6) ====================
  'transfer.consent.title': 'Connection request',
  'transfer.consent.body': '{name} wants to connect for file transfer',
  'transfer.consent.namelessHint':
    "Device name unavailable — verify the fingerprint below before deciding",
  'transfer.consent.fingerprintLabel': 'Device fingerprint',
  'transfer.consent.copy': 'Copy fingerprint',
  'transfer.consent.copied': 'Copied',
  'transfer.consent.copyHint': 'Copy the full fingerprint to compare on another device',
  'transfer.consent.countdown': 'Auto-reject in {seconds}s',
  'transfer.consent.accept': 'Accept',
  'transfer.consent.deny': 'Reject',
  'transfer.consent.close': 'Close (same as reject)',

  // ==================== Trusted peers (spec decision 8) ====================
  'transfer.trusted.title': 'Trusted peers',
  'transfer.trusted.loading': 'Loading...',
  'transfer.trusted.empty': 'No trusted peers yet; trust is established after the other side approves your first connection',
  'transfer.trusted.loadFailed': 'Failed to load trusted peers',
  'transfer.trusted.retry': 'Retry',
  'transfer.trusted.addedAt': 'Added: {time}',
  'transfer.trusted.revoke': 'Revoke',
  'transfer.trusted.cancel': 'Cancel',
  'transfer.trusted.revokeTitle': 'Revoke trust',
  'transfer.trusted.revokeBody': 'After revoking, {name} will need your approval again on the next connection. Revoke trust for this device?',
  'transfer.trusted.revokeFailed': 'Revoke failed; please try again later',

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
  'transfer.settings.concurrencyHint':
    'Number of simultaneous transfers; increasing it may use more bandwidth',
  'transfer.settings.plainWarning':
    'Files are transferred unencrypted on your local network. Only use this on trusted WiFi.',
  'transfer.settings.receivingPolicy': 'Receiving policy',
  'transfer.settings.receivingPolicyAsk': 'Ask every time',
  'transfer.settings.receivingPolicyAccept': 'Accept automatically',
  'transfer.settings.receivingPolicyReject': 'Reject automatically',
  'transfer.settings.receivingPolicyHint': 'Whether to ask before receiving files from peers',
  'transfer.settings.approvalTimeout': 'Approval timeout (s)',
  'transfer.settings.encryption': 'Transfer encryption',
  'transfer.settings.encryptionOn': 'On',
  'transfer.settings.encryptionOff': 'Off',
  'transfer.settings.encryptionHint':
    'When enabled, sent files are end-to-end encrypted with AES-256-GCM; the peer must also run a recent BedCode version to decrypt them automatically',

  // ==================== Errors (spec §10 + v2 reject reasons) ====================
  'transfer.error.duplicateName':
    'Upload failed: a file with the same name already exists in the target folder',
  'transfer.error.remoteChanged':
    "The remote file has changed and can't be resumed. Please start over.",
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
  'transfer.empty.noRootsHint':
    'Add a local folder as a shared root so your peer can browse and download files from it',
  'transfer.empty.noPeer': 'No paired device detected',
  'transfer.empty.noPeerHint':
    'Make sure your phone and computer are on the same network and the phone is paired with sharing enabled',
  'transfer.empty.noDownloadDir': 'Configure a download folder in Settings first',
  'transfer.empty.noDownloadDirHint':
    'Choose where received files are saved, then you can download files from your peer to this device',

  // ==================== ETA ====================
  'transfer.eta.seconds': '{count}s left',
  'transfer.eta.minutes': '{count}m {seconds}s left',
  'transfer.eta.hours': '{count}h {minutes}m left',
}

export default en
