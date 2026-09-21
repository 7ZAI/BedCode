/**
 * 终端渲染组件（插件版）组装集成测试（票 03a / 03b）
 *
 * 被测对象：`TerminalPreview.vue`（自宿主拆分产物迁入后的组装层——xterm
 * 实例化 + kernel / writePipeline / renderer / resize / scroll / settingsSync
 * / IME 各域接线 + 输出桥 attachSink）。
 *
 * 行为契约：方案 1 迁移后渲染链路逐字等价宿主（宿主 terminal-flow 集成测试
 * 接缝不变），本文件补**组件级组装**验证：
 * - 输出桥接线：caps.output.attachSink 在 running 会话挂载时被调，字节帧
 *   sink.onData → 写入管线 → 真实 xterm buffer 渲染（DOM 渲染器，.xterm-rows
 *   文本可读——happy-dom 无 WebGL 上下文，initWebGL 按设计回退 DOM，双赢）；
 * - 设置同步 watch 行为（handoff 点名补验）：字号/主题变化 → 防抖持久化到
 *   accessor（save 调用），外部变化同步；
 * - 卸载清理：attachSink 返回的 detach 被调、防抖 timer 不再触发 save。
 *
 * 测试 seam（与宿主 terminal-flow 同策略）：
 * - 真实 xterm（happy-dom 中 open() 进带尺寸 DOM 元素可用，宿主已实测）；
 * - 真实 timers（xterm 解析/渲染依赖真实异步 tick，不用 fake timers）；
 * - mock 边界：vue-i18n（t 恒等）、vue-sonner（toast）、@tauri-apps/plugin-os
 *   （platform → windows，isLinux=false 走 WebGL→回退 DOM 路径）；
 * - 容器显式尺寸（happy-dom 无布局引擎，clientWidth/Height 取 style 值）。
 *
 * 输入链路（onData → context.terminal.sendInput）不在本文件覆盖：参数构造
 * 已由宿主 terminal-flow.test.ts 锁定（write_to_session 同形），插件版是同构
 * 替换（sendInput），接线风险低；真实键盘事件链在 happy-dom 不可靠。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import TerminalPreview from '../components/terminal/TerminalPreview.vue'
import {
  TERMINAL_HOST_CAPABILITIES_KEY,
  type TerminalHostCapabilities,
} from '../components/terminal/terminalHostCapabilities'
import type { TerminalSettingsAccessor } from '../composables/terminal/useTerminalSettingsSync'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { SessionInfo } from '../composables/terminal/model'

// ==================== mock 边界 ====================

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('vue-sonner', () => ({
  toast: { warning: vi.fn(), error: vi.fn(), info: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-os', () => ({
  platform: () => 'windows',
}))

// ==================== 测试基建 ====================

/** 内存版 settings accessor（save 写回 getter；与 settingsSync 外部变化同步 watch 闭环）。
 * 必须用 ref 存值：settingsSync 的 watch(() => settings.getFontSize()) 依赖 getter 读取时
 * 的响应式追踪——普通对象属性非响应式，save 改值后 watch 永不触发（01c 同款教训） */
function makeSettings(overrides?: Partial<TerminalSettingsAccessor>): TerminalSettingsAccessor {
  const fontSize = ref(12)
  const theme = ref('dracula')
  const bgImage = ref('')
  const bgOpacity = ref(30)
  const accessor: TerminalSettingsAccessor = {
    getFontSize: () => fontSize.value,
    getTheme: () => theme.value,
    getBgImage: () => bgImage.value,
    getBgOpacity: () => bgOpacity.value,
    getServerPort: () => 8080,
    save: vi.fn((patch) => {
      if (patch.fontSize != null) fontSize.value = patch.fontSize
      if (patch.theme != null) theme.value = patch.theme
      if (patch.bgImage != null) bgImage.value = patch.bgImage
      if (patch.bgOpacity != null) bgOpacity.value = patch.bgOpacity
    }),
    onChange: () => () => {},
    ...overrides,
  }
  return accessor
}

/** mock caps：attachSink 捕获 sink + 记录 detach 调用 */
function makeCaps(overrides?: Partial<TerminalHostCapabilities>): TerminalHostCapabilities {
  const settings = makeSettings()
  const attachSink = vi.fn((_sink: unknown) => vi.fn())
  return {
    settings,
    bgImage: {
      pickAndSet: vi.fn(async () => false),
      remove: vi.fn(async () => {}),
      imageName: '',
      hasImage: false,
    },
    output: { attachSink },
    extensions: {
      terminalToolbarItems: { value: [] } as never,
      titleBarItems: { value: [] } as never,
      pageToolbarItems: { value: [] } as never,
    },
    ...overrides,
  }
}

function makeContext(): PluginContext {
  return {
    // resize 裁决默认返回 applied（服务端正统渲染端）；具体命令行为按用例覆盖
    commands: {
      execute: vi.fn(async (_cmd: string) => ({
        status: 'applied',
        canonical: { kind: 'desktop' },
      })),
    },
    terminal: { sendInput: vi.fn(), onOutput: vi.fn(() => () => {}), onInput: vi.fn(() => () => {}) },
    session: { list: vi.fn(), get: vi.fn(), onStatusChange: vi.fn(() => () => {}) },
    ui: {} as never,
    events: { on: vi.fn(() => () => {}), emit: vi.fn() },
    storage: {} as never,
    http: {} as never,
    i18n: {} as never,
    _disposables: [],
    id: 'com.bedcode.session',
    extensionPath: '',
  } as unknown as PluginContext
}

