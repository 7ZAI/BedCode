/**
 * 终端渲染组件（插件版）组装集成测试（票 03a / 03b / 04）
 *
 * 被测对象：`TerminalPreview.vue`（自宿主拆分产物迁入后的组装层——xterm
 * 实例化 + kernel / writePipeline / renderer / resize / scroll / settingsSync
 * / IME 各域接线 + 输出拉取轮询）。
 *
 * 行为契约：方案 1 迁移后渲染链路逐字等价宿主（宿主 terminal-flow 集成测试
 * 接缝不变），本文件补**组件级组装**验证：
 * - 输出拉取（票 04）：running 会话挂载时经插件命令面 `session.output.pull`
 *   轮询拉取（插件 WASM → host-session.output-ring-fetch 原语，WIT list<u8>
 *   二进制直传）；返回的数据帧 sink.onData → 写入管线 → 真实 xterm buffer
 *   渲染（DOM 渲染器，.xterm-rows 文本可读——happy-dom 无 WebGL 上下文，
 *   initWebGL 按设计回退 DOM，双赢）；
 * - truncated resync：宿主环淘汰后游标落后 → 清屏重锚 + 截断提示（toast）；
 * - 设置同步 watch 行为（handoff 点名补验）：字号/主题变化 → 防抖持久化到
 *   accessor（save 调用），外部变化同步；
 * - 卸载清理：轮询定时器停止（不再有新的 pull 调用）、防抖 timer 不再触发 save。
 *
 * 测试 seam（与宿主 terminal-flow 同策略）：
 * - 真实 xterm（happy-dom 中 open() 进带尺寸 DOM 元素可用，宿主已实测）；
 * - 真实 timers（xterm 解析/渲染依赖真实异步 tick；轮询 interval 亦为真实
 *   setInterval——卸载清理断言依赖 interval 停止后调用数冻结）；
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

/** mock caps：设置/背景图/扩展点三桥 + 输出拉取走命令面（票 05 契约无 output 桥） */
function makeCaps(overrides?: Partial<TerminalHostCapabilities>): TerminalHostCapabilities {
  const settings = makeSettings()
  return {
    settings,
    bgImage: {
      pickAndSet: vi.fn(async () => false),
      remove: vi.fn(async () => {}),
      imageName: '',
      hasImage: false,
    },
    extensions: {
      terminalToolbarItems: { value: [] } as never,
      titleBarItems: { value: [] } as never,
      pageToolbarItems: { value: [] } as never,
    },
    ...overrides,
  }
}

/**
 * mock context：命令面按名路由——`session.output.pull` 默认返回 null（追平），
 * 用例可注入数据帧队列（依次出队）；`session.action.resize` 默认 applied。
 */
interface TestContext extends PluginContext {
  /** 注入输出拉取响应队列（依次出队；队空后返回 null 追平） */
  __setPullResponses: (items: (unknown | null)[]) => void
}

