/**
 * 终端渲染器 / resize 裁决域（插件迁移版）行为契约测试
 *
 * 被测对象：`useTerminalRenderer` / `useTerminalResize`（自宿主拆分产物迁入，
 * logger 与 resize 实现注入化）。
 *
 * 行为契约来源：
 * - 宿主 `src/__tests__/utils/terminalRendererPolicy.test.ts`（decideRenderer /
 *   decideAtlasRefreshFrames 纯函数契约——已随 utils 复制，本文件测域层接线）；
 * - 宿主 `useTerminalRenderer.ts` / `useTerminalResize.ts` 逐字逻辑（WebGL 加载、
 *   context-loss 恢复守卫、resize 拒绝抑制、确认覆盖、±1 漂移抑制）。
 *
 * 硬性门禁（unit-test-discipline）：正例 + 反例 + 边界；断言用状态/调用副作用。
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { ref, shallowRef } from 'vue'

// vue-i18n useI18n 必须在 Vue setup 上下文调用；单测直接调用 composable 无该上下文，
// 桩掉返回恒等 t（与宿主 SessionCenterView 测试的 `i18n: { t: (key) => key }` 同口径）
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

import { useTerminalRenderer } from '../composables/terminal/useTerminalRenderer'
import { useTerminalResize, type ResizeRequester } from '../composables/terminal/useTerminalResize'
import type { TerminalKernelContext } from '../composables/terminal/terminalKernel'

const noopLogger = { warn: vi.fn(), info: vi.fn() }

function makeCtx(overrides?: Partial<TerminalKernelContext>) {
  const ctx: TerminalKernelContext = {
    terminalRef: shallowRef(null as any),
    fitAddonRef: shallowRef(null as any),
    webglAddonRef: shallowRef(null as any),
    terminalHostRef: ref(null),
    isLinux: ref(false),
    isUserScrolling: ref(false),
    bgImageUrl: ref(''),
    getSession: () => null,
    callbacks: {
      getTheme: () => ({}),
      syncTerminalSize: () => {},
      applyResize: () => {},
      applyDprFit: () => {},
      fitAndRefresh: () => {},
      rebuildRenderer: () => {},
      scrollToBottom: vi.fn(),
    },
    ...overrides,
  }
  return ctx
}

/** mock xterm Terminal（renderer/resize 用到的成员） */
function makeTerminal() {
  const terminal = {
    cols: 80,
    rows: 24,
    element: { classList: { add: vi.fn(), remove: vi.fn() }, isConnected: true },
    options: {} as Record<string, unknown>,
    write: vi.fn(),
    clear: vi.fn(),
    refresh: vi.fn(),
    resize: vi.fn(),
    loadAddon: vi.fn(),
    clearTextureAtlas: vi.fn(),
    scrollToBottom: vi.fn(),
    onContextLoss: undefined as unknown,
  }
  return terminal
}

