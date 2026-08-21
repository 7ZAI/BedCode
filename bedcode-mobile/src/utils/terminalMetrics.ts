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

/** 字体网格尺寸（与 xterm renderService.dimensions.css.cell 同源） */
export interface CellSize {
  width: number
  height: number
}

/**
 * 移动端终端字体栈（唯一真源，TerminalView 与启动尺寸预估共用）：
 * monospace 优先（Android 无 Cascadia/Consolas/Monaco，直接回退系统等宽，
 * 避免「测量时字体缓存未就绪 → fallback 不同 → 网格与渲染宽度不一致」导致
 * 行尾字符溢出/裁半）；Windows 桌面调试时回退链覆盖等宽字体
 */
export const FONT_FAMILY =
  'monospace, "Cascadia Mono", Consolas, Monaco, "Courier New", "Roboto Mono", "Droid Sans Mono"'

/**
 * 测量字体网格：32 个 'W' 的隐藏行内元素（与 xterm _measureElement 同法）。
 * 字体未就绪时返回 0 尺寸，调用方回退默认值。
 */
export function measureCellSize(fontSize: number, fontFamily: string): CellSize {
  const el = document.createElement('div')
  el.style.cssText = [
    'position:absolute',
    'visibility:hidden',
    'left:-9999px',
    'top:0',
    `font-size:${fontSize}px`,
    `font-family:${fontFamily}`,
    'line-height:1',
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
 * @returns 网格尺寸；字体未就绪（cell 尺寸为 0）时返回 { cols: 0, rows: 0 }
 */
export function computeGridSize(
  container: HTMLElement,
  fontSize: number,
  fontFamily: string,
  marginCols = 2,
  marginRows = 1,
): { cols: number; rows: number } {
  const cell = measureCellSize(fontSize, fontFamily)
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
