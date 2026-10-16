/**
 * ServerView 端口保存 → 重启确认行为契约（issue：端口修改后不生效 + 无线索）
 *
 * 需求（用户 + 后台语义）：
 * - `update_server_port` 只持久化配置 + 更新 supervisor 内存端口，**不重启监听
 *   socket**——真实端口只在服务器重启/下次启动后生效；
 * - UI 要求：修改端口后提示用户「是否立即重启」，是 → 保存+重启一次到位，
 *   否 → 保存配置、下次启动生效。
 *
 * 契约清单：
 * - C1 端口未修改：点保存不触发任何调用（防误保存 / 无意义请求）
 * - C2 运行中保存：updatePort(新端口) + 弹出「立即重启」确认
 * - C3 确认立即重启：restartServer + loadStatus + 刷新输入框 + 成功 toast + 弹窗关闭
 * - C4 选择稍后重启：不重启（restartServer 不调用）+ 延迟生效 toast + 弹窗关闭
 * - C5 停止态保存：updatePort + 延迟生效 toast，**不**弹确认（无需重启）
 * - C6 updatePort 失败：透出错误（showUserError），不弹窗、不提示成功
 * - C7 端口输入框回车同样触发保存（keyup.enter 绑定）
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { flushPromises, shallowMount } from '@vue/test-utils'
import { defineComponent, h, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import ServerView from '@/views/ServerView.vue'

// ==================== mocks ====================

const toastSuccess = vi.fn()
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ success: toastSuccess, error: vi.fn(), info: vi.fn(), warning: vi.fn() }),
}))

const MODAL_OPEN_EVENT = 'update:modelValue'

/** useServer Mock 替身：ref 状态可被测试改写，updatePort 同步内存端口（同后台语义） */
function makeServerMocks(initialPort = 8765) {
  const status = ref<'running' | 'stopped'>('running')
  const port = ref(initialPort)
  const mocks = {
    status: status as any,
    port: port as any,
    autoStart: ref(true) as any,
    localIps: ref<string[]>([]) as any,
    metrics: ref(null) as any,
    metricsHistory: ref([]) as any,
    loading: ref(false) as any,
    networkConfig: ref(null) as any,
    loadStatus: vi.fn(async () => {}),
    startServer: vi.fn(async () => {}),
    stopServer: vi.fn(async () => {}),
    restartServer: vi.fn(async () => {}),
    updatePort: vi.fn(async (p: number) => {
      port.value = p
    }),
    updateAutoStart: vi.fn(async () => {}),
    startPolling: vi.fn(),
    stopPolling: vi.fn(),
    loadNetworkConfig: vi.fn(async () => {}),
    updateNetworkConfig: vi.fn(async () => {}),
    resetNetworkConfig: vi.fn(async () => {
      port.value = 8765
      return { port: 8765 } as any
    }),
  }
  return { mocks, status, port }
}

vi.mock('@/composables/useServer', () => ({
  useServer: () => serverMocks.mocks,
}))

let serverMocks: ReturnType<typeof makeServerMocks>
beforeEach(() => {
  serverMocks = makeServerMocks()
  toastSuccess.mockClear()
})

// echarts / 图表太重，stub 掉
vi.mock('vue-echarts', () => ({ default: defineComponent({ name: 'VChartStub', render: () => h('div') }) }))
vi.mock('echarts/core', () => ({ use: vi.fn() }))
vi.mock('echarts/charts', () => ({ LineChart: {} }))
vi.mock('echarts/components', () => ({
  TitleComponent: {},
  TooltipComponent: {},
  LegendComponent: {},
  GridComponent: {},
}))
vi.mock('echarts/renderers', () => ({ CanvasRenderer: {} }))

/** Modal 替身：透传 modelValue 供断言，转发 close 事件 */
const ModalStub = defineComponent({
  name: 'ModalStub',
  props: { modelValue: Boolean },
  emits: [MODAL_OPEN_EVENT, 'close'],
  render() {
    return h('div', { class: 'modal-stub' })
  },
})

/** 测试用最小 i18n 实例：只含断言所涉 key；缺失 key 回退为 key 本身 */
function createTestI18n() {
  return createI18n({
    legacy: false,
    locale: 'zh-CN',
    fallbackLocale: 'zh-CN',
    messages: {
      'zh-CN': {
        desktop: {
          server: {
            portSave: '保存',
            portRestartConfirmTitle: '保存端口',
            portRestartConfirmBody: '端口修改需要重启服务器后才能生效，是否立即重启？',
            portRestartNow: '立即重启',
            portRestartLater: '稍后重启',
            portSavedDelayed: 'PORT_SAVED_DELAYED',
            portRestartApplied: 'PORT_RESTART_APPLIED',
          },
        },
      },
    },
  })
}

