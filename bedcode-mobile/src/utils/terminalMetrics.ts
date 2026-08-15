/**
 * 终端网格测量工具（移动端）
 *
 * 与 xterm 渲染器同源的字体测量：xterm 内部用 32 个 'W' 的隐藏测量元素
 * （cellWidth = offsetWidth / 32，cellHeight = offsetHeight）。
 * 复刻同一逻辑，可在 Terminal 创建前算出网格尺寸 → 构造时直接传入正确
 * cols/rows，消灭「默认 80x24 → 未 fit 尺寸被发送给 PTY」的窗口。
 *
 * 网格计算对齐 FitAddon 公式，并额外扣除行列余量：列尾留两格（滚动条
 * 14px 之外再留两格，行尾字符远离边缘）、行尾留一格（底部留白呼吸空间）。
 */

/** 字体网格尺寸（与 xterm renderService.dimensions.css.cell 同源） */
export interface CellSize {
  width: number
  height: number
}

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
 *   cols = ⌊(容器宽 − 滚动条14px − marginCols×cellWidth) / cellWidth⌋
 *   rows = ⌊(容器高 − marginRows×cellHeight) / cellHeight⌋
 *
 * 行列余量不对称（与 fitWithMargin 保持一致）：
 * - 列尾预留 2 格：滚动条 14px 之外再留两格，行尾字符远离滚动条/边缘
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
  // 滚动条：scrollback > 0 时 FitAddon 扣除 14px（overviewRuler 宽度）
  const width = container.clientWidth - 14 - cell.width * marginCols
  const height = container.clientHeight - cell.height * marginRows
  return {
    cols: Math.max(2, Math.floor(width / cell.width)),
    rows: Math.max(1, Math.floor(height / cell.height)),
  }
}
