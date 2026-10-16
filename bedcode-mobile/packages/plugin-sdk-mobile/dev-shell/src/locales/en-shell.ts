/**
 * Dev Shell 宿主壳文案 —— English
 * -----------------------------------------------------------------------------
 * 与宿主 `bedcode-mobile/src/locales/en/shell.ts` 同键同义（措辞沿用宿主，
 * 避免预览与真机两套说法）。宿主新增 key 时按 `dev-shell/__tests__/visualParity.test.ts`
 * 之外的这份副本同步（本文件是 dev-shell 自有命名空间，不做逐字比对）。
 *
 * dev-shell 独有的 key 保留在既有段内并注明理由（主题档位 / 调试数据等浏览器环境专有面）。
 *
 * 新增 / 修改 key 必须同步 en/shell.ts（i18n 双语纪律）。
 */
export const enShell = {
  shell: {
    common: {
      loading: 'Loading…',
      retry: 'Retry',
      na: '—',
      running: 'Running',
      stopped: 'Stopped',
      disabled: 'Disabled',
      approval: 'Pending approval',
      error: 'Failed to start',
      cancel: 'Cancel',
      active: 'Current',
      back: 'Back'
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
      manageApps: 'Manage apps',
      appAria: '{name}, {state}',
      emptyApps: 'No WASM app installed',
      emptyAppsHint: 'Install or enable an app in "App manager" and its entry shows up here',
      sysTitle: 'System',
      sysApps: 'App manager',
      sysPermissions: 'Permissions',
      sysSettings: 'Settings'
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
      uninstallFailed: 'Uninstall failed: {error}'
    },
    detail: {
      title: 'App details',
      back: 'Back',
      official: 'Official',
      approvalDesc: 'This app requests {count} permissions. Approve it before it can start — approving also pins the current file hashes.',
      approveStart: 'Approve and start',
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
      version: 'v{version}'
    },
    run: {
      menu: 'App menu',
      recents: 'Recents',
      home: 'Back home',
      noSurface: 'No interface available',
      noSurfaceHint: 'This app does not provide a run surface yet',
      notFound: 'App not found or uninstalled'
    },
    switcher: {
      title: 'Recents',
      back: 'Back',
      closeAll: 'Close all',
      closeCard: 'Close the card for {name}',
      dormant: 'Not running',
      enterApp: 'Open {name}',
      launchApp: 'Start {name}',
      prev: 'Previous app',
      next: 'Next app',
      position: 'Card {index} of {total}',
      goHome: 'Back to home',
      empty: 'Nothing to switch to',
      emptyHint: 'Open an app from the home screen and it will show up here'
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
        host: 'Connection'
      },
      connected: {
        title: 'Connected to desktop',
        body: 'Connection established and authenticated'
      },
      reconnected: {
        title: 'Connection restored',
        body: 'Automatic reconnect succeeded'
      },
      disconnected: {
        title: 'Connection lost',
        body: 'The link to the desktop was interrupted'
      },
      reauthRejected: {
        title: 'Re-pairing required',
        body: 'Credentials were rejected; pair again to reconnect'
      },
      app: {
        started: {
          title: '{name} started',
          body: 'The app is running in the background'
        },
        stopped: {
          title: '{name} stopped',
          body: 'The app stopped; its data is kept'
        },
        approved: {
          title: '{name} approved',
          body: 'Permissions approved and content hash pinned'
        },
        installed: {
          title: '{name} installed',
          body: 'Approve it to start'
        },
        uninstalled: {
          title: '{name} uninstalled',
          body: 'The app and its data were deleted'
        },
        failed: {
          title: '{name} failed to start',
          body: 'Check the app details and retry'
        }
      }
    },
    settings: {
      title: 'Settings',
      platform: 'Platform',
      appearance: 'Appearance',
      appearanceHint: 'Theme and palette',
      permissions: 'Permission overview',
      permissionsHint: 'Group apps by permission',
      about: 'About WasmApp',
      appSettings: 'App settings',
      runtime: 'WasmApp Mobile {version}',
      devShellHint: 'Browser debug environment · WASM backend does not run here',
      language: 'Language',
      debugObjects: 'Debug objects',
      debugObjectsHint: '{count} objects · {running} running',
      devData: 'Debug data',
      clearDevData: 'Clear debug data',
      clearDevDataHint: 'Clears dev-shell localStorage keys (plugin storage, recents, language) and reloads. This cannot be undone.',
      clearDevDataDone: 'Cleared {count} debug entries, reloading…'
    },
    permissions: {
      pending: 'Pending approval',
      pendingNote: 'Waiting for your decision',
      pendingBody: '{count} permissions awaiting approval',
      approveStart: 'Approve and start',
      approveFailed: 'Approval failed; check the app details',
      viewSwitch: 'Permission overview view',
      viewApps: 'By app',
      viewGroups: 'By permission',
      appSection: 'App permissions',
      appSummary: '{granted} granted',
      appSummaryLocked: '{granted} granted · {locked} default',
      appNone: 'No permissions',
      granted: 'Granted',
      locked: 'Default',
      ungranted: 'Not granted'
    },
    capsule: {
      title: 'App',
      permissions: 'Permission settings',
      about: 'About this app',
      disable: 'Disable this app'
    },
    permissionPrompt: {
      title: '{name} requests permissions',
      purpose: 'Purpose: {reason}',
      purposeUnknown: 'Purpose: not declared by the app',
      deny: 'Deny',
      allow: 'Allow',
      fineprint: 'Denying disables the related feature; you can change it anytime in "Apps → {name} → Permissions".'
    },
    permission: {
      group: {
        terminal: 'Terminal & sessions',
        data: 'Files & network',
        device: 'Device & link',
        interface: 'Interface & messaging',
        other: 'Other'
      },
      perm: {
        storage: {
          title: 'Local Storage'
        },
        terminalOutput: {
          title: 'Terminal Output Stream'
        },
        sessionRead: {
          title: 'Session Read'
        },
        sessionWrite: {
          title: 'Session Write'
        },
        uiToolbox: {
          title: 'Toolbox UI'
        },
        uiNavtab: {
          title: 'Nav Tab'
        },
        uiSettings: {
          title: 'Settings Section'
        },
        uiInput: {
          title: 'Input Assistant'
        },
        networkHttp: {
          title: 'Network Requests'
        },
        fsRead: {
          title: 'File Read'
        },
        fsWrite: {
          title: 'File Write'
        },
        fsPick: {
          title: 'File Picker'
        },
        fileservice: {
          title: 'File Service'
        },
        transfer: {
          title: 'File Transfer'
        },
        systemOpen: {
          title: 'System Open'
        },
        bus: {
          title: 'Message Bus'
        },
        broadcast: {
          title: 'Broadcast'
        },
        mdns: {
          title: 'LAN Discovery'
        },
        peer: {
          title: 'Peer Connection'
        },
        wsClient: {
          title: 'WebSocket Client'
        },
        auth: {
          title: 'Authentication'
        },
        notify: {
          title: 'Notifications'
        },
        uiRoute: {
          title: 'Page Routing'
        },
        uiDialog: {
          title: 'Dialogs'
        },
        uiBack: {
          title: 'Back Key'
        }
      }
    },
    mobile: {
      plugin: {
        noPermissions: 'No permissions requested'
      }
    }
  }
};


/** dev-shell 专有文案（工具条 / 日志面板 / 内置应用 / 插件路由页） */
export const enDevShell = {
  'devshell.plugin.loadFailed': 'Plugin component not registered',
  'devshell.plugin.routeLoading': 'Loading',
  'devshell.terminal.queueMock': 'Queue(mock):',
  'devshell.terminal.queueEmpty': 'Empty',
  'devshell.terminal.queueClear': 'Clear',
}
