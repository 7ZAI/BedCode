/**
 * 终端渲染器域（TerminalView 拆分产物，范式参考桌面端
 * `bedcode-desktop/src/composables/terminal/useTerminalRenderer.ts`）
 *
 * 职责：终端网格测量与构造期预估（computeInitialSize）、DPR 感知 fit
 * （applyDprFit，含 ±1 列漂移钳制）、WebGL 渲染器可选加载与 context-loss 恢复、
 * 字符图集预热补刷（仅 WebGL）、DPR 动态变化监听。
 *
 * 依赖：共享内核 ctx（terminalRef / fitAddonRef / webglAddonRef /
 * xtermContainerRef / gridCalibrated + applyResize 回调）。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { logger } from '@/utils/frontendLogger'
import {
  computeGridSize,
  FONT_FAMILY,
  TERMINAL_LINE_END_MARGIN_COLS,
  TERMINAL_LINE_HEIGHT,
} from '@/utils/terminalMetrics'
import { getXtermScaledDimensions } from '@/utils/terminalDimensions'
import { ATLAS_PREHEAT_DELAY_MS, resolveGridResize } from '@/utils/terminalResizePolicy'
import type { Terminal } from '@xterm/xterm'

/**
 * 渲染器开关：移动端默认 DOM（xterm 内置 canvas）——
 * Android WebView 的 WebGL 常为软件渲染（SwiftShader），双缓冲纹理交换
 * 在 TUI 全屏重绘（opencode/vim 每帧清屏+重绘）时可能闪烁/撕裂；
 * WebGL 不可用（context loss / 初始化失败）时自动回退 DOM 渲染器。
 * 切换为 true 即启用 WebGL addon，仅影响移动端；桌面端不受此开关影响。
 *
 * 默认关闭：addon-webgl 0.19 无公开调优 API（DPR 强制跟随设备、图集页数无上限、
 * 新字符动态光栅化），长会话下 GPU 显存膨胀 + 图集光栅化卡顿 + context loss 重建
 * 是移动端越用越卡的来源之一。内置 DOM 渲染器（canvas 2D 行渲染）对 ~40 行可视区
 * 性能足够，且无上述开销；如需验证可临时切回 true 做 A/B 对比。
 */
export const USE_WEBGL_RENDERER = false

