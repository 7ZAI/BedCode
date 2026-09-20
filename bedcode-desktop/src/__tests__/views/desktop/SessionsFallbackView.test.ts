/**
 * SessionsFallbackView 行为契约（票 13）
 *
 * 会话页富交互已随 `com.bedcode.session` 插件迁出，本页只在插件未激活 / error /
 * 停用时渲染。被测契约（外部可见行为）：
 * - C1 兜底可辨识：页头标题 + 兜底提示（用户知道当前不是插件页）
 * - C2 配置列：有配置即列出（名称 / 环境徽标 / 命令），点「启动」以该配置调宿主启动
 * - C3 运行中列：只列非 stopped / error 会话（与插件侧同一口径）
 * - C4 停止：确认后调 killSession，成功后刷新列表
 * - C5 删除：确认后 killSession + deleteSession 两调用（先停后删）
 * - C6 加载失败不白屏：列表为空 + 错误 toast
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import SessionsFallbackView from '@/views/SessionsFallbackView.vue'
import * as commands from '@/composables/useDesktopCommands'

vi.mock('@/composables/useDesktopCommands', () => ({
  listSessionConfigs: vi.fn(async () => []),
  listSessions: vi.fn(async () => []),
  startSession: vi.fn(async () => 'session-1'),
  killSession: vi.fn(async () => {}),
  deleteSession: vi.fn(async () => {}),
}))

const toast = { success: vi.fn(), error: vi.fn(), info: vi.fn() }
vi.mock('@/composables/useToast', () => ({
  useToast: () => toast,
}))

vi.mock('@/components/Modal.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="modal"><slot /><slot name="footer" /></div>',
    props: ['modelValue', 'title', 'size'],
  },
}))

const mockListConfigs = vi.mocked(commands.listSessionConfigs)
const mockListSessions = vi.mocked(commands.listSessions)
const mockStartSession = vi.mocked(commands.startSession)
const mockKillSession = vi.mocked(commands.killSession)
const mockDeleteSession = vi.mocked(commands.deleteSession)

const configs = [
  {
    id: 'cfg-1',
    name: 'Dev Shell',
    environment: 'linux',
    working_dir: '/tmp',
    command: 'bash',
    auto_start: false,
  },
]

const sessions = [
  { id: 's-1', configId: 'cfg-1', name: 'Dev Shell', status: 'running' },
  { id: 's-2', configId: 'cfg-1', name: 'Dev Shell(1)', status: 'stopped' },
]

function mountView() {
  const pinia = createPinia()
  setActivePinia(pinia)
  return mount(SessionsFallbackView, { global: { plugins: [pinia, i18n] } })
}

/** 在弹窗内按文案找按钮（列表与弹窗可能有同名按钮） */
function modalButton(wrapper: ReturnType<typeof mountView>, text: string) {
  const modal = wrapper.find('.modal')
  return modal.findAll('button').find((b) => b.text() === text)
}

describe('SessionsFallbackView（宿主兜底壳）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListConfigs.mockResolvedValue([] as never)
    mockListSessions.mockResolvedValue([] as never)
  })

  it('C1 兜底可辨识：页头标题与兜底提示同时渲染', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('h2').text()).toBe('终端会话')
    expect(wrapper.text()).toContain('会话中心插件未启用，当前为宿主兜底界面')
  })

  it('C2 配置列：列出配置并显示环境徽标与命令，点启动以该配置调用宿主命令', async () => {
    mockListConfigs.mockResolvedValue(configs as never)
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('Dev Shell')
    expect(wrapper.text()).toContain('linux')
    expect(wrapper.text()).toContain('bash')

    const startBtn = wrapper.findAll('button').find((b) => b.text() === '启动')
    expect(startBtn).toBeDefined()
    await startBtn!.trigger('click')
    await flushPromises()

    expect(mockStartSession).toHaveBeenCalledWith('cfg-1')
  })

  it('C2b 无配置时显示空态与插件提示', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('暂无会话配置')
    expect(wrapper.text()).toContain('请先启用插件')
  })

  it('C3 运行中列：只列非 stopped / error 会话', async () => {
    mockListSessions.mockResolvedValue(sessions as never)
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('Dev Shell')
    expect(wrapper.text()).toContain('运行中')
    // stopped 会话不进「运行中的会话」列表 → 名称只出现一次
    expect(wrapper.text()).not.toContain('Dev Shell(1)')
  })

  it('C4 停止：确认后调 killSession 并刷新列表', async () => {
    mockListSessions.mockResolvedValue([sessions[0]] as never)
    const wrapper = mountView()
    await flushPromises()
    mockListSessions.mockClear()

    const stopBtn = wrapper.findAll('button').find((b) => b.text() === '停止')
    await stopBtn!.trigger('click')
    await flushPromises()

    const confirmBtn = modalButton(wrapper, '停止')!
    await confirmBtn.trigger('click')
    await flushPromises()

    expect(mockKillSession).toHaveBeenCalledWith('s-1')
    expect(mockListSessions).toHaveBeenCalled()
  })

  it('C5 删除：确认后先停后删（两调用）', async () => {
    mockListSessions.mockResolvedValue([sessions[0]] as never)
    const wrapper = mountView()
    await flushPromises()

    const deleteBtn = wrapper.findAll('button').find((b) => b.text() === '删除')
    await deleteBtn!.trigger('click')
    await flushPromises()

    const confirmBtn = modalButton(wrapper, '停止并删除')!
    await confirmBtn.trigger('click')
    await flushPromises()

    expect(mockKillSession).toHaveBeenCalledWith('s-1')
    expect(mockDeleteSession).toHaveBeenCalledWith('s-1')
  })

  it('C6 加载失败不白屏：列表空且给出错误提示', async () => {
    mockListConfigs.mockRejectedValue(new Error('boom'))
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('暂无会话配置')
    expect(toast.error).toHaveBeenCalled()
  })
})
