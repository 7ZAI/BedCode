/**
 * 计划任务只读数据访问（spec §10 数据源：list / logs 端点）
 *
 * 统一封装 `_http_endpoint` 命令（与宿主网关 /api/plugin/{id}/... 同路径语义，
 * WASM invoke_command 精确匹配该命令名），解析 `{ status, body: { code, message, data } }`
 * 信封：status != 200 时抛错（message 透传），命令/网络异常原样上抛，
 * 错误态展示由调用方（面板组件）负责。
 *
 * 仅只读查询：CRUD 全部走 CLI（bedtask），本模块不暴露写操作。
 */
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

/** 调度任务（scheduled_jobs 行 + list 端点附带的最近执行摘要列） */
export interface JobDef {
  id: string
  name: string | null
  schedule: string
  exec_type: 'script' | 'inline'
  exec_value: string
  cwd: string | null
  env: string | null
  timeout_sec: number
  /** SQLite 布尔列：1 = 启用，0 = 停用 */
  enabled: number
  /** SQLite 布尔列：1 = 一次性（触发成功后自动停用） */
  once: number
  /** 下次触发时间（本地时区字符串 YYYY-MM-DD HH:MM:SS，字典序可比） */
  next_at: string | null
  created_at: string
  updated_at: string
  /** 最近一次执行的状态摘要（list 端点子查询，可能为 null = 从未执行） */
  last_status: string | null
  last_finished_at: string | null
}

/** 执行记录（job_executions 行；status: waiting|running|succeeded|failed|timeout|missed） */
export interface ExecutionRecord {
  exec_id: string
  job_id: string
  status: string
  /** 触发方式：'cron' | 'manual' */
  trigger: string
  started_at: string | null
  finished_at: string | null
  exit_code: number | null
  /** stdout/stderr 落盘文件路径（运行中/排队中为 null） */
  output_path: string | null
}

/** 端点响应信封（http_response 模块统一形状） */
interface EndpointEnvelope<T> {
  status: number
  body: { code: number; message: string; data?: T }
}

/** WASM 插件 HTTP 端点入口命令名（invoke_command 精确匹配，宿主网关同款路由） */
const HTTP_ENDPOINT_COMMAND = '_http_endpoint'

export function useSchedulerApi(context: PluginContext) {
  async function call<T>(
    method: string,
    path: string,
    query?: Record<string, string | number>,
  ): Promise<T> {
    const res = (await context.commands.execute(HTTP_ENDPOINT_COMMAND, {
      method,
      path,
      body: null,
      query: query ?? {},
    })) as EndpointEnvelope<T> | null
    if (!res || typeof res.status !== 'number') {
      throw new Error('scheduler endpoint: invalid response envelope')
    }
    if (res.status !== 200) {
      throw new Error(res.body?.message || `scheduler endpoint error (${res.status})`)
    }
    return res.body?.data as T
  }

  /** 任务列表（含最近一次执行摘要 last_status / last_finished_at） */
  async function listJobs(): Promise<JobDef[]> {
    const data = await call<{ jobs: JobDef[] }>('GET', 'task-scheduler/list')
    return data?.jobs ?? []
  }

  /** 最近执行记录（limit 与端点默认一致：20，上限 100） */
  async function fetchLogs(jobId: string, limit = 20): Promise<ExecutionRecord[]> {
    const data = await call<{ executions: ExecutionRecord[] }>('GET', 'task-scheduler/logs', {
      job_id: jobId,
      limit,
    })
    return data?.executions ?? []
  }

  return { listJobs, fetchLogs }
}