function makeRunningSession(): SessionInfo {
  return {
    id: 'sess-1',
    name: 'test shell',
    config_id: 'cfg-1',
    configId: 'cfg-1',
    status: 'running',
    session_type: 'shell',
    sessionType: 'shell',
    created_at: '2026-09-21T00:00:00Z',
    createdAt: '2026-09-21T00:00:00Z',
    startedAt: '2026-09-21T00:00:00Z',
  }
}

/** 等待 xterm 解析/渲染的真实异步 tick（宿主 terminal-flow 同款） */
async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 20))
}

/** 真实 xterm DOM 渲染器输出的可见文本（happy-dom 无 WebGL → DOM 回退路径） */
function renderedRowsText(wrapper: VueWrapper): string {
  return wrapper.element.querySelector('.xterm-rows')?.textContent ?? ''
}

let wrapper: VueWrapper | null = null
let caps: TerminalHostCapabilities

/** wrapper.vm 的 exposed 属性类型（@vue/test-utils 不推导 defineExpose 类型） */
interface ExposedTerminalPreview {
  fontSize: number
  terminalTheme: string
  themeNames: Record<string, string>
}

function vm(wrapper: VueWrapper): ExposedTerminalPreview {
  return wrapper.vm as unknown as ExposedTerminalPreview
}

beforeEach(() => {
  vi.clearAllMocks()
  Object.defineProperty(window, 'devicePixelRatio', { value: 1, configurable: true })
  caps = makeCaps()
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

describe('TerminalPreview（插件版组装）', () => {
  it('挂载 running 会话：接入输出桥 + 实例化真实 xterm', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })

    // onMounted 异步链完成后：输出桥被接入（running 状态 attach）
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })
    // 真实 xterm 已挂载进容器（.xterm 元素存在）
    expect(wrapper.element.querySelector('.xterm')).toBeTruthy()
    // expose 的响应式状态初始值 = accessor 初始值（wrapper.vm 对 exposed ref 自动解包）
    expect(vm(wrapper).fontSize).toBe(12)
    expect(vm(wrapper).terminalTheme).toBe('dracula')
    expect(vm(wrapper).themeNames).toHaveProperty('dracula')
  })

  it('输出帧经写入管线渲染到 xterm buffer（DOM 渲染器文本可读）', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })

    // 取出接入的 sink，注入一帧字节（宿主 Channel 帧已过游标校验，直接入队）
    const sink = vi.mocked(caps.output.attachSink).mock.calls[0][0]
    sink.onData({ data: new TextEncoder().encode('hello terminal') })
    sink.onData({ data: new TextEncoder().encode('\r\nworld') })

    // 写入管线 rAF 合并 → terminal.write → xterm 解析 → DOM 渲染器文本
    await vi.waitFor(
      () => {
        const text = renderedRowsText(wrapper!)
        expect(text).toContain('hello terminal')
        expect(text).toContain('world')
      },
      { timeout: 3000 },
    )
  })

  it('字号变化 → options 生效 + 300ms 防抖持久化（accessor.save）', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })

    // 用户侧字号变化（非 Linux：不乘 1.15；wrapper.vm 对 exposed ref 自动解包，赋值写回）
    vm(wrapper).fontSize = 16
    await nextTick()

    // 防抖窗口内不持久化
    expect(caps.settings.save).not.toHaveBeenCalledWith({ fontSize: 16 })
    // 300ms 后 save 触发
    await vi.waitFor(() => expect(caps.settings.save).toHaveBeenCalledWith({ fontSize: 16 }), {
      timeout: 1500,
    })
  })

  it('主题变化 → 300ms 防抖持久化（accessor.save）', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })

    vm(wrapper).terminalTheme = 'light'
    await vi.waitFor(() => expect(caps.settings.save).toHaveBeenCalledWith({ theme: 'light' }), {
      timeout: 1500,
    })
  })

  it('外部设置变化同步（accessor save 写回 → 外部 watch 捕获）', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })

    // 模拟宿主设置面板外部修改：save 写回 accessor 内存值（mock accessor 的
    // save 已实现写回 getter）→ settingsSync 的外部变化 watch 应同步 fontSize
    ;(caps.settings.save as ReturnType<typeof vi.fn>)({ fontSize: 18 })
    await nextTick()
    await vi.waitFor(() => expect(vm(wrapper!).fontSize).toBe(18), { timeout: 1000 })
  })

  it('卸载清理：断开输出桥 + 防抖 timer 不再触发 save', async () => {
    const context = makeContext()
    wrapper = mount(TerminalPreview, {
      props: { session: makeRunningSession() },
      global: {
        provide: {
          pluginContext: context,
          [TERMINAL_HOST_CAPABILITIES_KEY]: caps,
        },
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(caps.output.attachSink).toHaveBeenCalled(), { timeout: 2000 })

    // 触发一个防抖窗口内的字号变化（timer 挂起）
    vm(wrapper).fontSize = 14
    await nextTick()

    // 卸载：输出桥 detach 被调；xterm dispose 清理
    const detach = vi.mocked(caps.output.attachSink).mock.results[0].value
    wrapper.unmount()
    wrapper = null
    expect(detach).toHaveBeenCalled()
    expect(document.querySelector('.xterm')).toBeNull()

    // disposeSettingsSync 已取消防抖：等待超过防抖窗口后不再有新增 save 调用
    const saveCalls = vi.mocked(caps.settings.save).mock.calls.length
    await flushAsync()
    await new Promise((r) => setTimeout(r, 500))
    expect(vi.mocked(caps.settings.save).mock.calls.length).toBe(saveCalls)
  })
})