export function useTerminalRenderer(ctx: TerminalKernelContext) {
  // WebGL 字符图集重建后的补刷定时器（仅 WebGL 渲染器；DOM 下不排）
  let atlasPreheatTimer: ReturnType<typeof setTimeout> | null = null
  // DPR 变化监听（matchMedia 递归注册）；屏幕旋转/DPI 变化时窗口尺寸可能不变，
  // ResizeObserver 不触发，须显式重新 fit
  let dprMediaQuery: MediaQueryList | null = null
  let dprChangeHandler: ((this: MediaQueryList, ev: MediaQueryListEvent) => void) | null = null

  /**
   * WebGL 渲染器：动态加载（移动端包体积/启动优化），
   * 处理上下文丢失（丢失时回退 DOM 渲染，1s 后尝试重建）
   */
  async function initWebGL(term: Terminal): Promise<boolean> {
    try {
      const { WebglAddon } = await import('@xterm/addon-webgl')
      const addon = new WebglAddon()
      ctx.webglAddonRef.value = addon
      addon.onContextLoss(() => {
        logger.warn('[TerminalView] WebGL context lost, disposing renderer')
        addon.dispose()
        if (ctx.webglAddonRef.value === addon) ctx.webglAddonRef.value = null
        // 上下文丢失时恢复 DOM 层光标
        term.element?.classList.remove('xterm-hidden-cursor')
        // 延迟 1s 后尝试重新创建 WebGL 渲染器
        setTimeout(() => {
          // 终端已销毁（换会话/卸载）则放弃恢复
          if (ctx.terminalRef.value !== term) return
          try {
            const newAddon = new WebglAddon()
            newAddon.onContextLoss(() => {
              logger.warn('[TerminalView] WebGL context lost again')
              newAddon.dispose()
              if (ctx.webglAddonRef.value === newAddon) ctx.webglAddonRef.value = null
              term.element?.classList.remove('xterm-hidden-cursor')
            })
            term.loadAddon(newAddon)
            ctx.webglAddonRef.value = newAddon
            term.element?.classList.add('xterm-hidden-cursor')
            // WebGL 渲染器 cell 尺寸与 DOM 渲染器不同（桌面端同款处理）：恢复后
            // 重算一次网格，避免渲染器切换留下 ±1 行列漂移
            ctx.callbacks.applyResize()
            logger.info('[TerminalView] WebGL context recovered')
          } catch (e) {
            logger.warn('[TerminalView] WebGL recovery failed, using canvas fallback:', e)
            ctx.webglAddonRef.value = null
          }
        }, 1000)
      })
      term.loadAddon(addon)
      return true
    } catch (e) {
      // WebGL 不可用时回退到 canvas 渲染器
      logger.warn('[TerminalView] WebGL not supported, falling back to DOM renderer:', e)
      ctx.webglAddonRef.value = null
      return false
    }
  }

  /**
   * initTerminal 调用的渲染器初始化入口：按 USE_WEBGL_RENDERER 决定是否加载
   * WebGL addon，并在激活后隐藏 DOM 层光标（避免双光标）。
   */
  async function initRenderer(term: Terminal): Promise<void> {
    if (!USE_WEBGL_RENDERER) return
    const webglActive = await initWebGL(term)
    if (webglActive) {
      term.element?.classList.add('xterm-hidden-cursor')
    }
  }

  /**
   * 创建前预计算终端网格：容器尺寸 ÷ 字体网格（与 FitAddon 一致，仅扣自绘
   * 滚动条预留宽 + 行尾安全余量 1 列，高度不增减）
   */
  function computeInitialSize(fontSize: number): { cols: number; rows: number } {
    const container = ctx.xtermContainerRef.value
    if (!container) return { cols: 80, rows: 24 }
    const grid = computeGridSize(container, fontSize, FONT_FAMILY, TERMINAL_LINE_END_MARGIN_COLS, 0, TERMINAL_LINE_HEIGHT)
    // 字体未就绪（0 尺寸）时回退默认值：发送路径的 80x24 过滤 + fit 后校准兜底
    if (grid.cols <= 0 || grid.rows <= 0) return { cols: 80, rows: 24 }
    return grid
  }

  /** 读取 xterm 实测 cell CSS 尺寸（DPR 感知计算的输入）；不可用时返回 null */
  function measureCellSize(): { width: number; height: number } | null {
    const term = ctx.terminalRef.value
    if (!term) return null
    // addon-fit 0.11 内部即此访问路径（FitAddon.proposeDimensions）；
    // 私有 API 无类型声明，逐级防御，任一环节缺失即回退 fitAddon.fit()
    const core = (term as unknown as { _core?: unknown })._core as
      | { _renderService?: { dimensions?: { css?: { cell?: { width: number; height: number } } } } }
      | undefined
    const cell = core?._renderService?.dimensions?.css?.cell
    if (!cell || cell.width <= 0 || cell.height <= 0) return null
    return { width: cell.width, height: cell.height }
  }

  /**
   * 尺寸适配（初始校准 / 主题切换 / 手动刷新入口）：委托 applyDprFit 统一口径
   * （DPR 感知 + 滚动条预留宽 + 行尾安全余量），不再裸调 FitAddon.fit()——
   * 裸 fit 无行尾余量，行尾字符贴画布右缘被削半。
   * applyDprFit 在字体测量未就绪时降级裸 fit（幂等无操作），由就绪轮询重试。
   * @returns 是否实际发生了尺寸变化
   */
  function fitWithMargin(): boolean {
    const term = ctx.terminalRef.value
    if (!term || !ctx.fitAddonRef.value) return false
    const beforeCols = term.cols
    const beforeRows = term.rows
    applyDprFit()
    if (term.cols !== beforeCols || term.rows !== beforeRows) {
      // 调试验证：记录 fit 导致的尺寸变化轨迹（排查行尾裁切/右侧遮挡）
      logger.debug(`[TerminalView] fit: ${beforeCols}x${beforeRows} -> ${term.cols}x${term.rows}`)
    }
    return term.cols !== beforeCols || term.rows !== beforeRows
  }

  /**
   * DPR 感知 fit：容器 CSS 尺寸 × devicePixelRatio 换算物理像素后计算 cols/rows
   * （行高 ceil、列宽 floor；仅扣自绘滚动条预留宽 + 行尾安全余量 1 列），
   * 替代裸 fitAddon.fit() 的 DPR 不感知计算（Android 高 DPR 下网格更精确、无字模）。
   * 容器/cell 尺寸不可用时优雅降级回 fitAddon.fit()，不炸。
   *
   * 触发 resize 前经 resolveGridResize 解析目标网格：列 ±1 漂移裁掉（保持当前列
   * → xterm 走不到 Buffer._reflow 的整缓冲重排），行任何变化立即生效。键盘避让
   * 驱动的容器高度收缩/还原因此只付「行变化 + 一次可见区重绘」，与历史长度无关。
   */
  function applyDprFit() {
    const term = ctx.terminalRef.value
    if (!term || !ctx.fitAddonRef.value) return
    const container = ctx.xtermContainerRef.value
    const cell = measureCellSize()
    if (!container || container.clientWidth <= 0 || container.clientHeight <= 0 || !cell) {
      ctx.fitAddonRef.value.fit()
      // 降级 fit 后同步变化（不重绘——由调用方统一处理）
      return
    }
    const { cols, rows } = getXtermScaledDimensions({
      containerWidthCss: container.clientWidth,
      containerHeightCss: container.clientHeight,
      cellWidthCss: cell.width,
      cellHeightCss: cell.height,
      devicePixelRatio: window.devicePixelRatio,
      marginCols: TERMINAL_LINE_END_MARGIN_COLS,
    })
    // 走到这里说明容器与字体度量都已就绪：网格可信（与是否发生尺寸变化无关，
    // 供 queueResize 判断「当前 cols/rows 是否仍属构造期兜底值」）
    ctx.gridCalibrated.value = true
    const resolved = resolveGridResize(term.cols, term.rows, cols, rows)
    if (!resolved) return
    // 调试验证：记录漂移钳制后的实际写入网格（列被裁掉时可对照原始 target）
    logger.debug(
      `[TerminalView] applyDprFit: ${term.cols}x${term.rows} -> ${resolved.cols}x${resolved.rows} ` +
        `(target ${cols}x${rows})`,
    )
    term.resize(resolved.cols, resolved.rows)
    scheduleAtlasPreheat()
  }

  /**
   * 字符图集重建后的补刷：resize 后等 async 分片把非 ASCII 字形（中文/box-drawing/
   * emoji）基本光栅化完成，补一次全量重绘，让屏幕一次恢复完整。
   *
   * 仅 WebGL 渲染器需要且仅在有图集时执行（对齐桌面端 scheduleAtlasPreheat 的
   * 「无 webglAddon 直接 no-op」门控）：DOM 渲染器没有字符图集，refresh 只是重建
   * DOM 行（TUI 满屏样式时每行数十 span），由渲染循环自身驱动——此前无条件排一次
   * 700ms 后的整屏重绘，等于每次键盘避让白付一次全屏重绘。
   * 同一次 resize 合并为一次（重置计时器）。
   */
  function scheduleAtlasPreheat() {
    if (!ctx.webglAddonRef.value) return
    if (atlasPreheatTimer) clearTimeout(atlasPreheatTimer)
    atlasPreheatTimer = setTimeout(() => {
      atlasPreheatTimer = null
      // xterm 已销毁（element 已脱离 DOM）则不再重绘
      const term = ctx.terminalRef.value
      if (term && term.element?.isConnected) {
        term.refresh(0, term.rows - 1)
      }
    }, ATLAS_PREHEAT_DELAY_MS)
  }

  /**
   * 监听 DPR 变化并应用新尺寸：matchMedia 只匹配固定 dppx 值，每次命中后按新
   * DPR 重新注册（递归），直到组件卸载。屏幕旋转/DPI 变化时窗口尺寸可能不变，
   * ResizeObserver 不触发，须显式重新 fit（桌面端同路径）。
   */
  function watchDprChanges() {
    if (dprMediaQuery && dprChangeHandler) {
      dprMediaQuery.removeEventListener('change', dprChangeHandler)
    }
    dprChangeHandler = () => {
      // DPI 变化后重新 fit + 条件重绘/同步（窗口尺寸可能未变，ResizeObserver 不触发）
      ctx.callbacks.applyResize()
      watchDprChanges()
    }
    dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
    dprMediaQuery.addEventListener('change', dprChangeHandler)
  }

  /** 组件卸载清理：atlas 预热定时器 + DPR 变化监听 */
  function disposeRenderer() {
    if (atlasPreheatTimer) {
      clearTimeout(atlasPreheatTimer)
      atlasPreheatTimer = null
    }
    if (dprMediaQuery && dprChangeHandler) {
      dprMediaQuery.removeEventListener('change', dprChangeHandler)
      dprMediaQuery = null
      dprChangeHandler = null
    }
  }

  // 注册跨域回调（调用时解析，创建顺序无关）
  ctx.callbacks.applyDprFit = applyDprFit

  return {
    initRenderer,
    computeInitialSize,
    fitWithMargin,
    applyDprFit,
    scheduleAtlasPreheat,
    watchDprChanges,
    disposeRenderer,
  }
}
