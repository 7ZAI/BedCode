import type { MessageSchema } from './messages'

const zhCN: MessageSchema = {
  panel: {
    title: '计划任务',
    refresh: '刷新',
    loading: '加载中…',
    readOnlyHint: '只读视图 · 任务管理请使用 CLI（bedtask）',
    empty: '暂无计划任务（使用 bedtask add 创建）',
    emptyHint: '在终端中用 bedtask 管理任务，本面板自动同步',
    loadFailed: '加载失败：插件不可用或网关异常，请稍后重试',
    jobsSection: '任务列表',
    executionsSection: '最近执行',
    selectHint: '点击任务查看最近执行记录',
    neverRun: '从未执行',
    noExecutions: '暂无执行记录（bedtask run 可手动触发一次）',
    copy: '复制',
    copied: '已复制',
    copyFailed: '复制失败',
    outputPath: '输出文件',
    noOutput: '无输出文件',
    enabled: '启用',
    disabled: '已停用',
    nextAt: '下次触发',
    lastRun: '最近执行',
    exitCode: '退出码',
    triggerCron: '定时',
    triggerManual: '手动',
    statusSucceeded: '成功',
    statusFailed: '失败',
    statusTimeout: '超时',
    statusMissed: '错过',
    statusWaiting: '排队中',
    statusRunning: '执行中',
  },
}

export default zhCN
