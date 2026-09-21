/**
 * 终端渲染器域（插件侧，自宿主 TerminalPreview 拆分产物迁入）
 *
 * 职责：WebGL / DOM 渲染器决策（spec D-1/D-2，route 默认 'B'）、透明度切换时
 * 渲染器重建（context-loss 自动回退 + 1s 恢复）、DPR 感知网格重算、WebGL atlas
 * 预热补刷、Linux 首帧模糊修复、DPR 动态变化监听。
 *
 * 依赖：共享内核 ctx（terminalRef / fitAddonRef / webglAddonRef / terminalHostRef
 * / isLinux / bgImageUrl + getTheme / applyResize 回调）。
 *
 * 与宿主版本差异（票 01b 迁入适配）：logger 改经参数注入（插件不引宿主
 * frontendLogger；调用方传 `console` 或宿主注入的 logger 等价物）。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { computed } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { WebglAddon } from '@xterm/addon-webgl'
import {
  decideRenderer,
  decideAtlasRefreshFrames,
  ATLAS_PREHEAT_FRAME_BUDGET,
} from '../../utils/terminal/terminalRendererPolicy'
import { getXtermScaledDimensions } from '../../utils/terminal/terminalDimensions'
import { shouldApplyGridResize } from '../../utils/terminal/terminalResizePolicy'

// Linux WebKitGTK 终端渲染器选择：WebGL 字形图集在 dpr=1 时按 Math.floor(advance)
// 切格 + LINEAR 采样，文字比原生终端发蒙；DOM(canvas) 渲染器逐字形 fillText，
// 边缘更清晰（对齐 VS Code Linux 默认关 GPU 加速的做法）。置 false 可切回 WebGL 对比。
const LINUX_USE_DOM_RENDERER = true

/** 渲染器日志接口（调用方注入：宿主 logger 或 console） */
export interface TerminalRendererLogger {
  warn: (...args: unknown[]) => void
  info: (...args: unknown[]) => void
}

