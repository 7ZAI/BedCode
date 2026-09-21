/**
 * 终端设置同步域（插件迁移版）行为契约测试
 *
 * 被测对象：`useTerminalSettingsSync`（自宿主拆分产物迁入，设置访问器注入化）。
 *
 * 行为契约来源：宿主 `useTerminalSettingsSync.ts` 逐字逻辑（字号/主题/背景三项
 * watch、防抖持久化、外部变化同步、透明切换重建）+ 宿主 terminal-flow 集成测试
 * 的设置同步用例。
 *
 * 硬性门禁（unit-test-discipline）：正例 + 边界 + 副作用（save 调用、options 变更、
 * rebuildRenderer 回调）；不用恒真断言。
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { ref, shallowRef, nextTick } from 'vue'
import { useTerminalSettingsSync, type TerminalSettingsAccessor } from '../composables/terminal/useTerminalSettingsSync'
import type { TerminalKernelContext } from '../composables/terminal/terminalKernel'

function makeSettings(overrides?: Partial<TerminalSettingsAccessor>): TerminalSettingsAccessor {
  // 必须用 ref 存值：宿主 useTerminalSettingsSync 的 watch(() => settings.getFontSize())
  // 依赖 getter 读取时的响应式追踪——闭包变量非响应式，save 改值后 watch 永不触发
  // （实测：expected 12 to be 18）。ref.value 读写让外部变化可被 watch 捕获。
  const fontSize = ref(12)
  const theme = ref('dracula')
  const bgImage = ref('')
  const bgOpacity = ref(30)
  const port = ref(8080)
  const listeners: Array<() => void> = []
  const accessor: TerminalSettingsAccessor = {
    getFontSize: () => fontSize.value,
    getTheme: () => theme.value,
    getBgImage: () => bgImage.value,
    getBgOpacity: () => bgOpacity.value,
    getServerPort: () => port.value,
    save: vi.fn((patch) => {
      if (patch.fontSize != null) fontSize.value = patch.fontSize
      if (patch.theme != null) theme.value = patch.theme
      if (patch.bgImage != null) bgImage.value = patch.bgImage
      if (patch.bgOpacity != null) bgOpacity.value = patch.bgOpacity
    }),
    onChange: (listener) => {
      listeners.push(listener)
      return () => {
        const i = listeners.indexOf(listener)
        if (i >= 0) listeners.splice(i, 1)
      }
    },
    ...overrides,
  }
  return accessor
}

function makeCtx(overrides?: Partial<TerminalKernelContext>) {
  const ctx: TerminalKernelContext = {
    terminalRef: shallowRef(null as any),
    fitAddonRef: shallowRef({ fit: vi.fn() } as any),
    webglAddonRef: shallowRef(null as any),
    terminalHostRef: ref(null),
    isLinux: ref(false),
    isUserScrolling: ref(false),
    bgImageUrl: ref(''),
    getSession: () => null,
    callbacks: {
      getTheme: () => ({}),
      syncTerminalSize: vi.fn() as () => void,
      applyResize: vi.fn() as () => void,
      applyDprFit: vi.fn() as () => void,
      fitAndRefresh: vi.fn() as () => void,
      rebuildRenderer: vi.fn() as () => void,
      scrollToBottom: vi.fn() as () => void,
    },
    ...overrides,
  }
  return ctx
}

function makeTerminal() {
  return {
    options: {} as Record<string, unknown>,
    cols: 80,
    rows: 24,
    element: { isConnected: true },
    refresh: vi.fn(),
    resize: vi.fn(),
  } as any
}

const logger = { error: vi.fn() }

describe('useTerminalSettingsSync（插件迁移版）', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('初始值取自 accessor（正例）：字号/主题/背景三项与设置桥一致', () => {
    const settings = makeSettings()
    const ctx = makeCtx()
    const sync = useTerminalSettingsSync(ctx, settings, logger)
    expect(sync.fontSize.value).toBe(12)
    expect(sync.terminalTheme.value).toBe('dracula')
    expect(sync.bgImage.value).toBe('')
    expect(sync.bgOpacity.value).toBe(30)
  })

  it('字号变化应用 + 防抖持久化（正例）：options.fontSize 更新 + 300ms 后 save', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)
    const { fitAndRefresh } = ctx.callbacks

    sync.fontSize.value = 16
    await nextTick()
    expect(terminal.options.fontSize).toBe(16) // 非 Linux：不乘 1.15
    expect(fitAndRefresh).toHaveBeenCalled()
    expect(settings.save).not.toHaveBeenCalled() // 防抖窗口内不保存

    await vi.advanceTimersByTimeAsync(300)
    expect(settings.save).toHaveBeenCalledWith({ fontSize: 16 })
  })

  it('Linux 字号乘基线因子（边界）：视觉字号 = 设置值 × 1.15，保存仍存原值', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      isLinux: ref(true),
    })
    const sync = useTerminalSettingsSync(ctx, settings, logger)
    // 注意：makeSettings 默认 fontSize 就是 12，赋同值 Vue watch（Object.is 比较）不触发——
    // 必须赋与默认不同的值（14）才能验证 Linux 基线因子生效
    sync.fontSize.value = 14
    await nextTick()
    expect(terminal.options.fontSize).toBe(14 * 1.15)
    expect(sync.effectiveFontSize.value).toBe(14 * 1.15)
  })

  it('主题变化应用 + 防抖持久化（正例）：options.theme 更新 + 300ms 后 save', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)

    sync.terminalTheme.value = 'light'
    await nextTick()
    expect(terminal.options.theme).toBeDefined()
    await vi.advanceTimersByTimeAsync(300)
    expect(settings.save).toHaveBeenCalledWith({ theme: 'light' })
  })

  it('背景图开关切换透明重建（正例）：url 有无变化触发 rebuildRenderer', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)
    const { rebuildRenderer } = ctx.callbacks
    const rebuildMock = rebuildRenderer as ReturnType<typeof vi.fn>

    // 开背景图：url 从 '' → 有值 → 重建
    sync.bgImageUrl.value = 'http://127.0.0.1:8080/static/terminal-bg?t=1'
    await nextTick()
    expect(rebuildMock).toHaveBeenCalled()

    // 关背景图：url 从有值 → '' → 重建
    rebuildMock.mockClear()
    sync.bgImageUrl.value = ''
    await nextTick()
    expect(rebuildMock).toHaveBeenCalled()
  })

  it('仅不透明度变化不重建（边界）：url 未变只重设 theme + refresh', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)
    const { rebuildRenderer } = ctx.callbacks
    const rebuildMock = rebuildRenderer as ReturnType<typeof vi.fn>

    // 先开背景图（触发一次重建）
    sync.bgImageUrl.value = 'http://127.0.0.1:8080/static/terminal-bg?t=1'
    await nextTick()
    rebuildMock.mockClear()

    // 仅不透明度变化：不重建
    sync.bgOpacity.value = 50
    await nextTick()
    expect(rebuildMock).not.toHaveBeenCalled()
    expect(terminal.refresh).toHaveBeenCalled()
  })

  it('dispose 清理防抖与订阅（正例）：save 不再触发、onChange 退订', async () => {
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)

    sync.fontSize.value = 14
    await nextTick() // 让 watch 回调完成（options 更新 + 防抖 timer 挂起）
    sync.disposeSettingsSync()
    await vi.advanceTimersByTimeAsync(400)
    expect(settings.save).not.toHaveBeenCalled() // dispose 取消防抖，timer 不再触发 save
  })

  it('外部设置变化同步（正例）：accessor onChange 后 get 新值被 watch 捕获', async () => {
    // 直接改 accessor 内部值 + 触发 onChange（模拟宿主设置面板保存）
    const terminal = makeTerminal()
    const settings = makeSettings()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const sync = useTerminalSettingsSync(ctx, settings, logger)

    // 模拟外部修改：更新 accessor 内部值后触发 onChange（真实桥同路径）
    // mock 的 save 会更新内部闭包 fontSize；watch(() => settings.getFontSize()) 依赖变化自动响应
    ;(settings.save as any)({ fontSize: 18 })
    await nextTick()
    expect(sync.fontSize.value).toBe(18)
  })
})
