/**
 * Host Shell copy — English
 *
 * Keys mirror zh-CN/shell.ts one-to-one (i18n bilingual discipline).
 * Grouping: home=home screen, switcher=recents, notify=notification shade,
 * run=app run screen, settings=platform settings, permissions=permission manager.
 *
 * Copy discipline (2026-10-14 revision): state only facts and consequences a user can
 * perceive — no platform implementation notes or prototype-stage explanations.
 */
export default {
  shell: {
    common: {
      loading: 'Loading…',
      retry: 'Retry',
      na: '—',
      running: 'Running',
      stopped: 'Stopped',
      disabled: 'Disabled',
      /** Pending approval: whole-manifest permission grant + content hash pinning */
      approval: 'Pending approval',
      error: 'Failed to start',
      cancel: 'Cancel',
    },
    home: {
      localDevice: 'This device',
      appCount: '{count} apps',
      runningTitle: 'Running',
      switchTo: 'Switch to {name}',
      stopInstance: 'Stop',
      slotsTitle: 'Quick cards',
      appsTitle: 'My WASM apps',
      appsNote: '{count} installed',
      /** Icon button in the app grid section header (opens app manager) */
      manageApps: 'Manage apps',
      appAria: '{name}, {state}',
      emptyApps: 'No WASM app installed',
      emptyAppsHint: 'Install or enable an app in "App manager" and its entry shows up here',
      sysTitle: 'System',
      sysApps: 'App manager',
      sysPermissions: 'Permissions',
      sysSettings: 'Settings',
    },
    apps: {
      title: 'WASM apps',
      subtitle: '{installed} installed · {running} running',
      statInstalled: 'Installed',
      statRunning: 'Running',
      statSize: 'Total size',
      installedTitle: 'Installed apps',
      addTitle: 'Add app',
      installLocal: 'Install local package (.wasm)',
      installLocalHint: 'Approval required',
      empty: 'No apps',
      permissionCount: '{count} permissions',
      installSuccess: 'Installed — approve and start it from the list',
      installFailed: 'Install failed: {error}',
      uninstallConfirm: 'Uninstall "{name}" and delete its data? This cannot be undone.',
      uninstallSuccess: 'Uninstalled {name}',
      uninstallFailed: 'Uninstall failed: {error}',
    },
    detail: {
      title: 'App details',
      back: 'Back',
      official: 'Official',
      /** Approval is whole-manifest (all declared permissions + content hash pinning) */
      approvalDesc:
        'This app requests {count} permissions. Approve it before it can start — approving also pins the current file hashes.',
      approveStart: 'Approve and start',
      /** Runtime state (each app owns its connection; the platform projects state only) */
      state: 'Runtime state',
      permissions: 'Permissions',
      permissionsNote: '{granted} granted · {locked} by default',
      permissionLocked: 'Granted by default · cannot be turned off',
      permissionUnsupported: 'Per-permission toggles are not supported by the current backend',
      permissionToggleFailed: 'Permission change not applied: {reason}',
      storage: 'Storage',
      appData: 'App data',
      grants: 'Grant log',
      grantsNote: 'Last 7 days',
      grantsEmpty: 'No grant records',
      disable: 'Disable app',
      uninstall: 'Uninstall and delete data',
      version: 'v{version}',
    },
    run: {
      menu: 'App menu',
      recents: 'Recents',
      home: 'Back home',
      noSurface: 'No interface available',
      noSurfaceHint: 'This app does not provide a run surface yet',
      notFound: 'App not found or uninstalled',
    },
    switcher: {
      title: 'Recents',
      back: 'Back',
      closeAll: 'Close all',
      closeCard: 'Close the card for {name}',
      dormant: 'Not running',
      /** Accessible name of the whole-card tap target (tap card = open/start) */
      enterApp: 'Open {name}',
      launchApp: 'Start {name}',
      /** Deck navigation — keyboard/click alternative to dragging (WCAG 2.2) */
      prev: 'Previous app',
      next: 'Next app',
      /** Card position for screen readers (the dots themselves are decorative) */
      position: 'Card {index} of {total}',
      /** Empty-state primary action: recents is a dead end without a way out */
      goHome: 'Back to home',
      empty: 'Nothing to switch to',
      emptyHint: 'Open an app from the home screen and it will show up here',
    },
    notify: {
      title: 'Notifications',
      count: '{count} items',
      open: 'Open notifications ({count} items)',
      close: 'Collapse notifications',
      pending: 'Pending',
      pendingNote: 'Waiting for your decision',
      pendingBody: '{count} permissions awaiting approval',
      pendingAction: 'Review',
      list: 'Notifications',
      listNote: '{count} items',
      empty: 'No notifications',
      emptyHint: 'App launches, connection state and pending approvals show up here',
      clearAll: 'Clear all',
      dismissOne: 'Dismiss notification: {title}',
      openByTap: 'Tap to open {title}',
      justNow: 'Just now',
      minutesAgo: '{count} min ago',
      hoursAgo: '{count} h ago',
      daysAgo: '{count} d ago',
      source: {
        shell: 'Platform',
        host: 'Connection',
      },
      connected: {
        title: 'Connected to desktop',
        body: 'Connection established and authenticated',
      },
      reconnected: {
        title: 'Connection restored',
        body: 'Automatic reconnect succeeded',
      },
      disconnected: {
        title: 'Connection lost',
        body: 'The link to the desktop was interrupted',
      },
      reauthRejected: {
        title: 'Re-pairing required',
        body: 'Credentials were rejected; pair again to reconnect',
      },
      /** App lifecycle notifications (results of the shell's own actions) */
      app: {
        started: {
          title: '{name} started',
          body: 'The app is running in the background',
        },
        stopped: {
          title: '{name} stopped',
          body: 'The app stopped; its data is kept',
        },
        approved: {
          title: '{name} approved',
          body: 'Permissions approved and content hash pinned',
        },
        installed: {
          title: '{name} installed',
          body: 'Approve it to start',
        },
        uninstalled: {
          title: '{name} uninstalled',
          body: 'The app and its data were deleted',
        },
        failed: {
          title: '{name} failed to start',
          body: 'Check the app details and retry',
        },
      },
    },
    settings: {
      title: 'Settings',
      platform: 'Platform',
      appearance: 'Appearance',
      appearanceHint: 'Theme and palette',
      permissions: 'Permission overview',
      permissionsHint: 'Group apps by permission',
      // 2026-10-14: Connection / authentication entries were removed from platform
      // settings — they are not host-owned (connections and credentials belong to each
      // app). 2026-10-15: the egress policy entry was removed too (user request; legacy
      // page and route stay until retired, see revision-2.md R11).
      about: 'About WasmApp',
      appSettings: 'App settings',
      runtime: 'WasmApp Mobile {version}',
    },
    permissions: {
      pending: 'Pending approval',
      pendingNote: 'Waiting for your decision',
      pendingBody: '{count} permissions awaiting approval',
      approveStart: 'Approve and start',
      approveFailed: 'Approval failed; check the app details',
      // Dual views: by app (default, one app per row — no cross-group repeats) / by permission (reverse lookup)
      viewSwitch: 'Permission overview view',
      viewApps: 'By app',
      viewGroups: 'By permission',
      appSection: 'App permissions',
      appSummary: '{granted} granted',
      appSummaryLocked: '{granted} granted · {locked} default',
      appNone: 'No permissions',
      granted: 'Granted',
      locked: 'Default',
      ungranted: 'Not granted',
    },
    capsule: {
      title: 'App',
      permissions: 'Permission settings',
      about: 'About this app',
      disable: 'Disable this app',
    },
    permissionPrompt: {
      title: '{name} requests permissions',
      purpose: 'Purpose: {reason}',
      purposeUnknown: 'Purpose: not declared by the app',
      deny: 'Deny',
      allow: 'Allow',
      fineprint:
        'Denying disables the related feature; you can change it anytime in "Apps → {name} → Permissions".',
    },
    permission: {
      group: {
        terminal: 'Terminal & sessions',
        data: 'Files & network',
        device: 'Device & link',
        interface: 'Interface & messaging',
        other: 'Other',
      },
    },
  },
}
