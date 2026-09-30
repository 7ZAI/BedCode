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
 * 输入链路（onData → 命令通道 `session.input`）不在本文件覆盖：参数构造由本文件
 * 的 mock 命令路由可见（票 08 起宿主 `context.terminal.sendInput` 已退役，
 * 插件写自家会话输入走自有命令通道）；真实键盘事件链在 happy-dom 不可靠。
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
  /** 注入 `session.output.watermarks` 诊断响应条目（驻留退出快照用） */
  __setWatermarkReport: (rows: unknown[]) => void
  /** 触发组件已订阅的插件事件（P2 output-notify 用例：模拟宿主→插件→前端事件到达） */
  __emitEvent: (name: string, payload: unknown) => void
}

function makeContext(): TestContext {
  let pullResponses: (unknown | null)[] = []
  let watermarkRows: unknown[] = []
  /** 插件事件订阅表：`events.on` 登记 / 释放按真实语义走（供 `__emitEvent` 触发） */
  const eventHandlers = new Map<string, ((payload: unknown) => void)[]>()
  const execute = vi.fn(async (cmd: string) => {
    if (cmd === 'session.output.pull') {
      const next = pullResponses.shift()
      return next === undefined ? null : next
    }
    if (cmd === 'session.output.watermarks') {
      return { entries: watermarkRows, count: watermarkRows.length }
    }
    if (cmd === 'session.action.resize') return { status: 'applied', canonical: { kind: 'desktop' } }
    return null
  })
  return {
    commands: { execute },
    // 票 08/09：`terminal.sendInput` 与 `session.list/get/onStatusChange` 已退役
    // （宿主会话数据 / 输入命令面与内核状态订阅转接通道注销），只留观察面与窗口原语
    terminal: { onOutput: vi.fn(() => () => {}), onInput: vi.fn(() => () => {}) },
    session: { predictTerminalSize: vi.fn(async () => null) },
    ui: {} as never,
    events: {
      on: vi.fn((name: string, handler: (payload: unknown) => void) => {
        const list = eventHandlers.get(name) ?? []
        list.push(handler)
        eventHandlers.set(name, list)
        return () => {
          const index = list.indexOf(handler)
          if (index >= 0) list.splice(index, 1)
        }
      }),
      emit: vi.fn(),
    },
    storage: {} as never,
    http: {} as never,
    // 插件组件统一经 context.i18n.t 取文案（带插件 ID 前缀）；桩为恒等 t
    i18n: { t: (key: string) => key, getI18n: () => undefined, registerMessages: vi.fn() },
    _disposables: [],
    id: 'com.bedcode.terminal-session',
    extensionPath: '',
    // 返回对象带注入器：测试用例设置拉取响应队列
    __setPullResponses: (items: (unknown | null)[]) => {
      pullResponses = items
    },
    __setWatermarkReport: (rows: unknown[]) => {
      watermarkRows = rows
    },
    __emitEvent: (name: string, payload: unknown) => {
      for (const handler of eventHandlers.get(name) ?? []) handler(payload)
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

/**
 * 终端挂载宿主元素（xterm.open() 直接挂载其下）。
 *
 * 取 xterm 根元素的父节点而非测试专用选择器/属性：该元素就是 scoped CSS 覆盖层
 * （`:deep(.xterm .xterm-viewport)`）的祖先，CSS 变量 `--term-bg` 在此下发、
 * 由 `.xterm-viewport` 继承——变量挂错层或漏挂，断言即红。
 */
function terminalHost(wrapper: VueWrapper): HTMLElement {
  const xtermRoot = wrapper.element.querySelector('.xterm')
  expect(xtermRoot, 'xterm 根元素未挂载，无法定位终端宿主').toBeTruthy()
  return xtermRoot!.parentElement as HTMLElement
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

/** 派发一次按键到 xterm 隐藏 textarea（happy-dom 需显式补 keyCode/which） */
function pressKey(target: VueWrapper, keyCode: number, key = 'a'): void {
  const textarea = target.element.querySelector('.xterm-helper-textarea')
  if (!textarea) throw new Error('xterm helper textarea not found')
  const ev = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
  Object.defineProperty(ev, 'keyCode', { get: () => keyCode })
  Object.defineProperty(ev, 'which', { get: () => keyCode })
  textarea.dispatchEvent(ev)
}

/** 某命令的调用次数 */
function countCalls(context: TestContext, cmd: string): number {
  return vi.mocked(context.commands.execute).mock.calls.filter(([c]) => c === cmd).length
}

/** 全部 ack 调用的参数（按调用顺序） */
function ackCalls(context: TestContext): { sessionId: string; offset: number }[] {
  return vi
    .mocked(context.commands.execute)
    .mock.calls.filter(([c]) => c === 'session.output.ack')
    .map(([, args]) => args as { sessionId: string; offset: number })
}

/** 文本输出帧（wire 形状：字节数组 + 末偏移）；`startOffset` 为帧内首字节偏移 */
function textFrame(text: string, startOffset: number) {
  const data = Array.from(new TextEncoder().encode(text))
  return { data, nextOffset: startOffset + data.length, truncated: false }
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

  it('truncated 重锚：环淘汰后游标落后 → 清屏 + 后台日志 + 从现存段起播', async () => {
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
    // 截断静默：仅后台日志（logTruncated 经 console.warn 落日志，不弹 toast）
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

  // ==================== 终端底色同源下发（黑带回归线） ====================
  // 契约：xterm 行网格只铺 rows×行高，容器高不是行高整数倍时底部余量不画任何
  // cell，露出的是覆盖层 .xterm-viewport 的底色。覆盖层底色若沿用 xterm.css 硬编码
  // 的 #000，非黑主题（default 本身即 #000000，故缺陷只在非黑主题暴露）就会在
  // 「显示区与窗口底边之间」留一条黑带（真机截图实测 ~14 CSS px）。因此主题底色
  // 必须同时下发到两个消费点：宿主容器 background-color + 覆盖层读取的 --term-bg。

  it('正例：底色同源 —— 容器 background-color 与 --term-bg 同为当前主题底色（非 #000）', async () => {
    await mountRunning()

    const host = terminalHost(wrapper!)
    // dracula 主题底色 = #1e1e2e（字面量钉住契约，不从主题表反查）
    expect(host.style.getPropertyValue('--term-bg').trim()).toBe('#1e1e2e')
    // 容器自身底色同步（接受 hex / rgb() 两种序列化形式）
    expect(host.style.backgroundColor).toMatch(/30,\s*30,\s*46|#1e1e2e/i)
    // 非黑主题下不得落回 xterm.css 的 #000（落回即黑带复现）
    expect(host.style.getPropertyValue('--term-bg').trim()).not.toBe('#000000')
    // 不得绑成背景图模式的透明主题底色 rgba(0,0,0,0)（否则覆盖层永远透明 → 残影）
    expect(host.style.getPropertyValue('--term-bg')).not.toMatch(/rgba|transparent/i)
  })

  it('正例：切换到 solarizedDark（真机截图所用主题）→ --term-bg 跟随为 #002b36', async () => {
    await mountRunning()
    // 前置：dracula 底色
    expect(terminalHost(wrapper!).style.getPropertyValue('--term-bg').trim()).toBe('#1e1e2e')

    vm(wrapper!).terminalTheme = 'solarizedDark'

    await vi.waitFor(
      () => expect(terminalHost(wrapper!).style.getPropertyValue('--term-bg').trim()).toBe('#002b36'),
      { timeout: 1000 },
    )
  })

  it('反例：未知主题键 → --term-bg 取 default 回退底色（非空串，否则覆盖层回退到 #000）', async () => {
    await mountRunning()

    vm(wrapper!).terminalTheme = 'no-such-theme'

    await vi.waitFor(
      () => expect(terminalHost(wrapper!).style.getPropertyValue('--term-bg').trim()).toBe('#000000'),
      { timeout: 1000 },
    )
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

  it('resize 命令首次失败自动重试（竞态加固）：重试结果进入裁决链路 → 覆盖确认弹窗', async () => {
    const context = makeContext()
    const execute = vi.mocked(context.commands.execute)
    let resizeCalls = 0
    execute.mockImplementation(async (cmd: string) => {
      if (cmd === 'session.output.pull') return null
      if (cmd === 'session.action.resize') {
        resizeCalls += 1
        if (resizeCalls === 1) {
          // 真实形状：命令通道抛非 Error 对象（旧实现日志里显示成 `{}`，无法定位）
          throw { message: '会话不存在：sess-1' }
        }
        // 重试（或后续 onResize）成功：返回需要确认覆盖 → 应走通裁决链路弹窗
        return { status: 'needsConfirmation', currentCanonical: { kind: 'mobile', deviceName: 'Pixel' } }
      }
      return null
    })
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
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

    // 重试延迟（300ms）后裁决结果生效：弹窗出现即证明「失败 → 重试成功」链路走通
    await vi.waitFor(
      () => expect(wrapper!.text()).toContain('session.terminal.rendererOverrideTitle'),
      { timeout: 3000 },
    )
    // 首次失败后必然有后续成功调用（onResize 也可能追加，故断下界）
    expect(resizeCalls).toBeGreaterThanOrEqual(2)
    // 契约守卫（2026-09-27）：resize 载荷必须携带请求方身份 `requester`——插件
    // WASM `ResizeRequest` 强制要求，缺省即报 `missing field 'requester'` 并静默回退
    // applied（PTY 尺寸从不跟随网格 → 输出换行错位格式混乱，实测日志 3× WARN）。
    // 真实形状：{ sessionId, cols, rows, force, requester: { kind: 'desktop' } }
    const resizePayloads = execute.mock.calls
      .filter(([cmd]) => cmd === 'session.action.resize')
      .map(([, arg]) => arg)
    expect(resizePayloads.length).toBeGreaterThan(0)
    for (const payload of resizePayloads) {
      expect(payload).toMatchObject({ requester: { kind: 'desktop' } })
    }
    // 诊断日志带可读原因（非 `{}`）
    expect(
      warnSpy.mock.calls.some((args) => String(args[0]).includes('重试成功')),
    ).toBe(true)
    warnSpy.mockRestore()
  })
})

// ==================== 输出背压（ack 未确认窗口 / 驻留） ====================

describe('TerminalPreview 输出背压（ack 窗口 / 驻留）', () => {
  it('F1 交付即账：数据交付后按交付水位回发 ack（首次立即回发）', async () => {
    const context = makeContext()
    context.__setPullResponses([textFrame('hello', 0)])
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

    await vi.waitFor(() => expect(ackCalls(context).length).toBeGreaterThan(0), { timeout: 2000 })
    // ack 偏移 = 交付帧的末偏移（5），会话 id 正确
    expect(ackCalls(context)[0]).toEqual({ sessionId: 'sess-1', offset: 5 })
  })

  it('F1 节流：累计交付达 64 KiB 阈值 → 立即回发新水位', async () => {
    const context = makeContext()
    const big = 'x'.repeat(64 * 1024)
    // 首帧小（首次 ack 立即回发 offset=2），次帧 64 KiB → 阈值达标立即回发
    context.__setPullResponses([textFrame('hi', 0), textFrame(big, 2)])
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

    await vi.waitFor(
      () => expect(ackCalls(context).some((a) => a.offset === 2 + 64 * 1024)).toBe(true),
      { timeout: 3000 },
    )
  })

  it('F1 节流：未达阈值时由 250 ms 空闲兜底回发', async () => {
    const context = makeContext()
    context.__setPullResponses([textFrame('ab', 0), textFrame('cd', 2)])
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

    // 首帧立即回发 offset=2
    await vi.waitFor(() => expect(ackCalls(context).some((a) => a.offset === 2)).toBe(true), {
      timeout: 2000,
    })
    // 次帧仅 +2 字节（未达 64 KiB）→ 由空闲兜底（250 ms）回发 offset=4
    await vi.waitFor(() => expect(ackCalls(context).some((a) => a.offset === 4)).toBe(true), {
      timeout: 2000,
    })
  })

  it('F2 驻留退避：throttled 不写入、不推进游标，退避 200 ms 后再拉', async () => {
    const context = makeContext()
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    context.__setPullResponses([
      { data: [], nextOffset: 0, truncated: false, throttled: true, unacked: 200 * 1024 },
      textFrame('after', 0),
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

    // 进入驻留（抑制态）
    await vi.waitFor(
      () => expect(warnSpy.mock.calls.some((a) => String(a[0]).includes('进入驻留'))).toBe(true),
      { timeout: 2000 },
    )
    // 退避窗口内：第二帧未被消费（无写入、无 ack）
    await new Promise((r) => setTimeout(r, 80))
    expect(renderedRowsText(wrapper!)).not.toContain('after')
    expect(ackCalls(context)).toEqual([])
    // 退避结束（200 ms）后恢复拉取并渲染
    await vi.waitFor(() => expect(renderedRowsText(wrapper!)).toContain('after'), { timeout: 2000 })
    warnSpy.mockRestore()
  })

  it('F3 resync：截断重锚后游标前进到现存段末（不回到 minOffset）+ ack 水位重锚', async () => {
    const context = makeContext()
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // 首帧正常交付（游标/ack 前进到 3）→ 次帧截断：现存段 [8,12) = "tail"（minOffset=8）
    context.__setPullResponses([
      textFrame('abc', 0),
      { data: Array.from(new TextEncoder().encode('tail')), nextOffset: 12, truncated: true },
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

    // 被淘汰前缀不可恢复：ack 最终上报现存段末偏移 12（非旧游标 3 / 非 minOffset 8）
    // （本帧仅 4 字节未达 64 KiB 阈值，故由 250 ms 空闲兜底回发——等它而非只等“有 ack”）
    await vi.waitFor(() => expect(ackCalls(context).some((a) => a.offset === 12)).toBe(true), {
      timeout: 2000,
    })
    // 游标同样前进到现存段末 12——回到 minOffset 会重复渲染同一段
    await vi.waitFor(
      () => {
        const pullArgs = vi
          .mocked(context.commands.execute)
          .mock.calls.filter(([cmd]) => cmd === 'session.output.pull')
          .map(([, args]) => args as { fromOffset?: number })
        expect(pullArgs.some((args) => args.fromOffset === 12)).toBe(true)
        expect(pullArgs.some((args) => args.fromOffset === 8)).toBe(false)
      },
      { timeout: 3000 },
    )
    warnSpy.mockRestore()
  })

  it('F4 输入即时拉取：按键写入后立即触发一轮 pull（不等 50 ms 轮询）', async () => {
    const context = await mountRunning()
    const pullsBefore = countCalls(context, 'session.output.pull')
    pressKey(wrapper!, 65)
    await new Promise((r) => setTimeout(r, 30))
    // 输入已投递
    expect(countCalls(context, 'session.input')).toBeGreaterThan(0)
    // 30 ms < 50 ms 轮询间隔：新增 pull 只可能来自输入即时拉取
    expect(countCalls(context, 'session.output.pull')).toBeGreaterThan(pullsBefore)
  })

  it('F5 接线：快档 50 ms 轮询在接线处生效（260 ms 内至少 3 轮新 pull）', async () => {
    const context = await mountRunning()
    const before = countCalls(context, 'session.output.pull')
    // 50 ms 快档 → 260 ms 内约 5 tick；旧 100 ms/500 ms 节奏只能拿到 0-2
    await new Promise((r) => setTimeout(r, 260))
    expect(countCalls(context, 'session.output.pull') - before).toBeGreaterThanOrEqual(3)
  })

  it('G2 驻留退出：取一次水位快照并把观测计数打进 info 日志', async () => {
    const context = makeContext()
    const infoSpy = vi.spyOn(console, 'info').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // 注入水位快照（诊断读面是插件真源；此处按 wire 形状喂前端解析分支）
    context.__setWatermarkReport([
      {
        sessionId: 'sess-1',
        pushed: 200_000,
        acked: 1024,
        unacked: 198_976,
        parked: false,
        parkCount: 1,
        unparkCount: 1,
        throttledPulls: 3,
        truncatedCount: 2,
      },
    ])
    context.__setPullResponses([
      { data: [], nextOffset: 0, truncated: false, throttled: true, unacked: 200 * 1024 },
      textFrame('after', 0),
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

    // 驻留退出（抑制后拉到数据）→ 拉一次水位诊断快照，且仅一次
    await vi.waitFor(() => expect(countCalls(context, 'session.output.watermarks')).toBe(1), {
      timeout: 2000,
    })
    // 快照内容进日志：驻留时长 + 退出后未确认量 + 累计驻留/抑制/环淘汰计数
    await vi.waitFor(
      () =>
        expect(
          infoSpy.mock.calls.some(
            (a) =>
              String(a[0]).includes('退出驻留') &&
              String(a[0]).includes('驻留次数=1') &&
              String(a[0]).includes('抑制次数=3') &&
              String(a[0]).includes('环淘汰=2') &&
              String(a[0]).includes('unacked=198976'),
          ),
        ).toBe(true),
      { timeout: 2000 },
    )
    warnSpy.mockRestore()
    infoSpy.mockRestore()
  })

  it('F6 驻留超时：持续抑制超 30 s 打一次 warn 并继续拉取', async () => {
    const context = makeContext()
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // 以 Date.now 快进模拟 30 s 驻留（每次读取 +16 s），避免真实等待
    let now = 1_000_000
    const nowSpy = vi.spyOn(Date, 'now').mockImplementation(() => {
      now += 16_000
      return now
    })
    const throttled = () => ({
      data: [],
      nextOffset: 0,
      truncated: false,
      throttled: true,
      unacked: 200 * 1024,
    })
    context.__setPullResponses([throttled(), throttled(), throttled(), throttled(), throttled()])
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

    await vi.waitFor(
      () =>
        expect(warnSpy.mock.calls.some((a) => String(a[0]).includes('输出背压超时'))).toBe(true),
      { timeout: 3000 },
    )
    // 每次驻留只告警一次（zombieWarned 抑制重复）
    const zombieWarns = warnSpy.mock.calls.filter((a) => String(a[0]).includes('输出背压超时'))
    expect(zombieWarns.length).toBe(1)
    nowSpy.mockRestore()
    warnSpy.mockRestore()
  })

  it('P2 输出可用通知：命中本会话立即拉一轮，非本会话忽略，卸载后不再触发', async () => {
    const context = await mountRunning()

    // 订阅已建立（事件名与插件 Rust `output::EVENT_OUTPUT_AVAILABLE` 逐字一致）
    const subscribed = vi.mocked(context.events.on).mock.calls.map((call) => String(call[0]))
    expect(subscribed).toContain('session:output-available')

    const before = countCalls(context, 'session.output.pull')

    // 非本会话：不拉（同一插件实例可能同时挂多个终端组件）
    context.__emitEvent('session:output-available', { sessionId: 'sess-other' })
    expect(countCalls(context, 'session.output.pull')).toBe(before)

    // 命中：**同步**发起一轮（不等 50 ms 快档轮询）——「空闲期新输出到达」的延迟补偿
    context.__emitEvent('session:output-available', { sessionId: 'sess-1' })
    expect(countCalls(context, 'session.output.pull')).toBe(before + 1)

    // 脏载荷不炸（宿主契约是 JSON：形状不对按「无通知」处理）
    context.__emitEvent('session:output-available', null)
    context.__emitEvent('session:output-available', {})
    expect(countCalls(context, 'session.output.pull')).toBe(before + 1)

    // 卸载释放订阅：此后事件不再触发拉取（不残留回调）
    wrapper!.unmount()
    wrapper = null
    context.__emitEvent('session:output-available', { sessionId: 'sess-1' })
    expect(countCalls(context, 'session.output.pull')).toBe(before + 1)
  })
})