function makeContext(): TestContext {
  let pullResponses: (unknown | null)[] = []
  const execute = vi.fn(async (cmd: string) => {
    if (cmd === 'session.output.pull') {
      const next = pullResponses.shift()
      return next === undefined ? null : next
    }
    if (cmd === 'session.action.resize') return { status: 'applied', canonical: { kind: 'desktop' } }
    return null
  })
  return {
    commands: { execute },
    terminal: { sendInput: vi.fn(), onOutput: vi.fn(() => () => {}), onInput: vi.fn(() => () => {}) },
    session: { list: vi.fn(), get: vi.fn(), onStatusChange: vi.fn(() => () => {}) },
    ui: {} as never,
    events: { on: vi.fn(() => () => {}), emit: vi.fn() },
    storage: {} as never,
    http: {} as never,
    i18n: {} as never,
    _disposables: [],
    id: 'com.bedcode.terminal-session',
    extensionPath: '',
    // 返回对象带注入器：测试用例设置拉取响应队列
    __setPullResponses: (items: (unknown | null)[]) => {
      pullResponses = items
    },
  } as unknown as TestContext
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

/** 挂载 running 会话（公共夹具）并等待输出拉取就绪 */
async function mountRunning() {
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
  // 就绪信号：轮询已发起（pull 命令被调）+ xterm 挂载
  await vi.waitFor(() => expect(context.commands.execute).toHaveBeenCalledWith('session.output.pull', expect.anything()), {
    timeout: 2000,
  })
  await flushAsync()
  return context
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
  it('挂载 running 会话：发起输出拉取轮询 + 实例化真实 xterm', async () => {
    const context = await mountRunning()
    // 真实 xterm 已挂载进容器（.xterm 元素存在）
    expect(wrapper!.element.querySelector('.xterm')).toBeTruthy()
    // 轮询经插件命令面拉取（票 04 撤宿主 Channel 桥：无 caps.output，断言 pull 命令）
    expect(context.commands.execute).toHaveBeenCalledWith(
      'session.output.pull',
      expect.objectContaining({ sessionId: 'sess-1', fromOffset: 0 }),
    )
    // expose 的响应式状态初始值 = accessor 初始值（wrapper.vm 对 exposed ref 自动解包）
    expect(vm(wrapper!).fontSize).toBe(12)
    expect(vm(wrapper!).terminalTheme).toBe('dracula')
    expect(vm(wrapper!).themeNames).toHaveProperty('dracula')
  })

  it('输出数据帧经写入管线渲染到 xterm buffer（DOM 渲染器文本可读）', async () => {
    const context = makeContext()
    const text = 'hello terminal\r\nworld'
    // 首批返回数据帧（字节数组 = WASM 原语拉取后插件命令面的 wire 形状），后续追平
    context.__setPullResponses([
      {
        data: Array.from(new TextEncoder().encode(text)),
        nextOffset: text.length,
        truncated: false,
      },
    ])
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

    // 写入管线 rAF 合并 → terminal.write → xterm 解析 → DOM 渲染器文本
    await vi.waitFor(
      () => {
        const rendered = renderedRowsText(wrapper!)
        expect(rendered).toContain('hello terminal')
        expect(rendered).toContain('world')
      },
      { timeout: 3000 },
    )

    // 游标已推进：下一次拉取带上 nextOffset（续拉不重复）
    const pullArgs = vi.mocked(context.commands.execute).mock.calls
      .filter(([cmd]) => cmd === 'session.output.pull')
      .map(([, args]) => args)
    expect(pullArgs.some((args) => (args as { fromOffset?: number }).fromOffset === text.length)).toBe(true)
  })

  it('truncated 重锚：环淘汰后游标落后 → 清屏 + 截断提示 + 从现存段起播', async () => {
    const context = makeContext()
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // 首帧 truncated：现存段 [8,12) = "tail"（min_offset=8）
    context.__setPullResponses([
      {
        data: Array.from(new TextEncoder().encode('tail')),
        nextOffset: 12,
        truncated: true,
      },
    ])
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

    // 现存段渲染（清屏重锚后播放）
    await vi.waitFor(() => expect(renderedRowsText(wrapper!)).toContain('tail'), { timeout: 3000 })
    // 截断提示（historyTruncated 首次触发：logTruncated 经 console.warn 落日志）
    await vi.waitFor(
      () => expect(warnSpy.mock.calls.some((args) => String(args[0]).includes('截断'))).toBe(true),
      { timeout: 2000 },
    )
    // 游标落回 nextOffset（后续拉取不带旧游标）
    const pullArgs = vi.mocked(context.commands.execute).mock.calls
      .filter(([cmd]) => cmd === 'session.output.pull')
      .map(([, args]) => args)
    expect(pullArgs.some((args) => (args as { fromOffset?: number }).fromOffset === 12)).toBe(true)
    warnSpy.mockRestore()
  })

  it('字号变化 → options 生效 + 300ms 防抖持久化（accessor.save）', async () => {
    await mountRunning()

    // 用户侧字号变化（非 Linux：不乘 1.15；wrapper.vm 对 exposed ref 自动解包，赋值写回）
    vm(wrapper!).fontSize = 16
    await nextTick()

    // 防抖窗口内不持久化
    expect(caps.settings.save).not.toHaveBeenCalledWith({ fontSize: 16 })
    // 300ms 后 save 触发
    await vi.waitFor(() => expect(caps.settings.save).toHaveBeenCalledWith({ fontSize: 16 }), {
      timeout: 1500,
    })
  })

  it('主题变化 → 300ms 防抖持久化（accessor.save）', async () => {
    await mountRunning()
    vm(wrapper!).terminalTheme = 'light'
    await vi.waitFor(() => expect(caps.settings.save).toHaveBeenCalledWith({ theme: 'light' }), {
      timeout: 1500,
    })
  })

  it('外部设置变化同步（accessor save 写回 → 外部 watch 捕获）', async () => {
    await mountRunning()

    // 模拟宿主设置面板外部修改：save 写回 accessor 内存值（mock accessor 的
    // save 已实现写回 getter）→ settingsSync 的外部变化 watch 应同步 fontSize
    ;(caps.settings.save as ReturnType<typeof vi.fn>)({ fontSize: 18 })
    await nextTick()
    await vi.waitFor(() => expect(vm(wrapper!).fontSize).toBe(18), { timeout: 1000 })
  })

  it('卸载清理：轮询停止（无新 pull 调用）+ 防抖 timer 不再触发 save', async () => {
    const context = await mountRunning()

    // 触发一个防抖窗口内的字号变化（timer 挂起）
    vm(wrapper!).fontSize = 14
    await nextTick()

    // 卸载：xterm dispose 清理；轮询定时器清掉
    wrapper!.unmount()
    wrapper = null
    expect(document.querySelector('.xterm')).toBeNull()

    const pullCalls = vi.mocked(context.commands.execute).mock.calls.filter(
      ([cmd]) => cmd === 'session.output.pull',
    ).length
    // 等待超过轮询快档间隔（100ms）——卸载后不再有新的 pull 调用
    await new Promise((r) => setTimeout(r, 350))
    const pullCallsAfter = vi.mocked(context.commands.execute).mock.calls.filter(
      ([cmd]) => cmd === 'session.output.pull',
    ).length
    expect(pullCallsAfter).toBe(pullCalls)

    // disposeSettingsSync 已取消防抖：等待超过防抖窗口后不再有新增 save 调用
    const saveCalls = vi.mocked(caps.settings.save).mock.calls.length
    await flushAsync()
    await new Promise((r) => setTimeout(r, 500))
    expect(vi.mocked(caps.settings.save).mock.calls.length).toBe(saveCalls)
  })
})
