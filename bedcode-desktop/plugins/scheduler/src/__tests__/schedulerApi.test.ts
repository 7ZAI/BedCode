/**
 * useSchedulerApi 单元测试：端点信封解析 + 错误映射 + 调用参数
 */
import { describe, it, expect } from 'vitest'
import { useSchedulerApi } from '../composables/useSchedulerApi'
import { makeContext, ok, endpointError } from './helpers'

const job = {
  id: 'j1',
  name: 'backup',
  schedule: '0 0 9 * * *',
  exec_type: 'script',
  exec_value: 'C:\\backup.bat',
  cwd: null,
  env: null,
  timeout_sec: 600,
  enabled: 1,
  once: 0,
  next_at: '2026-01-02 09:00:00',
  created_at: '2026-01-01 08:00:00',
  updated_at: '2026-01-01 08:00:00',
  last_status: 'succeeded',
  last_finished_at: '2026-01-01 09:00:05',
}

const execution = {
  exec_id: 'e1',
  job_id: 'j1',
  status: 'succeeded',
  trigger: 'cron',
  started_at: '2026-01-01 09:00:00',
  finished_at: '2026-01-01 09:00:05',
  exit_code: 0,
  output_path: 'C:\\Users\\t\\.bedcode\\scheduler\\e1.log',
}

describe('useSchedulerApi.listJobs', () => {
  it('以 GET task-scheduler/list 调用并返回 jobs', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(ok({ jobs: [job] }))

    const api = useSchedulerApi(context)
    const jobs = await api.listJobs()

    expect(execute).toHaveBeenCalledWith('_http_endpoint', {
      method: 'GET',
      path: 'task-scheduler/list',
      body: null,
      query: {},
    })
    expect(jobs).toEqual([job])
  })

  it('data 缺失时返回空数组', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(ok(null))
    const api = useSchedulerApi(context)
    expect(await api.listJobs()).toEqual([])
  })

  it('端点错误（status != 200）时抛出 body.message', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(endpointError(404, 'job not found: x'))
    const api = useSchedulerApi(context)
    await expect(api.listJobs()).rejects.toThrow('job not found: x')
  })

  it('响应信封非法时抛出错误（不静默返回空）', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(null)
    const api = useSchedulerApi(context)
    await expect(api.listJobs()).rejects.toThrow()
  })
})

describe('useSchedulerApi.fetchLogs', () => {
  it('以 GET task-scheduler/logs?job_id=&limit= 调用并返回 executions', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(ok({ job_id: 'j1', executions: [execution] }))

    const api = useSchedulerApi(context)
    const logs = await api.fetchLogs('j1', 20)

    expect(execute).toHaveBeenCalledWith('_http_endpoint', {
      method: 'GET',
      path: 'task-scheduler/logs',
      body: null,
      query: { job_id: 'j1', limit: 20 },
    })
    expect(logs).toEqual([execution])
  })

  it('limit 默认 20', async () => {
    const { context, execute } = makeContext()
    execute.mockResolvedValue(ok({ executions: [] }))
    const api = useSchedulerApi(context)
    await api.fetchLogs('j1')
    expect(execute.mock.calls[0][1].query.limit).toBe(20)
  })
})
