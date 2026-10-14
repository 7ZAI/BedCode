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
      sysSettings: 'Platform settings',
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
      enter: 'Open',
      launch: 'Start',
      /** Deck navigation — keyboard/click alternative to dragging (WCAG 2.2) */
      prev: 'Previous app',
      next: 'Next app',
      empty: 'No running apps',
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
      openApp: 'Open',
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
      title: 'Platform settings',
      background: 'Background',
      backgroundNote: 'Home screen only',
      wallNone: 'None',
      wallAurora: 'Aurora',
      wallDune: 'Dune',
      wallReef: 'Reef',
      wallCustom: 'Custom',
      wallPick: 'Choose from gallery',
      wallReplace: 'Replace image',
      wallRemove: 'Remove image',
      wallPickFailed: 'Could not read the selected image. Try another one.',
      wallDim: 'Dim strength',
      platform: 'Platform',
      appearance: 'Appearance',
      appearanceHint: 'Theme and palette',
      permissions: 'Permission overview',
      permissionsHint: 'Group apps by permission',
      // 2026-10-14: Connection / authentication entries were removed from platform
      // settings — they are not host-owned (connections and credentials belong to each
      // app). The legacy pages stay until their last reference retires; see revision-2.md R11.
      egress: 'Egress policy',
      egressHint: 'Security egress tiers',
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
