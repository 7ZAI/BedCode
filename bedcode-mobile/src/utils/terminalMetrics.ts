/**
 * 终端网格测量工具（移动端）
 *
 * 与 xterm 渲染器同源的字体测量：xterm 内部用 32 个 'W' 的隐藏测量元素
 * （cellWidth = offsetWidth / 32，cellHeight = offsetHeight）。
 * 复刻同一逻辑，可在 Terminal 创建前算出网格尺寸 → 构造时直接传入正确
 * cols/rows，消灭「默认 80x24 → 未 fit 尺寸被发送给 PTY」的窗口。
 *
 * 网格计算对齐 FitAddon 公式，并额外扣除行列余量：列尾留两格（滚动条
 * 预留宽之外再留两格，行尾字符远离边缘）、行尾留一格（底部留白呼吸空间）。
 */
import { logger } from '@/utils/frontendLogger'

/**
 * 自绘滚动条预留宽度（CSS px，唯一真源）：
 *
 * - 数值 = 自绘滚动指示线足迹（terminal.css .scrollbar-track：right 2px +
 *   width 4px = 6px），指示线完全落在预留区内，与终端画布零重叠
 * - 同时作为 xterm 构造选项 overviewRuler.width 传入（TerminalView）：xterm 6
 *   内部 SmoothScrollableElement 的 verticalScrollbarSize 与 FitAddon 的可用宽
 *   扣除均取自该值；缺省 14px 为「右侧固定空白竖条」的来源——原生滚动条已被
 *   CSS 隐藏，预留却仍按 14px 计算，画布与容器右缘之间出现死区
 * - 三处消费方必须同源：computeGridSize / getXtermScaledDimensions /
 *   TerminalView 的 overviewRuler.width（对齐 FitAddon 裸 fit 口径）
 */
export const TERMINAL_SCROLLBAR_GUTTER_PX = 6

/**
 * 行尾安全余量列数：网格列数在可用宽度内再预留的格数。
 *
 * 为什么需要：移动端 DOM 渲染下行尾列紧贴画布/容器右缘时，CJK 字形墨迹
 * 超出单元格 advance、字体回退度量偏差与亚像素舍入，都会被外层容器的
 * overflow:hidden 削掉右半（pi 等满行文本输出的最后一个字显示半个）。
 * 预留 1 列把行尾字符拉离边缘。所有 fit 口径必须一致携带：
 * computeGridSize 调用方与 getXtermScaledDimensions 的 marginCols 参数。
 */
export const TERMINAL_LINE_END_MARGIN_COLS = 1

/**
 * 移动端终端行高倍率（唯一真源，TerminalView 构造选项与网格预估共用）：
 *
 * 为什么 1.2：小屏上 CJK 满屏输出（opencode/pi 等中文 TUI）在 lineHeight 1 下
 * 行间近乎贴死，可读性差；1.2 提供行间呼吸感。取值需真机验证 TUI box-drawing
 * 边框的跨行连接（DOM 渲染器不拉伸字形，垂直线依赖字体墨迹跨行补连——
 * terminal.css 已放开行级 overflow，多数字体可连上；若真机出现边框断裂，
 * 优先下调此值而非回退 1）。
 *
 * 三处消费方必须同源：TerminalView 的 Terminal 构造 lineHeight 选项 /
 * measureCellSize（cell 高度随 lineHeight 放大）/ computeGridSize 调用链。
 * 不一致会导致「预估网格 ≠ 实际渲染网格」，表现为顶部空带或行数偏差。
 */
export const TERMINAL_LINE_HEIGHT = 1.2

/** 字体网格尺寸（与 xterm renderService.dimensions.css.cell 同源） */
export interface CellSize {
  width: number
  height: number
}

/**
 * 随包内置的 CJK 严格等宽字体族名（@font-face 声明见 styles/terminal-font.css）：
 * 拉丁 0.5em / CJK 1em（= 2 格）/ 制表符 0.5em —— 终端行尾对齐的硬需求，
 * 系统等宽字体 + 比例 CJK 回退会让 CJK advance ≠ 2×格宽，误差逐字累积成
 * 行尾「凹凸」与 TUI 背景盒出界（见 styles/terminal.css「行尾软裁切」注释）。
 */