function mountView() {
  return shallowMount(ServerView, {
    global: {
      plugins: [createTestI18n()],
      stubs: { Modal: ModalStub, VChart: true, PluginPageToolbar: true, 'echarts': true },
    },
  })
}

function modalOpen(wrapper: any): boolean {
  return wrapper.findComponent(ModalStub).props('modelValue') ?? false
}

describe('ServerView 端口保存 / 重启确认', () => {
  it('C1 端口未修改时保存不触发任何调用', async () => {
    const wrapper = await mountView()
    await flushPromises()
    expect(serverMocks.mocks.updatePort).not.toHaveBeenCalled()
    await (wrapper.vm as any).handleSavePort()
    expect(serverMocks.mocks.updatePort).not.toHaveBeenCalled()
    expect(modalOpen(wrapper)).toBe(false)
  })

  it('C2 运行中保存新端口 → updatePort + 弹出确认', async () => {
    const wrapper = await mountView()
    await flushPromises()
    serverMocks.mocks.port.value = 8765
    // 改输入框（模拟用户在输入框敲新端口）
    ;(wrapper.vm as any).portInput = 9000
    await (wrapper.vm as any).handleSavePort()
    expect(serverMocks.mocks.updatePort).toHaveBeenCalledWith(9000)
    expect(modalOpen(wrapper)).toBe(true)
  })

  it('C3 确认立即重启 → restartServer + loadStatus + 输入框同步新端口 + 成功 toast', async () => {
    const wrapper = await mountView()
    await flushPromises()
    ;(wrapper.vm as any).portInput = 9000
    await (wrapper.vm as any).handleSavePort()
    await (wrapper.vm as any).applyPortRestart()

    expect(serverMocks.mocks.restartServer).toHaveBeenCalledTimes(1)
    expect(serverMocks.mocks.loadStatus).toHaveBeenCalled()
    expect((wrapper.vm as any).portInput).toBe(9000) // syncPortInput 后与新端口一致
    expect(modalOpen(wrapper)).toBe(false)
    expect(toastSuccess).toHaveBeenCalledWith('PORT_RESTART_APPLIED')
  })

  it('C4 选择稍后重启 → 不重启 + 延迟生效 toast + 弹窗关闭', async () => {
    const wrapper = await mountView()
    await flushPromises()
    ;(wrapper.vm as any).portInput = 9000
    await (wrapper.vm as any).handleSavePort()
    ;(wrapper.vm as any).deferPortRestart()
    await flushPromises()

    expect(serverMocks.mocks.restartServer).not.toHaveBeenCalled()
    expect(toastSuccess).toHaveBeenCalledWith('PORT_SAVED_DELAYED')
    expect(modalOpen(wrapper)).toBe(false)
  })

  it('C5 停止态保存新端口 → updatePort + 延迟生效 toast，不弹确认', async () => {
    serverMocks.status.value = 'stopped'
    const wrapper = await mountView()
    await flushPromises()
    ;(wrapper.vm as any).portInput = 9000
    await (wrapper.vm as any).handleSavePort()

    expect(serverMocks.mocks.updatePort).toHaveBeenCalledWith(9000)
    expect(modalOpen(wrapper)).toBe(false)
    expect(toastSuccess).toHaveBeenCalledWith('PORT_SAVED_DELAYED')
  })

  it('C6 updatePort 失败 → 错误被透出处理（不弹确认、不提示成功，保存不再进行）', async () => {
    serverMocks.mocks.updatePort.mockRejectedValueOnce(new Error('persist failed'))
    const wrapper = await mountView()
    await flushPromises()
    ;(wrapper.vm as any).portInput = 9000
    await (wrapper.vm as any).handleSavePort()
    await flushPromises()

    expect(serverMocks.mocks.updatePort).toHaveBeenCalledWith(9000)
    expect(modalOpen(wrapper)).toBe(false)
    expect(toastSuccess).not.toHaveBeenCalled()
    // 失败的端口不视为已保存：内存端口仍旧值，输入框仍保留待保存的修改
    expect(serverMocks.mocks.port.value).toBe(8765)
  })

  it('C7 端口输入框回车触发保存（keyup.enter 绑定 handleSavePort）', async () => {
    const wrapper = await mountView()
    await flushPromises()
    ;(wrapper.vm as any).portInput = 9000
    const input = wrapper.find('input')
    await input.trigger('keyup', { key: 'Enter' })
    await flushPromises()
    expect(serverMocks.mocks.updatePort).toHaveBeenCalledWith(9000)
  })
})