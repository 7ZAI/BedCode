/**
 * SessionCenterView 行为契约（票 13，插件前端）
 *
 * 契约来源：宿主 `SessionsConfigView.vue`（搬走前的行为基线）+ spec D2/D3。
 * 断言的是**外部可见行为**——发往宿主/插件后端的调用（命令名 + 入参）、
 * 渲染出的可见列表与文案 key；不测内部实现、不测 mock 自身。
 *
 * 演示数据取 `devMock.session`（插件工程持有），保证测试数据与 dev-shell
 * 演示数据同源。
 *
 * 契约清单：
 * - C1 加载：配置经 `session.config.list`、会话经 `context.session.list()`，
 *   会话按 configId 归入对应配置卡片
 * - C2 启动：先预测宿主终端网格再 `session.create`（cols/rows 随预测值传递）
 * - C3 查看终端：运行中会话触发 `context.session.openTerminal`；已停止会话
 *   不触发（只提示「未运行」）
 * - C4 停止：确认后 `session.close` + 关闭终端窗口
 * - C5 删除运行中会话：先 `session.close` 再 `session.action.remove`（顺序）
 * - C6 重启：`session.action.restart`
 * - C7 编辑保存：`session.config.upsert` 收到既有 id 与逐字段同形的草稿；
 *   成功后把本次取值写入插件存储作为下次新建默认值
 * - C8 Tab 切换：「运行中的会话」只列非 stopped / error 会话
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import SessionCenterView from '../components/SessionCenterView.vue'
import devMock from '../devMock'

vi.mock('vue-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'linux' }))

const seed = devMock.session

/** 命令面路由：按命令 id 返演示数据，并记录调用（断言入参） */
async function defaultExecute(command: string, args?: unknown) {
  switch (command) {
    case 'session.config.list':
      return seed.configs
    case 'session.config.upsert':
      return args
    case 'session.config.delete':
      return { removed: true }
    case 'session.create':
      return { sessionId: 'mock-session-new' }
    case 'session.close':
      return { sessionId: (args as { sessionId: string }).sessionId, stopped: true }
    case 'session.action.remove':
      return { sessionId: (args as { sessionId: string }).sessionId, removed: true }
    case 'session.action.restart':
      return { sessionId: (args as { sessionId: string }).sessionId }
    case 'session.environment.wsl-distros':
      return { distros: seed.wslDistros }
    default:
      throw new Error(`unexpected command: ${command}`)
  }
}

const execute = vi.fn(defaultExecute)

const sessionList = vi.fn(async () => seed.sessions)
const openTerminal = vi.fn(async () => true)
const closeTerminal = vi.fn(async () => {})
const isTerminalOpen = vi.fn(() => false)
const predictTerminalSize = vi.fn(async () => ({ cols: 120, rows: 32 }))

const storageGet = vi.fn(async () => seed.formDefaults)
const storageSet = vi.fn(async () => {})

function makeContext(): PluginContext {
  return {
    i18n: { t: (key: string) => key },
    commands: { execute },
    session: { list: sessionList, openTerminal, closeTerminal, isTerminalOpen, predictTerminalSize },
    storage: { get: storageGet, set: storageSet },
  } as unknown as PluginContext
}

function mountView() {
  return mount(SessionCenterView, {
    global: {
      provide: { pluginContext: makeContext() },
      // 弹窗与遮罩走 <Teleport to="body">：stub 掉 teleport 使内容留在组件树内，
      // 断言可直接遍历（否则要查 document.body，且多次挂载会互相污染）
      stubs: { teleport: true },
    },
  })
}

/** 按文案 key 找按钮（i18n 桩直返 key） */
function findButton(wrapper: ReturnType<typeof mountView>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text)
}

function commandsTo(id: string) {
  return execute.mock.calls.filter((c) => c[0] === id)
}

