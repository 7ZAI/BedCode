/**
 * 服务器流组合集成测试（L2 场景 2）
 *
 * 协作实体：useServer（composable） + useSettingsStore（真实 Pinia store） +
 * ServerView（服务设置组件挂载）。
 *
 * 覆盖用户路径：配置回显（settings 通道 + server 通道双源一致）→ 启动/停止
 * 服务器（invoke 参数构造）→ 指标轮询（fake timers 驱动）→ 端口修改与
 * 默认配置还原回显。
 *
 * 测试 seam（与 useServer.test.ts 同模式）：
 * - 只 mock @tauri-apps/api/core 的 invoke 边界；vue-echarts 为 canvas 渲染库，
 *   happy-dom 无 2d context，按组件桩替换（与终端流 xterm 同理由，渲染层非被测逻辑）
 * - Pinia / composables / 组件逻辑全部真实执行
 * - fixture 数据全部取自工厂（makeServerStatusInfo / makeNetworkConfig /
 *   makeServerMetrics / makeAppConfig）
 *
 * 断言策略说明：
 * - useServer 的 port/metrics 等为「每次调用新建」的实例 ref（仅 status 是
 *   模块级共享），视图实例的状态无法从测试实例读到 → 视图驱动状态一律断言
 *   DOM（端口输入框/状态文本/指标值），模块级 status 直接断言
 * - 测试与挂载共享同一 pinia：useSettingsStore() 拿到的即视图可见的同一实例
 * - fake timers 下 flushPromises 的 setImmediate 被伪造会挂起，统一用
 *   flushAsync（setTimeout(0) / advanceTimersByTimeAsync(0)）推进
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import ServerView from '@/views/ServerView.vue'
import { useServer } from '@/composables/useServer'
import { useSettingsStore } from '@/stores/settings'
import {
  makeServerStatusInfo,
  makeNetworkConfig,
  makeServerMetrics,
  makeAppConfig,
} from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 + 渲染库桩 ====================

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// echarts 需要 canvas 2d context（happy-dom 无），图表属渲染层非被测逻辑，
// 用组件桩替代；echarts 注册调用（use([...])）仍真实执行
vi.mock('vue-echarts', () => ({
  default: { name: 'VChartStub', template: '<div class="vchart-stub" />' },
}))

// ==================== 测试基建 ====================

/** 后端状态（可变，用例按需改写） */
let backendStatus: ReturnType<typeof makeServerStatusInfo>
let backendNetworkConfig: ReturnType<typeof makeNetworkConfig>
let backendMetrics: ReturnType<typeof makeServerMetrics>

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'get_app_settings':
        return Promise.resolve(
          makeAppConfig({ network: { ...makeAppConfig().network, port: backendStatus.port } }),
        )
      case 'get_server_status':
        return Promise.resolve(backendStatus)
      case 'get_server_network_config':
        return Promise.resolve(backendNetworkConfig)
      case 'get_server_metrics':
        return Promise.resolve(backendMetrics)
      case 'update_server_network_config':
        return Promise.resolve(undefined)
      case 'reset_server_network_config':
        return Promise.resolve(makeNetworkConfig({ port: 8765, auto_start: true }))
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  // 去掉调用数组首元素（命令名），只保留参数：与 toHaveBeenCalledWith 的参数形态一致
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/**
 * 推进异步链：setTimeout(0) 只在微任务队列排空后触发，mock invoke 的纯微任务
 * async 链（onMounted 多级 await）一次调用即可全部推进；fake timers 下
 * setTimeout 被伪造，用 advanceTimersByTimeAsync(0) 等价推进
 */
async function flushAsync(): Promise<void> {
  if (vi.isFakeTimers()) {
    await vi.advanceTimersByTimeAsync(0)
  } else {
    await new Promise((r) => setTimeout(r, 0))
  }
}

let wrapper: ReturnType<typeof mount> | null = null
let pinia: ReturnType<typeof createPinia>
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  pinia = createPinia()
  setActivePinia(pinia)
  backendStatus = makeServerStatusInfo({ status: 'running', port: 9000, auto_start: true })
  backendNetworkConfig = makeNetworkConfig({ port: 9000, auto_start: true })
  backendMetrics = makeServerMetrics({ connections: 3, uptime_secs: 120 })
  installInvokeMock()
  // 重置 useServer 模块级共享 status，并清理上个用例遗留的轮询定时器
  const server = useServer()
  server.status.value = 'stopped'
  server.stopPolling()
  consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  consoleErrorSpy.mockRestore()
  vi.useRealTimers()
})

