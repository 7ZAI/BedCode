/**
 * SchedulerPanelView 组件测试
 *
 * 覆盖：任务列表渲染（启停徽标/状态徽标/next_at/最近执行）、空态、错误态、
 * 选中任务加载执行记录（trigger/时间/exit_code/输出路径）、复制输出路径、
 * scheduler:changed 事件实时刷新、再次点击收起。
 */
import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import SchedulerPanelView from '../components/SchedulerPanelView.vue'
import { makeContext, ok, endpointError, expectEndpoint } from './helpers'

const jobA = {
  id: 'j1',
  name: '备份脚本',
  schedule: '0 0 9 * * *',
  exec_type: 'script',
  exec_value: 'C:\\scripts\\backup.bat',
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

const jobB = {
  id: 'j2',
  name: null,
  schedule: '*/5 * * * * *',
  exec_type: 'inline',
  exec_value: 'echo ping',
  cwd: null,
  env: null,
  timeout_sec: 60,
  enabled: 0,
  once: 1,
  next_at: '2026-01-02 00:00:05',
  created_at: '2026-01-01 08:00:00',
  updated_at: '2026-01-01 08:00:00',
  last_status: null,
  last_finished_at: null,
}

const execDone = {
  exec_id: 'e1',
  job_id: 'j1',
  status: 'succeeded',
  trigger: 'cron',
  started_at: '2026-01-01 09:00:00',
  finished_at: '2026-01-01 09:00:05',
  exit_code: 0,
  output_path: 'C:\\Users\\t\\.bedcode\\scheduler\\e1.log',
}

const execRunning = {
  exec_id: 'e2',
  job_id: 'j1',
  status: 'running',
  trigger: 'manual',
  started_at: '2026-01-02 10:00:00',
  finished_at: null,
  exit_code: null,
  output_path: null,
}

function mountPanel(listJobs: unknown) {
  const m = makeContext()
  m.execute.mockResolvedValueOnce(ok({ jobs: listJobs }))
  const wrapper = mount(SchedulerPanelView, {
    global: { provide: { pluginContext: m.context } },
  })
  return { wrapper, ...m }
}

async function clickJobCard(wrapper: VueWrapper, index: number) {
  await wrapper.findAll('.scheduler-job')[index].trigger('click')
  await flushPromises()
}

beforeEach(() => {
  // 复制输出路径依赖 Clipboard API（happy-dom 默认无），测试前注入桩
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
    configurable: true,
  })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('任务列表', () => {
  it('渲染任务名称/调度表达式/启停徽标/状态徽标/元信息', async () => {
    const { wrapper } = mountPanel([jobA, jobB])
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('备份脚本')
    expect(text).toContain('0 0 9 * * *')
    expect(text).toContain('启用')
    expect(text).toContain('已停用')
    // 最近执行摘要徽标（succeeded）
    expect(text).toContain('成功')
    expect(text).toContain('下次触发: 2026-01-02 09:00:00')
    // 从未执行的任务显示最近执行为空态文案
    expect(text).toContain('从未执行')
    // 未命名任务以 id 兜底展示
    expect(text).toContain('j2')
  })

  it('加载前调用 list 端点', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    expectEndpoint(execute, 'GET', 'task-scheduler/list')
    expect(wrapper.findAll('.scheduler-job')).toHaveLength(1)
  })

  it('状态徽标映射：六种执行状态均有对应文案与配色（token/语义色，无硬编码）', async () => {
    const statuses = [
      { status: 'succeeded', label: '成功', cls: 'bg-green-500/10' },
      { status: 'failed', label: '失败', cls: 'bg-red-500/10' },
      { status: 'timeout', label: '超时', cls: 'bg-orange-500/10' },
      { status: 'missed', label: '错过', cls: 'bg-amber-500/10' },
      { status: 'waiting', label: '排队中', cls: 'bg-[var(--bg-hover)]' },
      { status: 'running', label: '执行中', cls: 'bg-blue-500/10' },
    ]
    const jobs = statuses.map((s, i) => ({
      ...jobA,
      id: `s${i}`,
      name: s.status,
      last_status: s.status,
    }))
    const { wrapper } = mountPanel(jobs)
    await flushPromises()

    for (const s of statuses) {
      // 徽标文案（i18n 标签）
      expect(wrapper.text()).toContain(s.label)
      // 徽标配色类落在对应 span 上
      const badge = wrapper.findAll('span').find((el) => el.text() === s.label)
      expect(badge, `status=${s.status} 的徽标未找到`).toBeTruthy()
      expect(badge!.classes()).toContain(s.cls)
    }
  })

  it('空态：无任务时展示 empty 文案与提示', async () => {
    const { wrapper } = mountPanel([])
    await flushPromises()
    expect(wrapper.text()).toContain('暂无计划任务')
    expect(wrapper.text()).toContain('bedtask')
  })

  it('错误态：list 端点异常时展示错误文案', async () => {
    const m = makeContext()
    m.execute.mockRejectedValueOnce(new Error('plugin not activated'))
    const w = mount(SchedulerPanelView, {
      global: { provide: { pluginContext: m.context } },
    })
    await flushPromises()
    expect(w.text()).toContain('加载失败')
  })

  it('错误态：端点返回非 200 时展示错误文案', async () => {
    const m = makeContext()
    m.execute.mockResolvedValueOnce(endpointError(500, 'scheduler error'))
    const w = mount(SchedulerPanelView, {
      global: { provide: { pluginContext: m.context } },
    })
    await flushPromises()
    expect(w.text()).toContain('加载失败')
  })
})