describe('useTerminalRenderer（插件迁移版）', () => {
  beforeEach(() => {
    // happy-dom 的 window.devicePixelRatio 是 undefined；applyDprFit 读它做换算，
    // 桩为 1（常见缩放）——宿主集成测试环境真实浏览器有值，此处只测换算逻辑
    Object.defineProperty(window, 'devicePixelRatio', { value: 1, configurable: true })
    vi.stubGlobal(
      'requestAnimationFrame',
      vi.fn((_cb: FrameRequestCallback) => {
        return 1
      }),
    )
    vi.stubGlobal('cancelAnimationFrame', vi.fn())
  })
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('非 Linux 无背景图 → WebGL 决策（正例）：initRenderer 加载 WebglAddon', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      isLinux: ref(false),
    })
    const renderer = useTerminalRenderer(ctx, noopLogger)
    expect(renderer.rendererDecision.value.useWebgl).toBe(true)
    expect(renderer.rendererDecision.value.allowTransparency).toBe(false)

    renderer.initRenderer(terminal as any)
    expect(terminal.loadAddon).toHaveBeenCalled()
    // WebGL 激活 → 隐藏 DOM 光标
    expect(terminal.element.classList.add).toHaveBeenCalledWith('xterm-hidden-cursor')
  })

  it('Linux 无背景图 → DOM 决策（反例）：不加载 WebGL addon', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      isLinux: ref(true),
    })
    const renderer = useTerminalRenderer(ctx, noopLogger)
    expect(renderer.rendererDecision.value.useWebgl).toBe(false)
    expect(renderer.rendererDecision.value.useDom).toBe(true)

    renderer.initRenderer(terminal as any)
    expect(terminal.loadAddon).not.toHaveBeenCalled()
  })

  it('背景图开启 → DOM + 透明（正例）：allowTransparency 决策', () => {
    const ctx = makeCtx({ bgImageUrl: ref('data:image/png;base64,x') })
    const renderer = useTerminalRenderer(ctx, noopLogger)
    expect(renderer.rendererDecision.value.useDom).toBe(true)
    expect(renderer.rendererDecision.value.allowTransparency).toBe(true)
  })

  it('DPR 感知 fit：容器尺寸与 cell 尺寸换算网格（正例）', () => {
    const terminal = makeTerminal()
    const fitAddon = { fit: vi.fn() }
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef(fitAddon as any),
      terminalHostRef: ref({ clientWidth: 800, clientHeight: 480 } as any),
    })
    // 注入 cell 尺寸（mock 内部 _renderService.dimensions.css.cell）
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: 20 } } } },
    }
    const renderer = useTerminalRenderer(ctx, noopLogger)
    // dpr=1：cols = floor(800/10)=80，rows = floor(480/ceil(20))=24
    renderer.applyDprFit()
    expect(terminal.resize).not.toHaveBeenCalled() // 与当前网格一致，不触发
    // 改容器 → 应触发 resize（±1 漂移抑制外）
    ;(ctx.terminalHostRef.value as any).clientWidth = 1100
    renderer.applyDprFit()
    expect(terminal.resize).toHaveBeenCalledWith(110, 24)
  })

  it('±1 网格漂移抑制（边界）：差 1 列不触发 resize，差 2 列触发', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref({ clientWidth: 800, clientHeight: 480 } as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: 20 } } } },
    }
    const renderer = useTerminalRenderer(ctx, noopLogger)
    // 容器宽 810 → 期望 cols 81（差 1）→ 抑制
    ;(ctx.terminalHostRef.value as any).clientWidth = 810
    renderer.applyDprFit()
    expect(terminal.resize).not.toHaveBeenCalled()
    // 容器宽 830 → 期望 cols 83（差 3）→ 触发
    ;(ctx.terminalHostRef.value as any).clientWidth = 830
    renderer.applyDprFit()
    expect(terminal.resize).toHaveBeenCalledWith(83, 24)
  })
})

describe('useTerminalResize（插件迁移版）', () => {
  let resizeImpl: ReturnType<typeof vi.fn>
  let ctx: TerminalKernelContext
  const session = { id: 's1', name: 'n1', config_id: 'c1', status: 'running', created_at: '' }

  beforeEach(() => {
    resizeImpl = vi.fn()
    ctx = makeCtx({
      getSession: () => session as any,
      terminalRef: shallowRef(makeTerminal() as any),
    })
  })

  it('resize 应用成功清空拒绝记录（正例）：applied 后同尺寸可重发', async () => {
    resizeImpl.mockResolvedValue({ status: 'applied', canonical: { kind: 'desktop' } })
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester)
    await resize.requestResize(100, 30)
    expect(resizeImpl).toHaveBeenCalledWith('s1', 100, 30, false)
    // 拒绝同尺寸后，成功应用应清空拒绝抑制
    resizeImpl.mockResolvedValue({ status: 'needsConfirmation', currentCanonical: { kind: 'mobile', deviceName: 'Pixel' } })
    await resize.requestResize(100, 30)
    expect(resize.showRendererOverrideModal.value).toBe(true)
    expect(resize.rendererOverrideTarget.value?.rendererName).toBe('Pixel')
  })

  it('拒绝后同尺寸抑制（反例）：不再重复弹窗', async () => {
    resizeImpl.mockResolvedValue({ status: 'needsConfirmation', currentCanonical: { kind: 'mobile', deviceName: 'Pixel' } })
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester)
    await resize.requestResize(100, 30)
    resize.cancelRendererOverride()
    await resize.requestResize(100, 30)
    expect(resize.showRendererOverrideModal.value).toBe(false) // 同尺寸被抑制
    expect(resizeImpl).toHaveBeenCalledTimes(1)
  })

  it('确认覆盖 force 重发（正例）：confirm 后 force=true 且弹窗关闭', async () => {
    resizeImpl
      .mockResolvedValueOnce({ status: 'needsConfirmation', currentCanonical: { kind: 'mobile', deviceName: 'Pixel' } })
      .mockResolvedValueOnce({ status: 'applied', canonical: { kind: 'desktop' } })
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester)
    await resize.requestResize(100, 30)
    await resize.confirmRendererOverride()
    expect(resizeImpl).toHaveBeenLastCalledWith('s1', 100, 30, true)
    expect(resize.showRendererOverrideModal.value).toBe(false)
  })

  it('无会话时不发请求（边界）：getSession 为 null 静默返回', async () => {
    const emptyCtx = makeCtx({ getSession: () => null })
    const resize = useTerminalResize(emptyCtx, resizeImpl as unknown as ResizeRequester)
    await resize.requestResize(100, 30)
    expect(resizeImpl).not.toHaveBeenCalled()
  })
})
