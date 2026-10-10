export default {
  settings: {
    title: 'Settings',
    // 票 2026-10-10：`general` / `dangerZone` 随 `actions` 组一并退役（零 UI 调用；
    // 危险区标题改用 `shell.settings.dangerZone`）
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
      subtitle: 'Theme, language, font and terminal count',
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
    },
    connection: {
      title: 'Connection Settings',
      subtitle: 'Link encryption and transport security',
      // Ticket 2026-10-10 C4: autoReconnect / keepAlive / defaultPort are business
      // settings, now owned by the terminal-session app's settings page.
      businessElsewhere:
        'Auto reconnect, keep-alive and default port now live in the Terminal Session app settings.',
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
      // by the terminal-session app.
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
    // business setting, now owned by the terminal-session app; the
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
    },
    // 票 2026-10-10：`actions` 组整组退役。两个 action 已按归属分流：
    // 「重置设置」→ wasm-apps/terminal-session/src/settings/i18n.ts（重置业务设置项）；
    // 「清除所有数据」→ src/locales/*/shell.ts 的 shell.settings.clearAllData*（设备级擦除，
    // 留宿主危险区）。留在宿主会造成同一串文案两份真源，改一处漏一处。
    browser: {
      confirmOpen: 'Are you sure you want to open this link in a browser?',
    },
    shortcuts: {
      title: 'Shortcut Settings',
      description: 'Configure shortcut keys for the terminal panel',
    },
  },
}