export const TERMINAL_CJK_FONT_FAMILY = 'Sarasa Mono SC'

/**
 * 移动端终端字体栈（唯一真源，TerminalView 与启动尺寸预估共用）：
 * 内置 CJK 等宽优先，monospace 次之（Android 无 Cascadia/Consolas/Monaco，
 * 直接回退系统等宽，避免「测量时字体缓存未就绪 → fallback 不同 → 网格与渲染
 * 宽度不一致」导致行尾字符溢出/裁半）；Windows 桌面调试时回退链覆盖等宽字体。
 *
 * 内置字体必须与 @font-face 声明同序：xterm 的字符测量元素与本文件的
 * measureCellSize 用同一串字体，两者不一致会让「测量格宽 ≠ 渲染格宽」。
 */
export const FONT_FAMILY =
  `"${TERMINAL_CJK_FONT_FAMILY}", monospace, "Cascadia Mono", Consolas, Monaco, "Courier New", "Roboto Mono", "Droid Sans Mono"`

/** 内置字体就绪等待上限（ms）：解码异常/设备异常时不能让终端永远等下去 */
const FONT_LOAD_TIMEOUT_MS = 3000

/**
 * 等待内置 CJK 等宽字体就绪（返回是否真的用上了内置字体）。
 *
 * 为什么必须等：字体未就绪时 measureCellSize / xterm 内部 charMeasure 量到的是
 * fallback 的格宽（0.6em 级），字体到位后变成内置的 0.5em —— 同一屏先后按两套
 * 度量算列数/行数，表现为行尾错位、满行被裁、fit 反复横跳。等到位再首次测量，
 * 「测量口径 = 渲染口径」才成立。
 *
 * 失败不阻断终端：超时/不支持 document.fonts 时返回 false，按 fallback 栈继续
 * （与引入内置字体前的行为一致）。
 */
