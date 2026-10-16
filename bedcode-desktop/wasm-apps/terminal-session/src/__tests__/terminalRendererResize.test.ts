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

import {
  useTerminalRenderer,
  VIEWPORT_CONVERGE_MAX_STEPS,
} from '../composables/terminal/useTerminalRenderer'
import { useTerminalResize, type ResizeRequester } from '../composables/terminal/useTerminalResize'
import type { TerminalKernelContext } from '../composables/terminal/terminalKernel'

const noopLogger = { warn: vi.fn(), info: vi.fn() }

/** 恒等翻译桩（与插件 context.i18n.t 同形；用例断言的是调用副作用，不看文案） */
const identityT = (key: string) => key

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
      settleViewport: () => {},
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

  it('Linux 首帧字体重测后重算网格（回归锁）：按修正后的 cell 重算行列', () => {
    // 现场：WebKitGTK 首帧用回退字体指标测出偏小 cell → 网格行数偏多（84 行装不下
    // 只容 25 行的容器）→ 内容高于容器、视口"滚动到底"→ 第一行被顶部工具条裁掉。
    // 旧实现重测后只 refresh，网格永不修正；本锁钉住「重测 → 重算网格」因果链。
    vi.useFakeTimers()
    try {
      const terminal = makeTerminal()
      terminal.cols = 80
      terminal.rows = 84
      const measure = vi.fn()
      ;(terminal as any)._core = {
        _charSizeService: { measure },
        // 重测后的真实指标（cell 8×16，dpr=1 → cols=100 / rows=25）
        _renderService: { dimensions: { css: { cell: { width: 8, height: 16 } } } },
      }
      const ctx = makeCtx({
        terminalRef: shallowRef(terminal as any),
        fitAddonRef: shallowRef({ fit: vi.fn() } as any),
        terminalHostRef: ref({ clientWidth: 800, clientHeight: 400 } as any),
        isLinux: ref(true),
      })
      const renderer = useTerminalRenderer(ctx, noopLogger)

      renderer.scheduleInitialFontRemeasure()
      // 未到延迟：不 measure、不改网格
      expect(measure).not.toHaveBeenCalled()
      expect(terminal.resize).not.toHaveBeenCalled()

      vi.advanceTimersByTime(300)

      expect(measure).toHaveBeenCalledTimes(1)
      expect(terminal.resize).toHaveBeenCalledWith(100, 25)
    } finally {
      vi.useRealTimers()
    }
  })

  it('非 Linux 不做首帧重测（反例）：不 measure、不碰网格', () => {
    vi.useFakeTimers()
    try {
      const terminal = makeTerminal()
      const measure = vi.fn()
      ;(terminal as any)._core = {
        _charSizeService: { measure },
        _renderService: { dimensions: { css: { cell: { width: 8, height: 16 } } } },
      }
      const ctx = makeCtx({
        terminalRef: shallowRef(terminal as any),
        fitAddonRef: shallowRef({ fit: vi.fn() } as any),
        terminalHostRef: ref({ clientWidth: 800, clientHeight: 400 } as any),
        isLinux: ref(false),
      })
      const renderer = useTerminalRenderer(ctx, noopLogger)

      renderer.scheduleInitialFontRemeasure()
      vi.advanceTimersByTime(300)

      expect(measure).not.toHaveBeenCalled()
      expect(terminal.resize).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it('终端已卸载（element 断开）不触发重测（边界）', () => {
    vi.useFakeTimers()
    try {
      const terminal = makeTerminal()
      terminal.element = {
        classList: { add: vi.fn(), remove: vi.fn() },
        isConnected: false,
      } as any
      const measure = vi.fn()
      ;(terminal as any)._core = { _charSizeService: { measure } }
      const ctx = makeCtx({
        terminalRef: shallowRef(terminal as any),
        fitAddonRef: shallowRef({ fit: vi.fn() } as any),
        isLinux: ref(true),
      })
      const renderer = useTerminalRenderer(ctx, noopLogger)

      renderer.scheduleInitialFontRemeasure()
      vi.advanceTimersByTime(300)

      expect(measure).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  // ==================== 视口—容器高度一致性收敛（顶行被裁加固） ====================
  //
  // 契约：视口在底部且实测画布（.xterm-screen）高于容器超过 1px → 逐行削减 rows
  //（最多 VIEWPORT_CONVERGE_MAX_STEPS 行）；用户上滚看历史 / 无溢出 / rows=1 时不干预。

  // 诊断日志断言需要干净的记录（logger 为模块级共享桩）
  beforeEach(() => {
    noopLogger.warn.mockClear()
    noopLogger.info.mockClear()
  })

  /** 视觉行高（px）：与 _renderService cell 高一致，模拟真实 xterm 布局 */
  const RENDERED_CELL_HEIGHT = 20

  /** 最小 host 桩：applyDprFit 读 clientWidth/clientHeight，收敛读 clientHeight */
  function makeHost(clientWidth = 800, clientHeight = 480) {
    return {
      clientWidth,
      clientHeight,
      getBoundingClientRect: () => ({ top: 0, bottom: clientHeight }),
    }
  }

  /**
   * 挂载 .xterm-screen 测量桩：高度 = rows × RENDERED_CELL_HEIGHT + overflowPx，
   * top = -offsetTop（host rect.top 恒 0 → offsetTop = 内容上移量，模拟非整行滚动残留）。
   * resize 后 rows 变化会反映到高度（与真实 xterm 一致，供逐行削减收敛终止）。
   */
  function attachScreenMock(terminal: any, overflowPx: number, offsetTop = 0) {
    const screen = {
      getBoundingClientRect: () => {
        const height = terminal.rows * RENDERED_CELL_HEIGHT + overflowPx
        return { top: -offsetTop, bottom: -offsetTop + height, height }
      },
    }
    terminal.element.querySelector = vi.fn((sel: string) =>
      sel === '.xterm-screen' ? screen : null,
    )
  }

  /** resize 桩：真实更新网格（收敛循环依赖 resize 后行数变化） */
  function attachResizeMock(terminal: any) {
    terminal.resize = vi.fn((cols: number, rows: number) => {
      terminal.cols = cols
      terminal.rows = rows
    })
  }

  it('视口在底部 + 画布高于容器 → 削减 1 行收敛（正例，顶行被裁修复）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 3) // 亚像素累积溢出 3px（无滚动残留）
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } } // 停在底部

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    // 目标网格 80×24 与当前一致（不触发网格 resize）→ 只发生收敛削减
    expect(terminal.resize).toHaveBeenCalledTimes(1)
    expect(terminal.resize).toHaveBeenCalledWith(80, 23)
    // 削行后重新钉底（避免削行引入新的滚动偏移）
    expect(terminal.scrollToBottom).toHaveBeenCalled()
    // 诊断日志（真机复验定位用）仅在收敛发生时输出
    expect(noopLogger.warn).toHaveBeenCalled()
  })

  it('内容上沿被裁（像素级滚动残留）→ 钉回整行底部且不削行（二轮修正正例）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    // 内容上移 6px（非整行 scrollTop 残留）但画布高度不超容
    attachScreenMock(terminal, 0, 6)
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } }

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    // 只做滚动位置校正：钉回整行底部，绝不能削行（削行会把上移变成底部黑缝）
    expect(terminal.scrollToBottom).toHaveBeenCalledTimes(1)
    expect(terminal.resize).not.toHaveBeenCalled()
    expect(noopLogger.warn).toHaveBeenCalled()
  })

  it('内容上沿被裁但用户已上滚 → 不校正（反例：滚动位置属用户意图）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 0, 6)
    terminal.buffer = { active: { viewportY: 90, baseY: 100 } } // 离开底部

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    expect(terminal.scrollToBottom).not.toHaveBeenCalled()
    expect(terminal.resize).not.toHaveBeenCalled()
    expect(noopLogger.warn).not.toHaveBeenCalled()
  })

  it('settleViewport 节流：窗口内重复调用只校正一次，force 绕过（输出写入防抖）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 0, 6) // 上沿残留 6px
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } }
    const renderer = useTerminalRenderer(ctx, noopLogger)

    // 输出写入路径（onWriteParsed）节流调用：首次生效，窗口内重复调用被节流
    expect(renderer.settleViewport()).toBe(true)
    expect(renderer.settleViewport()).toBe(false)
    expect(terminal.scrollToBottom).toHaveBeenCalledTimes(1)
    // fit 路径（convergeViewportOverflow）用 force 绕过节流，保证收敛时机必达
    expect(renderer.settleViewport(true)).toBe(true)
    expect(terminal.scrollToBottom).toHaveBeenCalledTimes(2)
  })

  it('画布不高于容器 → 不削减（反例）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 0) // 24×20 = 480 = 容器高，无溢出
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } }

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    expect(terminal.resize).not.toHaveBeenCalled()
    expect(noopLogger.warn).not.toHaveBeenCalled()
  })

  it('用户已上滚查看历史 → 不干预（边界，内容上移属正常滚动语义）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 3)
    terminal.buffer = { active: { viewportY: 90, baseY: 100 } } // 离开底部

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    expect(terminal.resize).not.toHaveBeenCalled()
    expect(noopLogger.warn).not.toHaveBeenCalled()
  })

  it('rows=1 时不再削减（边界，不得跌破 1 行）', () => {
    const terminal = makeTerminal()
    terminal.cols = 80
    terminal.rows = 1
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost(800, RENDERED_CELL_HEIGHT) as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    attachScreenMock(terminal, 3)
    terminal.buffer = { active: { viewportY: 0, baseY: 0 } }

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    expect(terminal.resize).not.toHaveBeenCalled()
  })

  it('持续溢出时单次收敛最多削减 2 行（防抖上限）', () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }
    attachResizeMock(terminal)
    // 溢出量与行数无关（极端：画布恒高 600px）→ 每轮都溢出，只能靠步数上限收口
    const screen = { getBoundingClientRect: () => ({ height: 600 }) }
    terminal.element.querySelector = vi.fn((sel: string) =>
      sel === '.xterm-screen' ? screen : null,
    )
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } }

    const renderer = useTerminalRenderer(ctx, noopLogger)
    renderer.applyDprFit()

    expect(terminal.resize).toHaveBeenCalledTimes(VIEWPORT_CONVERGE_MAX_STEPS)
    expect(terminal.resize).toHaveBeenLastCalledWith(80, 22)
  })

  it('element 无 querySelector（环境降级）→ 不抛错不干预（异常边界）', () => {
    const terminal = makeTerminal() // 默认 element 只有 classList/isConnected
    attachResizeMock(terminal)
    terminal.buffer = { active: { viewportY: 100, baseY: 100 } }
    const ctx = makeCtx({
      terminalRef: shallowRef(terminal as any),
      fitAddonRef: shallowRef({ fit: vi.fn() } as any),
      terminalHostRef: ref(makeHost() as any),
    })
    ;(terminal as any)._core = {
      _renderService: { dimensions: { css: { cell: { width: 10, height: RENDERED_CELL_HEIGHT } } } },
    }

    const renderer = useTerminalRenderer(ctx, noopLogger)
    expect(() => renderer.applyDprFit()).not.toThrow()
    expect(terminal.resize).not.toHaveBeenCalled()
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
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester, identityT)
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
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester, identityT)
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
    const resize = useTerminalResize(ctx, resizeImpl as unknown as ResizeRequester, identityT)
    await resize.requestResize(100, 30)
    await resize.confirmRendererOverride()
    expect(resizeImpl).toHaveBeenLastCalledWith('s1', 100, 30, true)
    expect(resize.showRendererOverrideModal.value).toBe(false)
  })

  it('无会话时不发请求（边界）：getSession 为 null 静默返回', async () => {
    const emptyCtx = makeCtx({ getSession: () => null })
    const resize = useTerminalResize(emptyCtx, resizeImpl as unknown as ResizeRequester, identityT)
    await resize.requestResize(100, 30)
    expect(resizeImpl).not.toHaveBeenCalled()
  })
})
