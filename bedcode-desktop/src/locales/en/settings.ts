export default {
  settings: {
    title: 'Settings',
    // Ticket 14: the `pairing` section retired — pairing code / QR validity now live
    // in the settings section contributed by com.bedcode.terminal-session (`pairing.settings.*`).
    linkCrypto: {
      title: 'Link Encryption',
      master: 'Enable Link Encryption',
      masterDesc:
        'End-to-end encrypt traffic over LAN; off by default — pair a mobile device first',
      encryptHttp: 'Encrypt HTTP Payloads',
      encryptHttpDesc: 'Envelope encryption for REST request/response bodies (e.g. terminal input)',
      encryptWsTerminal: 'Encrypt Terminal Channel',
      encryptWsTerminalDesc: 'WS frame encryption for the terminal channel (PTY output stream)',
      encryptWsEvent: 'Encrypt Event Channel',
      encryptWsEventDesc: 'WS frame encryption for the event channel (sync broadcasts)',
      plaintextFallback: 'Allow Plaintext Fallback',
      plaintextFallbackDesc:
        'Serve un-negotiated legacy clients in plaintext; when off, non-loopback un-negotiated requests are rejected',
      fingerprint: 'Local Fingerprint',
      fingerprintDesc:
        'Fingerprint of the local identity key (first 16 hex of SHA-256); verify against the value shown on the mobile device',
      saveFailed: 'Failed to save link encryption settings',
    },
    system: {
      title: 'System Settings',
      preventSleep: 'Prevent System Sleep',
      preventSleepDesc:
        'Prevent the system from sleeping while the server is running (display sleep allowed)',
      // Ticket 14: default port moved here with the retired pairing section
      defaultPort: 'Default Port',
      defaultPortDesc: 'The port used when the server starts; takes effect after restart',
    },
    log: {
      title: 'Log Settings',
      level: 'Log Level',
      levelDesc: 'Switch the runtime log file level; takes effect immediately without restart',
      levelDebug: 'Debug',
      levelInfo: 'Info',
      levelWarn: 'Warn',
      levelError: 'Error',
      levelApplied: 'Log level switched',
      format: 'Log Format',
      formatDesc: 'Text for direct reading; JSON for field-based filtering by scripts (after restart)',
      formatText: 'Text',
      formatJson: 'JSON',
      maxFiles: 'Retained Files',
      maxFilesDesc: 'Daily-rotated log files to keep, 0 means unlimited',
      capacityMb: 'Capacity Limit (MB)',
      capacityMbDesc: 'Total log directory size cap; oldest files are removed automatically. 0 means unlimited',
      persist: 'Persist Config',
      persistDesc: 'Format / retention / capacity are saved to the config file and take effect after restart',
      openDir: 'Open Log Directory',
      save: 'Save Config',
      saved: 'Log config saved',
      saveFailed: 'Failed to save log config',
    },
    // Authorization policies & records (2026-09-27 auth policy enhancement, ticket 01):
    // settings sub-page entry "App Authorization" plus the management page copy.
    // Strategy wording mirrors spec §4.1 one-to-one.
    authorization: {
      title: 'App Authorization',
      entry: 'Authorization Records',
      entryDesc: 'Review file and network authorization per app, and manage ask policies',
      back: 'Back to Settings',
      refresh: 'Refresh',
      appCount: '{count} apps',
      empty: 'No apps to manage yet',
      emptyHint: 'Installed and enabled apps will be listed here with their authorizations',
      recordCount: '{count} records',
      // Authorization record list (ticket 02): file/directory records inside the expanded
      // row plus their per-record actions. `ops` mirrors the dialog wording
      // (read / write / read & write); `source` values map 1:1 to host record sources.
      records: {
        toggle: 'Show authorization records',
        fsTitle: 'File & directory records',
        // Ticket 05: network records (normalized origin, optionally with a path prefix)
        networkTitle: 'Network address records',
        empty: 'No file authorization records yet',
        // Per-resource empty-state key (`<resource>Empty`, rendered resource by resource since ticket 05)
        fsEmpty: 'No file authorization records yet',
        networkEmpty: 'No network authorization records yet',
        revoke: 'Revoke',
        removeDeny: 'Remove denial',
        revoked: 'Authorization revoked — future access to this directory is denied',
        networkRevoked: 'Authorization revoked — future requests to this address are denied',
        denyRemoved: 'Denial record removed; the directory is back to unauthorized',
        effect: {
          allow: 'Allowed',
          deny: 'Denied',
        },
        ops: {
          read: 'read',
          write: 'write',
          read_write: 'read & write',
        },
        source: {
          user: 'User confirmed',
          always_allow: 'Auto-allowed (no prompt)',
          legacy: 'Legacy',
          user_deny: 'User denied',
        },
      },
      // Four-section titles and empty states (ticket 07 detail view / ticket 08 settings,
      // spec §9.2): user granted / auto-allowed (unconfirmed) / first-party / hard denied
      sections: {
        userGranted: 'Granted by user',
        autoAllowed: 'Auto-allowed',
        firstParty: 'First-party built-in',
        denied: 'Hard denied',
        // Auto-allowed records must carry an "unconfirmed" marker, visually distinct from
        // user-confirmed records (spec §9.4)
        unconfirmed: 'Unconfirmed',
        empty: 'No authorization records yet',
        // Two shapes of first-party entries: home prefix / named segment in any project
        firstPartyHome: 'Home directory',
        firstPartySegment: 'Project directory',
        revokeHint: 'Revoking denies future access to this directory',
        // Empty state for "no built-in exemptions" (a fact, not missing data)
        firstPartyEmpty: 'No built-in prompt exemptions for this app',
        firstPartyRevoked: 'Prompt exemption revoked — future access is denied',
      },
      strategy: {
        always_ask: 'Always Ask',
        default: 'Default',
        always_allow: 'Always Allow',
      },
      // Strategy control (ticket 03, "Always Allow" unlocked in ticket 04): all three tiers
      // are selectable; switching to "Always Allow" is confirmed first, and the dialog must
      // spell out the semantic boundary (spec §4.3) — switching does **not** grant everything
      // at once, records accumulate as the app actually touches targets (which is why the
      // dialog still warns about the growing list).
      strategyControl: {
        title: '{resource} request policy',
        saved: 'Policy updated — it applies to the next decision',
        hint: {
          // "Always ask" skips all allow records, so previously granted targets are
          // asked about too — "unrecorded target" wording implied the opposite (ticket 06)
          always_ask: 'Ask on every access, even for targets you already granted',
          default: 'Allow when a record matches, otherwise ask',
          always_allow: 'Allow unrecorded targets without asking, and record them (unconfirmed)',
        },
        confirmTitle: 'Switch to "Always Allow"?',
        confirmBody:
          '"{name}" will stop being asked for {resource} requests: targets with no record are allowed right away and recorded as "unconfirmed". Switching does not grant everything at once — only the targets this app actually touches accumulate over time, so the list keeps growing while the app keeps accessing new places. You can revoke single entries below at any time.',
        confirmOk: 'Switch to Always Allow',
      },
      resource: {
        fs: 'Files',
        network: 'Network',
      },
    },
    ui: {
      title: 'UI Settings',
    },
    appearance: {
      theme: 'Theme',
      palette: 'Color Palette',
      paletteWarm: 'Warm Workbench',
      paletteCool: 'Cool Slate',
      paletteForest: 'Forest Green',
      paletteOcean: 'Ocean Teal',
      paletteSunset: 'Sunset Ember',
      paletteViolet: 'Starry Violet',
      paletteDesc: 'Global color style, switches instantly',
      lightMode: 'Light Mode',
      darkMode: 'Dark Mode',
      followSystem: 'Follow System',
      language: 'Language',
      fontSize: 'Font Size',
      fontSmall: 'Small',
      fontNormal: 'Normal',
      fontLarge: 'Large',
      fontXl: 'Extra Large',
      animations: 'Animations',
      animationsDesc: 'Turn off to disable all page transitions and interaction animations',
    },
    about: {
      title: 'About',
      githubRepo: 'GitHub Repository',
      checkUpdate: 'Check for Updates',
      alreadyLatest: 'Already on the latest version',
      checkingUpdate: 'Checking for updates...',
      checkFailed: 'Failed to check for updates, please try again',
      newVersionAvailable: 'New version available',
      downloadingUpdate: 'Downloading update...',
      downloadComplete: 'Download complete, installing...',
      installingUpdate: 'Installing update...',
      downloadUpdate: 'Update Now',
    },
  },
}
