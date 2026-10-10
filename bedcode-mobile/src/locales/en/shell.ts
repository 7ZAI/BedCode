/**
 * Host Shell copy — English
 *
 * Keys mirror zh-CN/shell.ts one-to-one (i18n bilingual discipline).
 */
export default {
  shell: {
    /** Entry row in the legacy settings page (kept in shell.ts to avoid clashing with parallel edits) */
    entry: {
      title: 'WASM app platform (host shell)',
      subtitle: 'New platform shell · coexists with the current UI',
    },
    nav: {
      home: 'Home',
      apps: 'Apps',
      settings: 'Me',
    },
    common: {
      loading: 'Loading…',
      retry: 'Retry',
      na: '—',
      running: 'Running',
      stopped: 'Stopped',
      disabled: 'Disabled',
      error: 'Failed to start',
      cancel: 'Cancel',
    },
    home: {
      localDevice: 'This device',
      appCount: '{count} apps',
      slotsTitle: 'Quick cards',
      slotsNote: 'Provided by apps',
      appsTitle: 'My WASM apps',
      manage: 'Manage',
      recentTitle: 'Recently used',
      more: 'More',
      providedBy: 'By {name}',
      emptyApps: 'No WASM app installed',
      emptyAppsHint: 'Install or enable an app in "Apps" and its entry shows up here',
      openApp: 'Open app',
    },
    apps: {
      title: 'WASM apps',
      subtitle: '{installed} installed · {running} running',
      statInstalled: 'Installed',
      statRunning: 'Running',
      statSize: 'Total size',
      installedTitle: 'Installed apps',
      installedNote: 'Listed from loaded apps',
      addTitle: 'Add app',
      discover: 'Discover more apps',
      discoverHint: 'Local repo / pushed from desktop',
      installLocal: 'Install local package (.wasm)',
      installLocalHint: 'Approval + hash pinning required',
      empty: 'No apps',
      permissionCount: '{count} permissions',
      installSuccess: 'Installed — enable it from the list',
      installFailed: 'Install failed: {error}',
      uninstallConfirm: 'Uninstall "{name}" and delete its data? This cannot be undone.',
      uninstallSuccess: 'Uninstalled {name}',
      uninstallFailed: 'Uninstall failed: {error}',
    },
    detail: {
      title: 'App details',
      back: 'Back',
      official: 'Official',
      connection: 'Connection',
      connectionNote: 'Owned by this app',
      permissions: 'Permissions',
      permissionsNote: '{granted} granted · {locked} by default',
      permissionLocked: 'Granted by default · cannot be turned off',
      permissionUnsupported: 'Per-permission toggles are not supported by the current backend',
      permissionToggleFailed: 'Permission change not applied: {reason}',
      storage: 'Storage',
      appData: 'App data',
      cache: 'Cache',
      clear: 'Clear',
      grants: 'Grant log',
      grantsNote: 'Last 7 days',
      grantsEmpty: 'No grant records',
      demoPrompt: 'Demo: runtime permission prompt',
      disable: 'Disable app',
      uninstall: 'Uninstall and delete data',
      version: 'v{version}',
    },
    run: {
      capsule: 'App menu',
      exitApp: 'Exit app',
      noSurface: 'This app provides no surface yet',
      noSurfaceHint:
        'An app must register its surface component after activation — the platform cannot guess what to render',
      /** Reserved slot: before a real wasm-app lands, the surface stays a placeholder */
      reservedTitle: 'Surface reserved',
      reservedHint:
        'This is the mount point for a future wasm-app. Once the app registers its surface, its UI renders here — the platform only owns mounting and lifecycle, never in-app pages.',
      reservedStatus: '{state} · {count} permissions · {id}',
      notFound: 'App not found or uninstalled',
      switcherHint: 'Swipe up and hold to open the switcher',
    },
    switcher: {
      title: 'Running apps',
      backHome: 'Back home',
      empty: 'No running apps',
      foot: 'Tap a card to enter · swipe a card up to stop it',
      stop: 'Stop',
    },
    settings: {
      title: 'Me',
      platform: 'Platform',
      appearance: 'Appearance',
      appearanceHint: 'Theme and palette',
      permissions: 'Permission overview',
      permissionsHint: 'Group apps by permission',
      // Connection / authentication / egress: shell-side entries (reuse the legacy
      // settings pages until the old host is retired — no functional regression)
      connection: 'Connection',
      connectionHint: 'Port and auto-reconnect',
      authentication: 'Authentication',
      authenticationHint: 'Pairing and credentials',
      egress: 'Egress policy',
      egressHint: 'Security egress tiers',
      about: 'About WasmApp',
      // Ticket 2026-10-10: the notification entry retired with its business settings
      // moving to terminal-session. "Clear all data" is a device-lifecycle action (its
      // targets include entry credentials and host connection state), so it stays in the
      // host per the credential-zero-transit rule. See src/composables/useClearAllData.ts.
      dangerZone: 'Danger Zone',
      clearAllData: 'Clear All Data',
      clearAllDataHint:
        'Disconnects and wipes preset tasks, connection history, paired devices, session configs and local cache. This cannot be undone.',
      clearAllDataFailed: 'Cleanup did not finish; some data may remain. Retry or restart the app.',
      appSettings: 'App settings',
      runtime: 'WasmApp Mobile {version}',
    },
    capsule: {
      title: 'App',
      subtitle: 'Platform controls stacked above the app, like a mini-program capsule',
      permissions: 'Permission settings',
      about: 'About this app',
      disable: 'Disable this app',
      note: 'Stopping an app frees its memory and background tasks; data is kept',
    },
    permissionPrompt: {
      title: '{name} requests permissions',
      subtitle: 'Unified platform prompt · denial disables the feature',
      purpose: 'Purpose: {reason}',
      purposeUnknown: 'Purpose: not declared by the app',
      deny: 'Deny',
      allow: 'Allow',
      fineprint:
        'Denying disables the related feature; you can change it anytime in "Apps → {name} → Permissions". Authorization is enforced on the Rust side — this prompt is UX only.',
    },
    permission: {
      group: {
        terminal: 'Terminal & sessions',
        data: 'Files & network',
        interface: 'Interface & messaging',
        other: 'Other',
      },
    },
  },
}
