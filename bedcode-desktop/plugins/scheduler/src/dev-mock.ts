/**
 * 计划任务开发环境 mock（仅 dev-shell 生效）
 *
 * 浏览器 vite 无 Rust/WASM 后端，`_http_endpoint` 命令不可达。本模块按
 * http_response 信封形状注册前端命令 handler，让面板可在 dev-shell 预览
 * 完整 UI（空态 + 实态）。
 *
 * - 默认空数据（评审空态）
 * - URL `?mock=1` 时填充多状态示例任务（评审实态：启用/停用、各状态徽标、
 *   有/无输出文件、退出码等）
 */
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

/** 端点响应信封（与 useSchedulerApi 中 EndpointEnvelope 同形） */
interface Envelope<T> {
  status: number
  body: { code: number; message: string; data?: T }
}

function ok<T>(data: T): Envelope<T> {
  return { status: 200, body: { code: 0, message: 'ok', data } }
}

/** 示例任务（覆盖：启用/停用、cron/一次性、各最近状态） */
const sampleJobs = [
  {
    id: 'job-nightly-backup',
    name: '每日备份数据库',
    schedule: '0 2 * * *',
    exec_type: 'script',
    exec_value: 'backup.sh',
    cwd: '/home/user/scripts',
    env: null,
    timeout_sec: 3600,
    enabled: 1,
    once: 0,
    next_at: '2026-08-13 02:00:00',
    created_at: '2026-08-01 09:00:00',
    updated_at: '2026-08-12 08:30:00',
    last_status: 'succeeded',
    last_finished_at: '2026-08-12 02:03:21',
  },
  {
    id: 'job-cleanup-tmp',
    name: '清理临时文件',
    schedule: '0 4 * * 0',
    exec_type: 'inline',
    exec_value: 'rm -rf /tmp/bedcode-*',
    cwd: null,
    env: null,
    timeout_sec: 300,
    enabled: 1,
    once: 0,
    next_at: '2026-08-16 04:00:00',
    created_at: '2026-08-02 10:00:00',
    updated_at: '2026-08-11 22:00:00',
    last_status: 'failed',
    last_finished_at: '2026-08-09 04:01:44',
  },
  {
    id: 'job-weekly-report',
    name: '生成周报（停用）',
    schedule: '30 9 * * 1',
    exec_type: 'script',
    exec_value: 'report.py --weekly',
    cwd: '/srv/reports',
    env: 'PYTHONPATH=/srv/libs',
    timeout_sec: 1800,
    enabled: 0,
    once: 0,
    next_at: null,
    created_at: '2026-07-20 14:00:00',
    updated_at: '2026-08-05 16:20:00',
    last_status: 'timeout',
    last_finished_at: '2026-08-03 09:31:02',
  },
  {
    id: 'job-sync-remote',
    name: '远程数据同步',
    schedule: '*/15 * * * *',
    exec_type: 'inline',
    exec_value: 'rsync -avz data/ host:/data/',
    cwd: null,
    env: null,
    timeout_sec: 600,
    enabled: 1,
    once: 0,
    next_at: '2026-08-12 10:45:00',
    created_at: '2026-08-05 11:00:00',
    updated_at: '2026-08-12 10:39:00',
    last_status: 'running',
    last_finished_at: null,
  },
  {
    id: 'job-once-migrate',
    name: '一次性：数据迁移',
    schedule: 'once',
    exec_type: 'inline',
    exec_value: 'migrate --apply',
    cwd: null,
    env: null,
    timeout_sec: 900,
    enabled: 1,
    once: 1,
    next_at: '2026-08-12 12:00:00',
    created_at: '2026-08-11 08:00:00',
    updated_at: '2026-08-11 08:00:00',
    last_status: null,
    last_finished_at: null,
  },
]

/** 示例执行记录（覆盖：cron/manual 触发、各状态、有/无输出、退出码） */
const sampleExecutions = [
  {
    exec_id: 'exec-20260812020321',
    job_id: 'job-nightly-backup',
    status: 'succeeded',
    trigger: 'cron',
    started_at: '2026-08-12 02:00:00',
    finished_at: '2026-08-12 02:03:21',
    exit_code: 0,
    output_path: 'C:\\logs\\scheduler\\job-nightly-backup\\2026-08-12_020000.log',
  },
  {
    exec_id: 'exec-20260812020258',
    job_id: 'job-nightly-backup',
    status: 'succeeded',
    trigger: 'manual',
    started_at: '2026-08-11 22:15:00',
    finished_at: '2026-08-11 22:15:42',
    exit_code: 0,
    output_path: 'C:\\logs\\scheduler\\job-nightly-backup\\2026-08-11_221500.log',
  },
  {
    exec_id: 'exec-20260809040144',
    job_id: 'job-cleanup-tmp',
    status: 'failed',
    trigger: 'cron',
    started_at: '2026-08-09 04:00:00',
    finished_at: '2026-08-09 04:01:44',
    exit_code: 1,
    output_path: 'C:\\logs\\scheduler\\job-cleanup-tmp\\2026-08-09_040000.log',
  },
  {
    exec_id: 'exec-20260803093102',
    job_id: 'job-weekly-report',
    status: 'timeout',
    trigger: 'cron',
    started_at: '2026-08-03 09:30:00',
    finished_at: '2026-08-03 09:31:02',
    exit_code: null,
    output_path: null,
  },
  {
    exec_id: 'exec-20260812103900',
    job_id: 'job-sync-remote',
    status: 'running',
    trigger: 'cron',
    started_at: '2026-08-12 10:39:00',
    finished_at: null,
    exit_code: null,
    output_path: null,
  },
  {
    exec_id: 'exec-20260812103600',
    job_id: 'job-sync-remote',
    status: 'missed',
    trigger: 'cron',
    started_at: null,
    finished_at: null,
    exit_code: null,
    output_path: null,
  },
]

/** URL `?mock=1` 时预置示例数据（默认空态，便于分别评审） */
function seedEnabled(): boolean {
  return new URLSearchParams(window.location.search).get('mock') === '1'
}

let registered = false

/** 注册 dev mock（幂等）：dev-shell 中仅此一处命令 handler */
export async function registerDevMock(context: PluginContext): Promise<void> {
  if (registered) return
  registered = true

  const withSeed = seedEnabled()

  context.commands.register('_http_endpoint', (args: any) => {
    const method: string = args?.method ?? 'GET'
    const path: string = args?.path ?? ''
    const query: Record<string, string> = args?.query ?? {}

    if (method === 'GET' && path === 'task-scheduler/list') {
      return ok({ jobs: withSeed ? sampleJobs : [] })
    }
    if (method === 'GET' && path === 'task-scheduler/logs') {
      const jobId = query.job_id
      const executions = withSeed ? sampleExecutions.filter((e) => e.job_id === jobId) : []
      return ok({ executions })
    }
    return {
      status: 404,
      body: { code: 404, message: `scheduler mock: unknown endpoint ${method} ${path}` },
    }
  })

  console.log(
    withSeed
      ? '[Scheduler] dev-shell mock 已注册（示例数据，?mock=1）'
      : '[Scheduler] dev-shell mock 已注册（空数据，加 ?mock=1 看示例）',
  )
}

export function disposeDevMock(): void {
  // handler 生命周期随 context 释放，无需额外清理
  registered = false
}