export function useTerminalRenderer(ctx: TerminalKernelContext, logger: TerminalRendererLogger) {
  // 渲染器重建序列号（spec D-1 竞态守卫）：每次透明度状态切换重建渲染器时递增，
  // context-loss 的 1s 异步恢复回调比对序列号，旧序列号（被更新的重建覆盖的
  // addon 周期）的回调一律丢弃——快速连续切背景图时只有最新一次重建的 addon 存活，
  // 对齐 VS Code _webglAddonLoadId 递增守卫（xtermTerminal.ts:901 / :1039）
  let rendererRebuildSeq = 0
  // WebGL resize 后字符图集重建的补刷（atlas 预热）：rAF 有界迭代补全量重绘，
  // 让 idle 分片异步光栅化的非 ASCII 字形逐步上屏（替代固定延时猜数，见
  // scheduleAtlasPreheat）。0 表示当前无迭代在跑
  let atlasPreheatRaf = 0
  // DPR 变化监听（跨屏拖动 / 系统缩放变化）：matchMedia 递归注册，卸载时移除
  let dprMediaQuery: MediaQueryList | null = null
  let dprChangeHandler: ((e: MediaQueryListEvent) => void) | null = null

  // 渲染器/透明度决策（spec D-1 / D-2）：背景图开启时强制 DOM 渲染器（透明天然
  // 正确，一次切断 alpha 帧缓冲 + 无条件透明 viewport 两条残影通路）；Linux 无画布
  // 场景保持既有 LINUX_USE_DOM_RENDERER 事实选择；其余场景 WebGL（高吞吐）。
  // isLinux 在 onMounted 中 await initPlatform() 后才确定，而 initTerminal 在之后
  // 调用，故此 computed 首次被消费时输入已就绪
  const rendererDecision = computed(() =>
    decideRenderer({
      isLinux: ctx.isLinux.value,
      hasBackgroundImage: !!ctx.bgImageUrl.value,
      linuxUseDomRenderer: LINUX_USE_DOM_RENDERER,
    }),
  )

  /**
   * WebGL 渲染器：加载并处理上下文丢失（丢失时回退 DOM 渲染，1s 后尝试重建）。
   *
   * seq：initWebGL 调用时刻的渲染器重建序列号（rebuildRenderer / context-loss
   * 恢复之间的竞态守卫）。恢复回调触发时若序列号已变（期间经历过渲染器重建），
   * 说明本次恢复属于已被覆盖的旧 addon 周期，直接丢弃，避免旧 addon 的异步
   * 恢复覆盖新 addon（对齐 VS Code _webglAddonLoadId 守卫）。
   */
  function initWebGL(term: Terminal, seq: number): boolean {
    try {
      const addon = new WebglAddon()
      ctx.webglAddonRef.value = addon
      addon.onContextLoss(() => {
        logger.warn('[TerminalPreview] WebGL context lost, attempting recovery')
        ctx.webglAddonRef.value?.dispose()
        ctx.webglAddonRef.value = null
        // 上下文丢失时恢复 DOM 光标
        term.element?.classList.remove('xterm-hidden-cursor')
        // 延迟 1s 后尝试重新创建 WebGL 渲染器
        setTimeout(() => {
          // 恢复回调的竞态守卫：期间若有新 addon 已激活（webglAddon 非空）、
          // 渲染器被重建过（序列号漂移）或终端已销毁，本次恢复均属残留周期，
          // 直接丢弃，避免覆盖新渲染器的上下文
          if (!term || ctx.webglAddonRef.value || seq !== rendererRebuildSeq) return
          try {
            const newAddon = new WebglAddon()
            newAddon.onContextLoss(() => {
              logger.warn('[TerminalPreview] WebGL context lost again')
              newAddon.dispose()
              if (ctx.webglAddonRef.value === newAddon) ctx.webglAddonRef.value = null
              term.element?.classList.remove('xterm-hidden-cursor')
            })
            term.loadAddon(newAddon)
            ctx.webglAddonRef.value = newAddon
            // 恢复后重新隐藏 DOM 光标
            term.element?.classList.add('xterm-hidden-cursor')
            // WebGL 渲染器 cell 尺寸与 DOM 渲染器不同（VS Code 在 webgl 加载后同样
            // 触发刷新重测网格），恢复后重算一次避免行列差 1 的漂移
            ctx.callbacks.applyResize()
            logger.info('[TerminalPreview] WebGL context recovered')
          } catch (e) {
            logger.warn('[TerminalPreview] WebGL recovery failed, using canvas fallback:', e)
            ctx.webglAddonRef.value = null
          }
        }, 1000)
      })
      term.loadAddon(addon)
      return true
    } catch (e) {
      logger.warn('[TerminalPreview] WebGL not supported:', e)
      ctx.webglAddonRef.value = null
      return false
    }
  }

  /**
   * 透明度状态（背景图开/关）切换时重建渲染器（spec D-1）：
   * dispose 现有 WebGL addon → 重设 allowTransparency → 按 decideRenderer 结果
   * 重新 initWebGL 或保持 DOM → 重设 theme → 重算尺寸 → 全量重绘。
   *
   * 为什么必须重建而非只改 options：addon-webgl 0.19.0 中 _setTransparency 是零
   * 调用的死代码，渲染层 alpha 标志与 canvas 的 { alpha } 属性在 getContext 之后
   * 不可变。先 dispose 再 loadAddon 避免泄漏 WebGL context（对齐 VS Code
   * _enableWebglRenderer "Dispose of existing addon before creating a new one
   * to avoid leaking WebGL contexts"）。WebGL 与 DOM 渲染器 cell 尺寸不同，
   * 重建后必须重算行列（context-loss 恢复路径同款处理）；重建期间同步
   * xterm-hidden-cursor 类的加/删，避免双光标或光标消失。
   *
   * DOM 渲染器分支无 addon 可重建：xterm 6.0 的 DOM(canvas) 渲染器透明由 DOM 层
   * 实现（canvas 恒为 alpha），重设选项 + theme 即生效，此处走同路径保证两条
   * 残影通路（alpha 帧缓冲 / 无条件透明 viewport）一致的收敛状态。
   */
  function rebuildRenderer() {
    const terminal = ctx.terminalRef.value
    if (!terminal) return
    // 重建序列号递增：从这一刻起，previous 周期（含其 1s 恢复回调）作废
    const seq = ++rendererRebuildSeq
    const decision = rendererDecision.value
    const webglAddon = ctx.webglAddonRef.value
    if (webglAddon) {
      webglAddon.dispose()
      ctx.webglAddonRef.value = null
    }
    // 透明度是渲染器构造时读一次的选项，必须在重建渲染器之前重设
    terminal.options.allowTransparency = decision.allowTransparency
    if (decision.useWebgl) {
      const loaded = initWebGL(terminal, seq)
      // WebGL 激活后隐藏 DOM 层光标，避免双光标（与 initTerminal 同款处理）；
      // 加载失败回退 DOM 渲染器时恢复 DOM 光标
      terminal.element?.classList.toggle('xterm-hidden-cursor', loaded)
    } else {
      // DOM 渲染器：DOM 层光标可见（不再由 WebGL 层替代）
      terminal.element?.classList.remove('xterm-hidden-cursor')
    }
    terminal.options.theme = ctx.callbacks.getTheme()
    ctx.callbacks.applyResize()
    // 全量重绘：applyResize 仅在 cols/rows 变化时重绘，而重建后必须无条件整屏
    // 重绘一次，清除旧渲染模式（alpha toggle / 渲染器切换）的残留帧
    terminal.refresh(0, terminal.rows - 1)
  }

  /**
   * initTerminal 调用的渲染器初始化入口：按决策加载 WebGL（或保持 DOM）并同步
   * DOM 层光标状态。DOM 渲染器下 webglAddon 为 null，后续 clearTextureAtlas /
   * xterm-hidden-cursor 分支天然跳过。
   */
  function initRenderer(term: Terminal) {
    // 传入当前重建序列号，作为 context-loss 恢复回调的竞态基线
    if (rendererDecision.value.useWebgl) {
      initWebGL(term, rendererRebuildSeq)
    }
    // WebGL 渲染器激活后，隐藏 DOM 层光标避免双光标问题
    // 只隐藏 DOM 层，保留 WebGL 层光标（WebGL 光标更流畅且不会出现双光标）
    if (ctx.webglAddonRef.value) {
      term.element?.classList.add('xterm-hidden-cursor')
    }
  }

  /** 读取 xterm 实测 cell CSS 尺寸（DPR 感知计算的输入）；不可用时返回 null */
  function measureCellSize(): { width: number; height: number } | null {
    const terminal = ctx.terminalRef.value
    if (!terminal) return null
    // SAFETY: xterm 未导出 _core/_renderService/dimensions 类型，仅按已知内部
    // 结构（addon-fit 0.11 同源访问路径）逐级类型断言；每级均有可选链防御，
    // 任何缺失/形状变化都返回 null 而非抛错，保证契约不因 xterm 内部变动而崩
    const core = (terminal as unknown as { _core?: unknown })._core as
      | {
          _renderService?: { dimensions?: { css?: { cell?: { width: number; height: number } } } }
        }
      | undefined
    const cell = core?._renderService?.dimensions?.css?.cell
    if (!cell || cell.width <= 0 || cell.height <= 0) return null
    return { width: cell.width, height: cell.height }
  }

  /**
   * DPR 感知 fit：容器 CSS 尺寸 × devicePixelRatio 换算设备像素后计算 cols/rows
   * （行高 ceil、列宽 floor），替换 fitAddon.fit() 的裸 DPR 不感知计算。
   * 容器/cell 尺寸不可用时优雅降级回 fitAddon.fit()，不炸。
   *
   * 触发 resize 前经 shouldApplyGridResize 抑制 ±1 列/行测量漂移：applyDprFit 的
   * 尺寸口径（clientWidth × 渲染器 css.cell）与当前网格存在 ±1~2 列系统性偏差，
   * 每次精确比较都 resize 会让 WebGL 在点"刷新"时重建整个字符图集（非 ASCII
   * 字形按 idle 分片异步重新光栅化），表现为前几次刷新格式乱、图集预热完才正常。
   * 真实 resize（拖窗/字号变化）后 scheduleAtlasPreheat 补刷收尾。
   */
  function applyDprFit() {
    const terminal = ctx.terminalRef.value
    const fitAddon = ctx.fitAddonRef.value
    if (!terminal || !fitAddon) return
    const host = ctx.terminalHostRef.value
    const cell = measureCellSize()
    if (!host || host.clientWidth <= 0 || host.clientHeight <= 0 || !cell) {
      fitAddon.fit()
      return
    }
    const { cols, rows } = getXtermScaledDimensions({
      containerWidthCss: host.clientWidth,
      containerHeightCss: host.clientHeight,
      cellWidthCss: cell.width,
      cellHeightCss: cell.height,
      devicePixelRatio: window.devicePixelRatio,
    })
    // 与 FitAddon.fit() 一致：尺寸不变不动（避免无谓 resize 事件），
    // 变化时经 ±1 漂移抑制，仅在真实变化时 resize
    if (terminal.cols !== cols || terminal.rows !== rows) {
      if (!shouldApplyGridResize(terminal.cols, terminal.rows, cols, rows)) {
        return
      }
      terminal.resize(cols, rows)
      scheduleAtlasPreheat()
    }
  }

  /**
   * WebGL atlas 预热补刷：resize 重建字符图集后，非 ASCII 字形（中文/box-drawing/
   * emoji）按 requestIdleCallback 分片异步光栅化（warmUp 只预热 ASCII 33-126）。
   *
   * 为什么用 rAF 有界迭代而非固定延时：atlas 页合并时 beginFrame() 会触发全量重绘，
   * 迭代刷新能自然跟上光栅化进度（spec D-4，替代 ATLAS_PREHEAT_DELAY_MS=700 猜数——
   * 低配机不够、高性能机浪费）。每帧补一次全量 refresh，直到帧预算 0 或元素脱离 DOM。
   *
   * 无 atlas 的场景（DOM 渲染器：Linux / 背景图强制 DOM）直接 no-op——refresh 在那里
   * 只是重建 DOM 行，由渲染循环自身驱动，无需预热。同窗口多次 resize 不重复启动。
   * rAF 在窗口最小化时暂停，恢复可见后继续跑完剩余预算是可接受语义（预算在隐藏期
   * 不消耗，可见后仍会补完）。
   */
  function scheduleAtlasPreheat() {
    const terminal = ctx.terminalRef.value
    if (!terminal || !ctx.webglAddonRef.value || atlasPreheatRaf !== 0) return
    let budget = ATLAS_PREHEAT_FRAME_BUDGET
    const step = () => {
      atlasPreheatRaf = 0
      // xterm 已销毁（element 已脱离 DOM）则不再重绘
      if (!terminal || !terminal.element?.isConnected) return
      terminal.refresh(0, terminal.rows - 1)
      // 帧预算递减：decideAtlasRefreshFrames 返回 0 表示停止（也防御负数/残留回调）
      const next = decideAtlasRefreshFrames(budget)
      if (next > 0) {
        budget = next
        atlasPreheatRaf = requestAnimationFrame(step)
      }
    }
    atlasPreheatRaf = requestAnimationFrame(step)
  }

  /**
   * Linux 首帧模糊修复：WebKitGTK 的 document.fonts 不追踪系统字体（DejaVu 等经
   * fontconfig 解析），fonts.ready 会提前 resolve——首次 measure 可能仍用回退字体/
   * 旧指标，导致字符尺寸按错指标光栅化 → 整屏文字发蒙。挂载并跑完首帧后延迟重测
   * + 全量重绘一次，消除首帧模糊残留。DOM 渲染器重绘即重建行；WebGL 渲染器还会
   * 重建字形图集（clearTextureAtlas 在 DOM 渲染器下为 no-op）。
   */
  function scheduleInitialFontRemeasure() {
    if (!ctx.isLinux.value) return
    setTimeout(() => {
      const terminal = ctx.terminalRef.value
      if (!terminal || !terminal.element?.isConnected) return
      // SAFETY: xterm 未导出 _charSizeService 类型，仅按已知内部结构断言；
      // 可选链 + 空值跳过保证内部变动时不抛错（与 measureCellSize 同源防御策略）
      const core = (terminal as unknown as {
        _core?: { _charSizeService?: { measure?: () => void } }
      })._core
      core?._charSizeService?.measure?.()
      // WebGL 图集按新指标重建；DOM 渲染器下 clearTextureAtlas 为 no-op
      if (ctx.webglAddonRef.value) {
        terminal.clearTextureAtlas()
      }
      terminal.refresh(0, terminal.rows - 1)
    }, 300)
  }

  /**
   * 监听 DPR 变化并应用新尺寸：matchMedia 只匹配固定 dppx 值，
   * 每次命中后按新 DPR 重新注册（递归），直到组件卸载
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

  /**
   * fit 后强制全量重绘：WebGL 渲染器在容器尺寸变化（全屏/滚动触发布局微变/字号变化）
   * 后不会自动重绘可见区域，旧纹理残留导致字符错位与格式错乱，故 fit 后立即
   * refresh 整屏修正。覆盖 resize 与字号变化两类重绘场景（刷新按钮不经过此处，
   * 见组件 refreshTerminal 的"为什么不 fit"说明）。
   */
  function fitAndRefresh() {
    const fitAddon = ctx.fitAddonRef.value
    const terminal = ctx.terminalRef.value
    if (!fitAddon || !terminal) return
    applyDprFit()
    terminal.refresh(0, terminal.rows - 1)
  }

  /** 组件卸载清理：取消 atlas 预热迭代与 DPR 监听 */
  function disposeRenderer() {
    if (atlasPreheatRaf) {
      cancelAnimationFrame(atlasPreheatRaf)
      atlasPreheatRaf = 0
    }
    if (dprMediaQuery && dprChangeHandler) {
      dprMediaQuery.removeEventListener('change', dprChangeHandler)
      dprMediaQuery = null
      dprChangeHandler = null
    }
  }

  // 注册跨域回调（调用时解析，创建顺序无关）
  ctx.callbacks.applyDprFit = applyDprFit
  ctx.callbacks.fitAndRefresh = fitAndRefresh
  ctx.callbacks.rebuildRenderer = rebuildRenderer

  return {
    rendererDecision,
    initRenderer,
    rebuildRenderer,
    applyDprFit,
    fitAndRefresh,
    scheduleInitialFontRemeasure,
    watchDprChanges,
    disposeRenderer,
  }
}