export async function ensureTerminalFontLoaded(fontSize: number): Promise<boolean> {
  if (typeof document === 'undefined' || !document.fonts) return false
  // 测量元素用 32 个 W（与本文件 measureCellSize / xterm 内部同法），按此提示
  // 浏览器需要哪些字形；两串字体都 load：内置族名 + 完整栈
  const probe = 'W'.repeat(32)
  let timer: ReturnType<typeof setTimeout> | undefined
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timeout after ${FONT_LOAD_TIMEOUT_MS}ms`)), FONT_LOAD_TIMEOUT_MS)
  })
  try {
    await Promise.race([
      Promise.all([
        document.fonts.load(`${fontSize}px "${TERMINAL_CJK_FONT_FAMILY}"`, probe),
        document.fonts.load(`${fontSize}px ${FONT_FAMILY}`, probe),
      ]),
      timeout,
    ])
    return document.fonts.check(`${fontSize}px "${TERMINAL_CJK_FONT_FAMILY}"`)
  } catch (e) {
    // 走 fallback 栈：功能不缺失（只是 CJK 对齐回退到系统字体），记 warn 不静默
    logger.warn(
      `[terminalMetrics] 内置终端字体未就绪，按 fallback 栈渲染: ${e instanceof Error ? e.message : String(e)}`,
    )
    return false
  } finally {
    // 字体先到时也要清掉超时定时器（Promise.race 的落败方不会自动清理）
    if (timer) clearTimeout(timer)
  }
}

/**
 * 测量字体网格：32 个 'W' 的隐藏行内元素（与 xterm _measureElement 同法）。
 * lineHeight 与 Terminal 构造选项同源（TERMINAL_LINE_HEIGHT），测得的
 * offsetHeight 即「cell 高度 ≈ 字符高 × 行高倍率」的预估，与 xterm 渲染器
 * 的 css.cell.height 口径一致（亚像素舍入差异由 fit 收敛循环吸收）。
 * 字体未就绪时返回 0 尺寸，调用方回退默认值。
 */
export function measureCellSize(
  fontSize: number,
  fontFamily: string,
  lineHeight: number = TERMINAL_LINE_HEIGHT,
): CellSize {
  const el = document.createElement('div')
  el.style.cssText = [
    'position:absolute',
    'visibility:hidden',
    'left:-9999px',
    'top:0',
    `font-size:${fontSize}px`,
    `font-family:${fontFamily}`,
    `line-height:${lineHeight}`,
    'white-space:nowrap',
  ].join(';')
  el.textContent = 'W'.repeat(32)
  document.body.appendChild(el)
  const width = el.offsetWidth / 32
  const height = el.offsetHeight
  el.remove()
  return { width, height }
}

/**
 * 计算适配容器的网格尺寸（与 FitAddon.proposeDimensions 同公式）：
 *   cols = ⌊(容器宽 − 滚动条预留宽 − marginCols×cellWidth) / cellWidth⌋
 *   rows = ⌊(容器高 − marginRows×cellHeight) / cellHeight⌋
 *
 * 行列余量不对称（与 fitWithMargin 保持一致）：
 * - 列尾预留 2 格：滚动条预留宽之外再留两格，行尾字符远离滚动条/边缘
 * - 行尾预留 1 格：内容底部与输入栏之间留一行呼吸空间，不遮挡
 *
 * @param marginCols - 列尾额外预留的格数（默认 2）
 * @param marginRows - 行尾额外预留的格数（默认 1）
 * @param lineHeight - 行高倍率（默认 TERMINAL_LINE_HEIGHT，与渲染口径同源）
 * @returns 网格尺寸；字体未就绪（cell 尺寸为 0）时返回 { cols: 0, rows: 0 }
 */
export function computeGridSize(
  container: HTMLElement,
  fontSize: number,
  fontFamily: string,
  marginCols = 2,
  marginRows = 1,
  lineHeight: number = TERMINAL_LINE_HEIGHT,
): { cols: number; rows: number } {
  const cell = measureCellSize(fontSize, fontFamily, lineHeight)
  if (cell.width <= 0 || cell.height <= 0) return { cols: 0, rows: 0 }
  // 滚动条：scrollback > 0 时 FitAddon 扣除 overviewRuler.width（见常量注释）
  const width = container.clientWidth - TERMINAL_SCROLLBAR_GUTTER_PX - cell.width * marginCols
  const height = container.clientHeight - cell.height * marginRows
  return {
    cols: Math.max(2, Math.floor(width / cell.width)),
    rows: Math.max(1, Math.floor(height / cell.height)),
  }
}

/** 字体未就绪时的兜底网格 */
const FALLBACK_GRID = { cols: 80, rows: 24 }

/**
 * 设备默认网格预估（会话启动时随请求传给主机 PTY 作初始尺寸）：
 * 以设备屏幕可视区为容器、按当前终端字号预算，同一设备/朝向下数值稳定。
 *
 * 为什么：PTY 在主机端「启动」时即按初始尺寸创建，早于 TerminalView 挂载；
 * 不传则主机用固定缺省（120x40），与手机屏差距大。挂载后仍由 fit 校准 +
 * resize 队列同步精确值，本估算只需消灭起步尺寸偏差窗口。
 *
 * @returns 网格尺寸；屏幕/字体不可用时回退 {80, 24}
 */
export function computeDeviceDefaultGridSize(fontSize: number): { cols: number; rows: number } {
  if (!Number.isFinite(fontSize) || fontSize <= 0) return FALLBACK_GRID
  const root = document.documentElement
  if (root.clientWidth <= 0 || root.clientHeight <= 0) return FALLBACK_GRID
  const grid = computeGridSize(root, fontSize, FONT_FAMILY, 2, 1)
  if (grid.cols <= 0 || grid.rows <= 0) return FALLBACK_GRID
  return grid
}
