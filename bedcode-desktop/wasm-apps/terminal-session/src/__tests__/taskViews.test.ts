/**
 * 任务域视图行为契约（票 17，自 `com.bedcode.auto-task` 前端搬入）
 *
 * 旧插件工程此前没有任何测试面（spec S3 记为覆盖缺口），搬迁正好补上。
 * 契约来源：搬迁前的组件行为 + spec D6「界面维持，贡献方换人」（像素与交互不变，
 * 只换归属与命令命名空间）。断言全部是外部可见行为——发往插件后端的命令名与入参、
 * 事件驱动的重取数、可见文案 key（i18n 桩直返 key）；不测内部状态、不测 mock 自身。
 *
 * 契约清单：
 * - M1 常驻不取数：弹窗随激活常驻挂载，但只在可见时才发取数命令（避免每 webview
 *   常驻轮询后端）
 * - M2 取数走新命令命名空间：队列 / 当前任务 / 开关 / 预设四条各一次且带 session_id
 * - M3 入队：输入回车 → `session.task.queue-add` 收 {session_id, prompt}，随后重取队列
 * - M4 开关：目标值在 await 前固化（事件先于 invoke 返回到达也不回翻）
 * - M5 事件门：队列变更仅在可见时重取
 * - H1 历史视图挂载即拉五域数据（记录 / 定时 / 会话配置 / 运行中会话 / 预设）
 * - H2 记录筛选：history-list 与 history-stats 收同一份筛选条件，且带分页 limit/offset
 * - H3 无会话时的创建路径落预设（不写队列）
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import TaskQueueModal from '../components/TaskQueueModal.vue'
import TaskHistoryView from '../components/TaskHistoryView.vue'
import { taskModalVisible } from '../state'

// 日期选择器与宿主共享下拉：只关心命令调用与渲染文案，第三方控件内部实现不进契约
vi.mock('@vuepic/vue-datepicker', () => ({
  default: { name: 'Datepicker', render: () => null },
}))
vi.mock('@binblink/bedcode-plugin-sdk-desktop/ui', () => ({
  default: { name: 'Select', props: ['modelValue'], render: () => null },
}))

// 宿主共享 router：终端窗口路由参数即当前会话 id
const routerStub = vi.hoisted(() => ({ current: null as unknown }))
vi.mock('@binblink/bedcode-plugin-sdk-desktop', () => ({
  getRouter: () => routerStub.current,
}))

const SESSION_ID = 'sess-1'

/** 命令面路由：按新命令 id 返回最小可渲染载荷，并记录调用 */
async function defaultExecute(command: string, args?: unknown) {
  switch (command) {
    case 'session.task.queue-list':
      return { tasks: [], active_task: null, session_id: (args as { session_id: string }).session_id }
    case 'session.task.history-list':
      return { tasks: [], total: 0 }
    case 'session.task.history-stats':
      return { total: 0, completed: 0, success_rate: 0 }
    case 'session.task.preset-list':
      return { presets: [{ id: 'p1', prompt: '预设内容', created_at: '' }] }
    case 'session.task.session-settings':
      return { auto_execute: false, auto_answer: false }
    case 'session.task.running-sessions':
      return { sessions: [{ session_id: SESSION_ID, is_supported: true, task_status: 'idle' }] }
    case 'session.task.session-configs':
      return { configs: [{ id: 'c1', name: 'claude-dev', is_supported: true }] }
    case 'session.task.scheduled-list':
      return { jobs: [] }
    case 'session.task.queue-add':
      return { task_id: 't1', position: 0 }
    case 'session.task.preset-create':
      return { preset_id: 'p2' }
    case 'session.task.set-auto-mode':
      return { ok: true }
    default:
      throw new Error(`unexpected command: ${command}`)
  }
}

const execute = vi.fn(defaultExecute)

function makeContext(): PluginContext {
  return {
    i18n: { t: (key: string) => key, getI18n: () => undefined },
    commands: { execute },
    events: { on: () => ({ dispose: () => {} }) },
  } as unknown as PluginContext
}

function mountComponent(component: unknown) {
  return mount(component as never, {
    global: { provide: { pluginContext: makeContext() }, stubs: { teleport: true } },
  })
}

function commandsTo(id: string) {
  return execute.mock.calls.filter((c) => c[0] === id)
}

