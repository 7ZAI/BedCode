import type { MessageSchema } from './messages'

const en: MessageSchema = {
  panel: {
    title: 'Scheduled Tasks',
    refresh: 'Refresh',
    loading: 'Loading…',
    readOnlyHint: 'Read-only · manage tasks via CLI (bedtask)',
    empty: 'No scheduled tasks (create one with bedtask add)',
    emptyHint: 'Manage tasks from the terminal with bedtask; this panel syncs automatically',
    loadFailed: 'Failed to load: plugin unavailable or gateway error, please retry later',
    jobsSection: 'Tasks',
    executionsSection: 'Recent Executions',
    selectHint: 'Select a task to view its recent executions',
    neverRun: 'Never ran',
    noExecutions: 'No executions yet (bedtask run triggers one manually)',
    copy: 'Copy',
    copied: 'Copied',
    copyFailed: 'Copy failed',
    outputPath: 'Output file',
    noOutput: 'No output file',
    enabled: 'Enabled',
    disabled: 'Disabled',
    nextAt: 'Next run',
    lastRun: 'Last run',
    exitCode: 'Exit code',
    triggerCron: 'Cron',
    triggerManual: 'Manual',
    statusSucceeded: 'Succeeded',
    statusFailed: 'Failed',
    statusTimeout: 'Timeout',
    statusMissed: 'Missed',
    statusWaiting: 'Waiting',
    statusRunning: 'Running',
  },
}

export default en
