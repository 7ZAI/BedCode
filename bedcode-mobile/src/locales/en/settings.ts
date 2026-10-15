export default {
  settings: {
    title: 'Settings',
    // Ticket 2026-10-10: `general` / `dangerZone` retired with the `actions` group
    // (no UI callers left). 2026-10-14: the device-level wipe copy moved into this
    // file's `about` group (entry moved from platform settings to "About WasmApp").
    groups: {
      connection: 'Connection',
      security: 'Security',
      system: 'System',
    },
    network: {
      title: 'Network Settings',
      websocketPort: 'WebSocket Port',
    },
    session: {
      title: 'Session Defaults',
      defaultEnvironment: 'Default Environment',
      defaultCommand: 'Default Start Command',
    },
    qr: {
      title: 'QR Code Settings',
      validity: 'QR Code Validity',
      validityDesc: 'Set the validity period for QR codes (60-3600 seconds)',
    },
    ui: {
      title: 'UI Settings',
      terminalFontSize: 'Terminal Font Size',
    },
    appearance: {
      title: 'Appearance',
      subtitle: 'Theme, language, font and home background',
      theme: 'Theme',
      lightMode: 'Light Mode',
      darkMode: 'Dark Mode',
      followSystem: 'Follow System',
      palette: 'Accent Palette',
      paletteDefault: 'Default',
      paletteForest: 'Forest',
      paletteOcean: 'Ocean',
      paletteSunset: 'Sunset',
      paletteViolet: 'Violet',
      language: 'Language',
      languageChinese: '中文',
      languageEnglish: 'English',
      fontSize: 'Font Size',
      fontNormal: 'Normal',
      fontLarge: 'Large',
      fontXLarge: 'Extra Large',
      generalSection: 'General',
      displaySection: 'Display',
      // Home wallpaper (moved from the shell settings page on 2026-10-15: background
      // belongs to appearance config, same screen as theme/palette; gradient & dim
      // preview logic live in styles/wallpaper.css, shared with the home screen)
      backgroundSection: 'Home background',
      backgroundNote: 'Home screen only',
      wallNone: 'None',
      wallAurora: 'Aurora',
      wallDune: 'Dune',
      wallReef: 'Reef',
      wallCustom: 'Custom',
      wallPick: 'Choose from gallery',
      wallReplace: 'Replace image',
      wallRemove: 'Remove image',
      wallDim: 'Overlay strength',
    },
    connection: {
      title: 'Connection Settings',
      subtitle: 'Link encryption and transport security',
      // Ticket 2026-10-10 C4: autoReconnect / keepAlive / defaultPort are business
      // settings, now owned by the bedcode app's settings page.
      businessElsewhere:
        'Auto reconnect, keep-alive and default port now live in the BedCode app settings.',
      linkCryptoSection: 'Link Encryption',
      linkCryptoMaster: 'Enable Link Encryption',
      linkEncryptHttp: 'Encrypt HTTP Payloads',
      linkStrictMode: 'Strict Mode (reject plaintext downgrade)',
      linkPeerFingerprint: 'Peer Fingerprint',
      linkNotPaired: 'Not paired',
      linkNeedPairing: 'Pair with the desktop first, then enable link encryption',
      linkCryptoHint:
        'Verify the desktop fingerprint shown in its settings page before enabling; off by default.',
    },
    authentication: {
      title: 'Authentication Settings',
      subtitle: 'Biometric credential',
      // Ticket 2026-10-10 C4: preferred auth method is a business setting, now owned
      // by the bedcode app.
      biometricSection: 'Biometric Credential',
      biometricDesc: 'The key is stored in system secure hardware and can only sign after fingerprint/face authentication. The private key never leaves the device.',
      bind: 'Bind Biometric Auth',
      unbind: 'Unbind Biometric Auth',
      bindHint: 'Connect via pairing code or QR code before binding',
      unbound: 'Not bound',
      bound: 'Bound',
      statusError: 'Failed to check biometric support, please retry',
      unsupported: 'Biometric authentication is not supported on this device',
      unsupportedNoHardware: 'This device has no biometric hardware',
      unsupportedNotEnrolled: 'No biometric enrolled. Please enroll fingerprint/face in system settings first',
      unsupportedUnavailable: 'Biometric hardware is temporarily unavailable (device locked or needs unlocking once after reboot)',
      bindSuccess: 'Biometric credential bound',
      bindFailed: 'Bind failed, please make sure you are connected and retry',
      bindLocked: 'Too many failed attempts, biometric paused. Please retry later',
      bindCancelled: 'Bind cancelled',
      bindInvalidated: 'Biometrics changed, key invalidated. Please bind again',
      unbindSuccess: 'Biometric credential unbound',
      unbindFailed: 'Unbind failed, please retry',
      notConnected: 'Not connected, cannot bind',
    },
    // Ticket 2026-10-10 C4: the whole `notification` group is retired — every entry is a
    // business setting, now owned by the bedcode app; the
    // `mobile-settings-notifications` route retires with it.
    egress: {
      title: 'Network Access',
      subtitle: 'View and revoke external network grants',
      grantsSection: 'Granted addresses',
      loading: 'Loading…',
      empty: 'No grants yet',
      allPaths: 'All paths',
      revokeAll: 'Revoke All',
      revokeHint: 'After revocation, previously approved external addresses will need to be re-authorized.',
      revokeConfirmTitle: 'Revoke all grants?',
      revokeConfirmMessage: 'This clears every remembered external address grant, including "don\'t ask again" entries.',
      strategySection: 'Access Strategy',
      strategyHint: 'Controls whether you are asked when a plugin requests access to an unapproved address.',
      strategyEmpty: 'No plugins to configure',
      alwaysAsk: 'Always Ask',
      alwaysAskDesc: 'Ask on every access (skips remembered grants)',
      defaultStrategy: 'Default',
      defaultStrategyDesc: 'Allow remembered addresses, ask for the rest',
      alwaysAllow: 'Always Allow',
      alwaysAllowDesc: 'Allow without asking (grants marked "unconfirmed")',
      recordsSection: 'Grant Records',
      sourceUser: 'Confirmed',
      sourceAlwaysAllow: 'Unconfirmed',
      sourceUserDeny: 'Denied',
      revokeOne: 'Revoke',
      revokeOneConfirmTitle: 'Revoke this grant?',
      revokeOneConfirmMessage: 'After revocation, this address needs to be re-approved before access.',
    },
    about: {
      title: 'About',
      subtitle: 'Version info and updates',
      infoSection: 'Info',
      updateSection: 'Updates',
      githubRepo: 'GitHub Repository',
      checkUpdate: 'Check for Updates',
      alreadyLatest: 'Already on the latest version',
      checkingUpdate: 'Checking for updates...',
      newVersionAvailable: 'New version available',
      updateCheckFailed: 'Update check failed',
      downloadingUpdate: 'Downloading update...',
      downloadComplete: 'Download complete, installing...',
      installingUpdate: 'Installing update...',
      downloadUpdate: 'Update Now',
      currentVersion: 'Current Version',
      // Danger zone (2026-10-14 revision): moved here from the top-level platform
      // settings page. Its targets include device entry credentials and host
      // connection state, so only the host may run it. See useClearAllData.ts.
      dangerZone: 'Danger Zone',
      clearAllData: 'Clear All Data',
      clearAllDataHint:
        'Disconnects and wipes preset tasks, connection history, paired devices, session configs and local cache. This cannot be undone.',
      clearAllDataFailed: 'Cleanup did not finish; some data may remain. Retry or restart the app.',
    },
    // Ticket 2026-10-10: the `actions` group retired; both actions were moved by
    // ownership — "reset settings" to the bedcode app, "clear all data"
    // to this file's about group (device-level wipe, host-owned).
    browser: {
      confirmOpen: 'Are you sure you want to open this link in a browser?',
    },
    shortcuts: {
      title: 'Shortcut Settings',
      description: 'Configure shortcut keys for the terminal panel',
    },
  },
}