describe('选中任务 → 执行记录', () => {
  it('点击任务卡片加载 logs 端点并渲染执行记录', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    execute.mockResolvedValueOnce(ok({ job_id: 'j1', executions: [execDone, execRunning] }))

    await clickJobCard(wrapper, 0)

    expectEndpoint(execute, 'GET', 'task-scheduler/logs')
    const text = wrapper.text()
    // 触发方式 + 状态徽标
    expect(text).toContain('定时')
    expect(text).toContain('手动')
    expect(text).toContain('执行中')
    // 时间（finished_at 优先，running 行回退 started_at）
    expect(text).toContain('2026-01-01 09:00:05')
    expect(text).toContain('2026-01-02 10:00:00')
    // 退出码 + 输出路径
    expect(text).toContain('退出码: 0')
    expect(text).toContain('e1.log')
    // 无输出文件的执行行
    expect(text).toContain('无输出文件')
    expect(wrapper.findAll('.scheduler-exec')).toHaveLength(2)
  })

  it('执行记录为空时展示 noExecutions', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    execute.mockResolvedValueOnce(ok({ job_id: 'j1', executions: [] }))
    await clickJobCard(wrapper, 0)
    expect(wrapper.text()).toContain('暂无执行记录')
  })

  it('再次点击收起执行记录', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    execute.mockResolvedValueOnce(ok({ job_id: 'j1', executions: [execDone] }))
    await clickJobCard(wrapper, 0)
    expect(wrapper.findAll('.scheduler-exec')).toHaveLength(1)

    // 收起：执行区移除，且不重新请求 logs 端点
    const logsCallsBefore = execute.mock.calls.filter(
      ([cmd, args]) => cmd === '_http_endpoint' && args?.path === 'task-scheduler/logs',
    ).length
    await clickJobCard(wrapper, 0)
    expect(wrapper.findAll('.scheduler-exec')).toHaveLength(0)
    const logsCallsAfter = execute.mock.calls.filter(
      ([cmd, args]) => cmd === '_http_endpoint' && args?.path === 'task-scheduler/logs',
    ).length
    expect(logsCallsAfter).toBe(logsCallsBefore)
  })
})

describe('输出路径复制', () => {
  it('点击复制调用 Clipboard API 并短暂显示已复制', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    execute.mockResolvedValueOnce(ok({ job_id: 'j1', executions: [execDone] }))
    await clickJobCard(wrapper, 0)

    const copyBtn = wrapper.findAll('.scheduler-exec')[0].find('button')
    await copyBtn.trigger('click')
    await flushPromises()

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(execDone.output_path)
    expect(copyBtn.text()).toContain('已复制')
  })

  it('无输出文件的执行行不渲染复制按钮', async () => {
    const { wrapper, execute } = mountPanel([jobA])
    await flushPromises()
    execute.mockResolvedValueOnce(ok({ job_id: 'j1', executions: [execRunning] }))
    await clickJobCard(wrapper, 0)
    expect(wrapper.findAll('.scheduler-exec')[0].find('button').exists()).toBe(false)
  })
})

describe('scheduler:changed 事件实时刷新', () => {
  it('订阅事件并在到达时重载列表', async () => {
    const { wrapper, context, execute } = mountPanel([jobA])
    await flushPromises()
    expect(context.events.on).toHaveBeenCalledWith('scheduler:changed', expect.any(Function))

    // CLI 侧变更后列表变化：事件到达 → 重新请求 list
    const listCallsBefore = execute.mock.calls.filter(
      ([cmd, args]) => cmd === '_http_endpoint' && args?.path === 'task-scheduler/list',
    ).length
    execute.mockResolvedValueOnce(ok({ jobs: [jobA, jobB] }))
    context.events.emit('scheduler:changed', { job_id: 'j1', status: 'succeeded', action: 'run' })
    await flushPromises()

    const listCallsAfter = execute.mock.calls.filter(
      ([cmd, args]) => cmd === '_http_endpoint' && args?.path === 'task-scheduler/list',
    ).length
    expect(listCallsAfter).toBe(listCallsBefore + 1)
    expect(wrapper.findAll('.scheduler-job')).toHaveLength(2)
  })

  it('卸载时释放事件订阅', async () => {
    const { wrapper, context } = mountPanel([jobA])
    await flushPromises()
    const onMock = context.events.on as Mock
    const disposable = onMock.mock.results[0].value
    const disposeSpy = vi.spyOn(disposable, 'dispose')
    wrapper.unmount()
    expect(disposeSpy).toHaveBeenCalled()
  })
})
