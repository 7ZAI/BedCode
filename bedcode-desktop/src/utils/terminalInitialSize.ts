/**
 * 桌面端终端初始网格精确预测（会话启动时给 PTY 初始尺寸用）
 *
 * 为什么能精确：桌面终端是独立窗口，创建尺寸确定（见 useSessionWindows.
 * openTerminalWindow）：宽 = 主窗口内容区宽 × 60%、高 = 主窗口内容区高；
 * 窗口内 chrome 固定 —— 工具条 h-10(40px) + 状态条 h-6(24px)，xterm 宿主
 * 无水平内边距。故在启动时刻即可由主窗口尺寸推出终端网格，PTY openpty
 * 直接以正确行列创建；挂载后 FitAddon 校准仅剩 ±0 行列的测量残差。
 *
 * 字体测量与 xterm 同法（32 个 'W' 的隐藏元素），逻辑像素口径一致。
 */

/** 终端窗口内固定 chrome：顶部工具条（h-10）+ 底部状态条（h-6） */
export const TERMINAL_WINDOW_CHROME_PX = 40 + 24

/** 滚动条宽度（FitAddon 在 scrollback > 0 时扣除 14px） */
const SCROLLBAR_PX = 14

/** 终端窗口宽度占主窗口内容区比例（openTerminalWindow 创建规则） */
export const TERMINAL_WINDOW_WIDTH_RATIO = 0.6

/** 桌面端终端字体栈（与 TerminalPreview initTerminal 保持一致） */
const FONT_STACK = 'Cascadia Mono, Consolas, Monaco, Courier New, monospace'

/**
 * 测量字体网格：32 个 'W' 的隐藏行内元素（与 xterm _measureElement 同法）。
 * 字体未就绪时返回 0 尺寸，调用方回退服务端默认值。
 */
function measureCellSize(fontSize: number, fontFamily: string): { width: number; height: number } {
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
 * 预测桌面终端窗口的初始网格：
 *
 * - 主窗口上下文（会话列表点启动，终端窗口尚未创建）：widthRatio 传
 *   TERMINAL_WINDOW_WIDTH_RATIO，按创建规则推算未来窗口尺寸；
 * - 终端窗口内上下文（延迟启动，组件已挂载但 xterm 未就绪）：保持默认
 *   widthRatio=1，即以当前窗口实际尺寸计算。
 *
 * @returns 网格尺寸；任一环节不可用（Tauri API 失败/字体未就绪/尺寸退化）
 *          返回 null，调用方不传、由服务端用配置默认值兜底。
 */
export async function computeDesktopInitialTerminalSize(
  fontSize: number,
  { widthRatio = 1 }: { widthRatio?: number } = {},
): Promise<{ cols: number; rows: number } | null> {
  if (!Number.isFinite(fontSize) || fontSize <= 0) return null
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const scaleFactor = await getCurrentWindow().scaleFactor()
    const inner = (await getCurrentWindow().innerSize()).toLogical(scaleFactor)
    const termW = Math.floor(inner.width * widthRatio)
    const termH = Math.round(inner.height)
    if (termW <= 0 || termH <= 0) return null
    const cell = measureCellSize(fontSize, FONT_STACK)
    if (cell.width <= 0 || cell.height <= 0) return null
    return {
      cols: Math.max(2, Math.floor((termW - SCROLLBAR_PX) / cell.width)),
      rows: Math.max(1, Math.floor((termH - TERMINAL_WINDOW_CHROME_PX) / cell.height)),
    }
  } catch {
    return null
  }
}
