/**
 * 计划任务插件翻译 schema（key 即契约：zh-CN / en 必须同构）
 */
export interface MessageSchema {
  panel: {
    title: string
    refresh: string
    loading: string
    readOnlyHint: string
    empty: string
    emptyHint: string
    loadFailed: string
    jobsSection: string
    executionsSection: string
    selectHint: string
    neverRun: string
    noExecutions: string
    copy: string
    copied: string
    copyFailed: string
    outputPath: string
    noOutput: string
    enabled: string
    disabled: string
    nextAt: string
    lastRun: string
    exitCode: string
    triggerCron: string
    triggerManual: string
    statusSucceeded: string
    statusFailed: string
    statusTimeout: string
    statusMissed: string
    statusWaiting: string
    statusRunning: string
  }
}