async function mountView() {
  wrapper = mount(ServerView, {
    global: {
      plugins: [pinia, i18n],
    },
  })
  await flushAsync()
}

/** 按按钮文案查找（wb-btn 工具栏区） */
function buttonByText(text: string) {
  return wrapper!.findAll('button').find((b) => b.text().trim() === text)!
}

/** 端口输入框（NETWORK 分区第一个 number 输入，DOM 顺序先于高级配置） */
function portInput() {
  return wrapper!.find('input[type="number"]').element as HTMLInputElement
}

// ==================== 场景 ====================

describe('服务器流：useServer × useSettingsStore × ServerView', () => {
  it('配置回显：settings 通道与 server 通道双源一致，视图同步端口', async () => {
    // 先走 settings 通道：应用启动时 main.ts 的 loadSettings 路径
    const settingsStore = useSettingsStore()
    await settingsStore.loadSettings()
    await flushAsync()
    expect(settingsStore.settings.network.port).toBe(9000)

    await mountView()
    // server 通道（get_server_status + get_server_network_config）回显：
    // status 为模块级共享 ref，任何 useServer() 实例读到同一值
    expect(useServer().status.value).toBe('running')
    // settings 通道与 server 通道各自回显（两通道独立取数，断言分别落在各自通道上）
    expect(settingsStore.settings.network.port).toBe(9000)
    // 视图端口输入框回显（视图实例的 port ref 经渲染暴露）
    expect(portInput().value).toBe('9000')
    // 状态回显
    expect(wrapper!.text()).toContain('运行中')
  })

  it('停止 → 端口修改 → 启动：invoke 参数构造 + 状态流转', async () => {
    await mountView()

    // 停止：running → stopped（status 模块级共享，测试实例可直读）
    await buttonByText('停止').trigger('click')
    await flushAsync()
    expect(invokeCalls('server_stop')).toEqual([[]])
    expect(useServer().status.value).toBe('stopped')
    expect(wrapper!.text()).toContain('已停止')

    // 修改端口后启动：ServerView.handleStart 先提交未应用端口再启动
    const input = wrapper!.find('input[type="number"]')
    await input.setValue(8080)
    await buttonByText('启动').trigger('click')
    await flushAsync()

    expect(invokeCalls('update_server_port')).toEqual([[{ port: 8080 }]])
    expect(invokeCalls('server_start')).toEqual([[{ port: 8080 }]])
    expect(useServer().status.value).toBe('running')
    // 端口回显（视图实例 port ref 经输入框渲染）
    expect(portInput().value).toBe('8080')
    expect(wrapper!.text()).toContain('运行中')
  })

  it('指标轮询：fake timers 驱动 → 视图监控区联动 + 停止后轮询终止', async () => {
    vi.useFakeTimers()
    await mountView()

    // 启动即运行中：onMounted 已开轮询（2000ms 周期）
    await vi.advanceTimersByTimeAsync(4000)
    expect(invokeCalls('get_server_metrics')).toHaveLength(2)
    // 视图监控区渲染指标：定位「连接数」标签所在格子的值文本（避免全局文本弱断言）
    const connectionsLabel = [...wrapper!.element.querySelectorAll('div')].find(
      (el) => el.textContent === '连接数',
    )!
    expect((connectionsLabel.nextElementSibling as HTMLElement).textContent).toBe('3')

    // 停止后轮询终止：不再产生新调用
    await buttonByText('停止').trigger('click')
    await vi.advanceTimersByTimeAsync(4000)
    expect(invokeCalls('get_server_metrics')).toHaveLength(2)
    expect(useServer().status.value).toBe('stopped')
  })

  it('还原默认配置：reset 返回值回显到端口与高级配置', async () => {
    await mountView()

    await buttonByText('还原默认').trigger('click')
    await flushAsync()

    expect(invokeCalls('reset_server_network_config')).toEqual([[]])
    // 还原默认（port 8765）回显到端口输入框
    expect(portInput().value).toBe('8765')
  })
})