describe('SessionCenterView（会话中心页面）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // clearAllMocks 只清调用记录、不清 mockImplementation：逐用例复位命令面路由，
    // 避免失败注入类用例（C9/C10）污染后续用例（独立、确定性）
    execute.mockImplementation(defaultExecute)
    sessionList.mockResolvedValue(seed.sessions)
    storageGet.mockResolvedValue(seed.formDefaults)
    isTerminalOpen.mockReturnValue(false)
    openTerminal.mockResolvedValue(true)
  })

  it('C1 加载：配置与会话分别经插件命令通道与插件会话 API 取得并按 configId 归组', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.config.list')).toHaveLength(1)
    expect(sessionList).toHaveBeenCalledTimes(1)

    const text = wrapper.text()
    // 两张配置卡片：名称 + 环境徽标 + 命令
    expect(text).toContain('claude-dev')
    expect(text).toContain('linux')
    // 归属：config-1 下的两个会话名都出现（含 waitingInput 的 (1)）
    expect(text).toContain('claude-dev(1)')
    expect(text).toContain('Ubuntu 工具链')
    // 分组标题带计数
    expect(text).toContain('session.section.configs')
  })

  it('C2 启动：先预测宿主终端网格，再以该尺寸创建并启动会话', async () => {
    const wrapper = mountView()
    await flushPromises()

    const startBtn = findButton(wrapper, 'session.button.start')!
    await startBtn.trigger('click')
    await flushPromises()

    expect(predictTerminalSize).toHaveBeenCalledTimes(1)
    const [command, args] = commandsTo('session.create')[0] as [string, Record<string, unknown>]
    expect(command).toBe('session.create')
    expect(args).toMatchObject({
      configId: seed.configs[0].id,
      cols: 120,
      rows: 32,
      start: true,
    })
  })

  it('C3 查看终端：运行中会话触发宿主开窗；已停止会话不触发且提示未运行', async () => {
    const wrapper = mountView()
    await flushPromises()

    // 运行中会话行（卡片内第一个会话名）
    const runningRow = wrapper
      .findAll('[title="session.terminal.view"]')
      .find((el) => el.text().includes('claude-dev'))
    expect(runningRow).toBeTruthy()
    await runningRow!.trigger('click')
    await flushPromises()
    expect(openTerminal).toHaveBeenCalledWith({ id: 'mock-session-1', name: 'claude-dev' })

    // 已停止会话（mock-session-3，落在第二张卡片）：点击后只提示未运行，不开窗
    openTerminal.mockClear()
    const { toast } = await import('vue-sonner')
    const stoppedRow = wrapper
      .findAll('[title="session.terminal.view"]')
      .find((el) => el.text().includes('Ubuntu 工具链'))
    expect(stoppedRow, '停止态会话行仍可点击（点击后提示未运行）').toBeTruthy()
    await stoppedRow!.trigger('click')
    await flushPromises()

    expect(openTerminal).not.toHaveBeenCalled()
    expect(toast.info).toHaveBeenCalledWith('session.error.notRunning')
  })

  it('C4 停止：确认后经 session.close 停止并关闭其终端窗口', async () => {
    const wrapper = mountView()
    await flushPromises()

    const stopBtn = wrapper.findAll('button').find((b) => b.attributes('title') === 'session.button.stop')
    await stopBtn!.trigger('click')
    await flushPromises()

    // 确认弹窗内的「停止」按钮
    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === 'session.button.stop')
      .find((b) => b.element.closest('.fixed') !== null)
    expect(confirm, '停止确认弹窗必须出现').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.close')[0][1]).toEqual({ sessionId: 'mock-session-1' })
    expect(closeTerminal).toHaveBeenCalledWith('mock-session-1')
  })

  it('C5 删除运行中会话：先停止（保留记录）再移除记录，并关闭终端窗口', async () => {
    const wrapper = mountView()
    await flushPromises()

    const delBtn = wrapper
      .findAll('button')
      .filter((b) => b.attributes('title') === 'session.button.delete')
      .find((b) => b.element.closest('.fixed') === null)
    await delBtn!.trigger('click')
    await flushPromises()

    const confirm = findButton(wrapper, 'session.confirm.stopAndDelete')
    expect(confirm, '删除确认弹窗必须出现').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.close')[0][1]).toEqual({ sessionId: 'mock-session-1' })
    expect(commandsTo('session.action.remove')[0][1]).toEqual({ sessionId: 'mock-session-1' })
    expect(closeTerminal).toHaveBeenCalledWith('mock-session-1')
  })

  it('C6 重启：经 session.action.restart 以会话 id 重启', async () => {
    const wrapper = mountView()
    await flushPromises()

    const restartBtn = wrapper
      .findAll('button')
      .find((b) => b.attributes('title') === 'session.terminal.restart')
    await restartBtn!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.action.restart')[0][1]).toEqual({ sessionId: 'mock-session-1' })
  })

  it('C7 编辑保存：upsert 收到既有 id 与逐字段同形草稿，并把取值存为下次新建默认值', async () => {
    const wrapper = mountView()
    await flushPromises()

    const editBtn = findButton(wrapper, 'session.button.edit')!
    await editBtn.trigger('click')
    await flushPromises()

    const saveBtn = findButton(wrapper, 'session.button.save')
    expect(saveBtn, '编辑态弹窗底部按钮为「保存」').toBeTruthy()
    await saveBtn!.trigger('click')
    await flushPromises()

    const [command, draft] = commandsTo('session.config.upsert')[0] as [string, Record<string, unknown>]
    expect(command).toBe('session.config.upsert')
    expect(draft).toEqual({
      id: 'mock-config-1',
      name: 'claude-dev',
      environment: 'linux',
      wslDistro: undefined,
      workingDir: '/home/dev/project',
      command: 'claude',
      autoStart: false,
    })
    expect(storageSet).toHaveBeenCalledWith(
      'session.formDefaults',
      expect.objectContaining({ environment: 'linux', workingDir: '/home/dev/project', command: 'claude' }),
    )
    // 保存后关闭弹窗（不重复提交）
    expect(findButton(wrapper, 'session.button.save')).toBeUndefined()
  })

  it('C8 Tab 切换：「运行中的会话」只列非 stopped 会话', async () => {
    const wrapper = mountView()
    await flushPromises()

    await findButton(wrapper, 'session.tab.running (2)')!.trigger('click')
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('claude-dev')
    // stopped 会话（mock-session-3）不入运行中汇总
    expect(text).not.toContain('Ubuntu 工具链')
  })

  it('C9 启动失败：以错误文案提示且撤下操作遮罩（不静默、不卡在遮罩）', async () => {
    const wrapper = mountView()
    await flushPromises()

    execute.mockImplementationOnce(async (command: string) => {
      if (command === 'session.create') throw new Error('spawn failed')
      return null
    })
    const { toast } = await import('vue-sonner')

    await findButton(wrapper, 'session.button.start')!.trigger('click')
    await flushPromises()

    expect(toast.error).toHaveBeenCalledWith('session.error.startFailed')
    expect(toast.success).not.toHaveBeenCalled()
    // 遮罩必须撤下（否则页面永久不可交互）
    expect(wrapper.find('[class*="bg-black/40"]').exists()).toBe(false)
    expect(closeTerminal).not.toHaveBeenCalled()
  })

  it('C10 删除失败：不再继续移除记录，并以错误文案提示', async () => {
    const wrapper = mountView()
    await flushPromises()

    execute.mockImplementation(async (command: string) => {
      if (command === 'session.close') throw new Error('close failed')
      if (command === 'session.config.list') return seed.configs
      return null
    })
    const { toast } = await import('vue-sonner')

    const delBtn = wrapper
      .findAll('button')
      .filter((b) => b.attributes('title') === 'session.button.delete')
      .find((b) => b.element.closest('.fixed') === null)
    await delBtn!.trigger('click')
    await flushPromises()
    await findButton(wrapper, 'session.confirm.stopAndDelete')!.trigger('click')
    await flushPromises()

    // 停止失败即中止：不得留下「记录已删、进程仍在」的半成品状态
    expect(commandsTo('session.action.remove')).toHaveLength(0)
    expect(toast.error).toHaveBeenCalledWith('session.error.deleteFailed')
  })
})