describe('TaskQueueModal（终端工具栏任务弹窗）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
    routerStub.current = { currentRoute: { value: { params: { id: SESSION_ID } } } }
    taskModalVisible.value = false
  })

  it('M1 常驻挂载不发取数命令，打开后才发', async () => {
    const wrapper = mountComponent(TaskQueueModal)
    await flushPromises()
    expect(execute).not.toHaveBeenCalled()

    taskModalVisible.value = true
    await flushPromises()
    expect(commandsTo('session.task.queue-list')).toHaveLength(1)
    wrapper.unmount()
  })

  it('M2 打开时按新命令命名空间取四域数据，且逐条带当前会话 id', async () => {
    const wrapper = mountComponent(TaskQueueModal)
    taskModalVisible.value = true
    await flushPromises()

    for (const id of [
      'session.task.queue-list',
      'session.task.history-list',
      'session.task.session-settings',
    ]) {
      const calls = commandsTo(id)
      expect(calls, `${id} 应被调用`).toHaveLength(1)
      expect(calls[0][1]).toMatchObject({ session_id: SESSION_ID })
    }
    // 预设是全局列表：调用不带任何入参（不按会话过滤）
    expect(commandsTo('session.task.preset-list')).toEqual([['session.task.preset-list']])
    wrapper.unmount()
  })

  it('M3 输入回车入队：queue-add 收 session_id + prompt，成功后重取队列', async () => {
    const wrapper = mountComponent(TaskQueueModal)
    taskModalVisible.value = true
    await flushPromises()
    const queueCallsBefore = commandsTo('session.task.queue-list').length

    const input = wrapper.find('textarea')
    expect(input.exists()).toBe(true)
    await input.setValue('  跑一轮回归  ')
    await input.trigger('keydown', { key: 'Enter', shiftKey: false, isComposing: false })
    await flushPromises()

    expect(commandsTo('session.task.queue-add')).toEqual([
      ['session.task.queue-add', { session_id: SESSION_ID, prompt: '跑一轮回归' }],
    ])
    expect(commandsTo('session.task.queue-list').length).toBe(queueCallsBefore + 1)
    wrapper.unmount()
  })

  it('M4 两个开关各自独立下发目标值（不是整包覆盖）', async () => {
    const wrapper = mountComponent(TaskQueueModal)
    taskModalVisible.value = true
    await flushPromises()

    // 开关是无文本的 role=switch 按钮，标签在同行的 p 里；按出现顺序取
    const switches = wrapper.findAll('button[role="switch"]')
    expect(switches).toHaveLength(2)
    expect(switches[0].attributes('aria-checked')).toBe('false')

    await switches[0].trigger('click')
    await flushPromises()
    expect(commandsTo('session.task.set-auto-mode')).toEqual([
      ['session.task.set-auto-mode', { session_id: SESSION_ID, auto_execute: true }],
    ])

    await switches[1].trigger('click')
    await flushPromises()
    expect(commandsTo('session.task.set-auto-mode')[1]).toEqual([
      'session.task.set-auto-mode',
      { session_id: SESSION_ID, auto_answer: true },
    ])
    wrapper.unmount()
  })

  it('M5 队列变更事件仅在弹窗可见时重取队列', async () => {
    let onEvent: ((data: unknown) => void) | null = null
    execute.mockImplementation(defaultExecute)
    const context = {
      i18n: { t: (key: string) => key, getI18n: () => undefined },
      commands: { execute },
      events: {
        on: (event: string, handler: (data: unknown) => void) => {
          if (event === 'task:queue-changed') onEvent = handler
          return { dispose: () => {} }
        },
      },
    } as unknown as PluginContext

    const wrapper = mount(TaskQueueModal, {
      global: { provide: { pluginContext: context }, stubs: { teleport: true } },
    })
    taskModalVisible.value = true
    await flushPromises()
    expect(onEvent).toBeTruthy()

    const before = commandsTo('session.task.queue-list').length
    taskModalVisible.value = false
    onEvent?.({ session_id: SESSION_ID })
    await flushPromises()
    expect(commandsTo('session.task.queue-list')).toHaveLength(before)

    taskModalVisible.value = true
    await flushPromises()
    const afterOpen = commandsTo('session.task.queue-list').length
    onEvent?.({ session_id: SESSION_ID })
    await flushPromises()
    expect(commandsTo('session.task.queue-list').length).toBeGreaterThan(afterOpen)
    wrapper.unmount()
  })
})

describe('TaskHistoryView（侧边栏任务历史）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
    routerStub.current = { currentRoute: { value: { params: { id: SESSION_ID } } } }
    taskModalVisible.value = false
  })

  it('H1 挂载即拉五域数据（命令名全部落 session.task.* 命名空间）', async () => {
    const wrapper = mountComponent(TaskHistoryView)
    await flushPromises()

    for (const id of [
      'session.task.history-list',
      'session.task.history-stats',
      'session.task.scheduled-list',
      'session.task.session-configs',
      'session.task.running-sessions',
      'session.task.preset-list',
    ]) {
      expect(commandsTo(id), `${id} 未被调用`).toHaveLength(1)
    }
    wrapper.unmount()
  })

  it('H2 列表与统计共用同一份筛选条件，列表另带分页参数', async () => {
    const wrapper = mountComponent(TaskHistoryView)
    await flushPromises()

    const list = commandsTo('session.task.history-list')[0][1] as Record<string, unknown>
    const stats = commandsTo('session.task.history-stats')[0][1] as Record<string, unknown>
    expect(list).toEqual({ ...stats, limit: expect.any(Number), offset: expect.any(Number) })
    wrapper.unmount()
  })

  it('H3 未选会话时创建落预设、不写任何队列；选了适配会话才入队', async () => {
    const inputs = () => wrapper.findAll('textarea')
    const createBox = () => inputs()[inputs().length - 1]

    // 无运行中会话：会话选择框回退「预存」，创建只落预设
    execute.mockImplementation(async (command: string, args?: unknown) => {
      if (command === 'session.task.running-sessions') return { sessions: [] }
      return defaultExecute(command, args)
    })
    let wrapper = mountComponent(TaskHistoryView)
    await flushPromises()
    await createBox().setValue('夜间跑一轮回归')
    await createBox().trigger('keydown', { key: 'Enter', shiftKey: false, isComposing: false })
    await flushPromises()
    expect(commandsTo('session.task.preset-create')).toEqual([
      ['session.task.preset-create', { prompt: '夜间跑一轮回归' }],
    ])
    expect(commandsTo('session.task.queue-add')).toEqual([])
    wrapper.unmount()

    // 有适配会话：创建直接入该会话队列
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
    wrapper = mountComponent(TaskHistoryView)
    await flushPromises()
    await createBox().setValue('白天跑冒烟')
    await createBox().trigger('keydown', { key: 'Enter', shiftKey: false, isComposing: false })
    await flushPromises()
    expect(commandsTo('session.task.queue-add')).toEqual([
      ['session.task.queue-add', { session_id: SESSION_ID, prompt: '白天跑冒烟' }],
    ])
    expect(commandsTo('session.task.preset-create')).toEqual([])
    wrapper.unmount()
  })
})
