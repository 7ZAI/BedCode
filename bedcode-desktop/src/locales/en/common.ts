export default {
  common: {
    button: {
      cancel: 'Cancel',
      stop: 'Stop',
    },
    status: {
      error: 'Error',
      unknown: 'Unknown',
      running: 'Running',
      stopped: 'Stopped',
      asking: 'Waiting for Input',
      starting: 'Starting',
    },
    // Ticket 01 (group F): common.errorCode.ipcTimeout removed with InvokeTimeoutError
    // ({cmd} leaked the command name + had no callers; timeouts now surface via
    // errors.host.invoke.timeout)
    misc: {
      terminalTitle: 'Terminal - {name}',
    },
  },
  // ==================== Error Envelope (ADR 0030) ====================
  // Top-level namespace: errors.<code> ↔ envelope code, zero mapping layer;
  // renaming a code is a breaking change. Registry v0 base codes (ticket 01):
  // fallback / timeout / frontend fallback; host.plugin.* runtime domain added by ticket 03.
  // UI rule: toasts/pages show friendly copy (template + params interpolation) only;
  // never render the code, request_id, or technical detail.
  errors: {
    retry: 'Retry',
    host: {
      internal: 'Operation failed, please try again',
      invoke: {
        timeout: 'Operation timed out, please retry',
      },
      // Ticket 02 mechanism codes (ADR 0030 registry v0): plugin-management failures
      // + ticket 03 runtime domain (event-channel envelopes, ADR 0030 decisions 7 / 11) —
      // one user-facing story per failure kind; interpolation params carry display names only
      plugin: {
        'not-activated': 'This app is not enabled, this action is unavailable',
        'not-found': 'App not found or removed',
        trap: 'App "{name}" misbehaved, auto-recovery attempted',
        'recovery-failed': 'App "{name}" failed and could not recover; check the app center',
        'self-check-failed': 'App "{plugin}" self-check failed, check its configuration',
        degraded: 'App "{name}" is running degraded, some features unavailable',
      },
    },
    frontend: {
      internal: 'Operation failed, please try again',
    },
  },
}
