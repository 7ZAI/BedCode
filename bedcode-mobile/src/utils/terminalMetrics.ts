/**
 * 终端网格测量工具（移动端）
 *
 * 与 xterm 渲染器同源的字体测量：xterm 内部用 32 个 'W' 的隐藏测量元素
 * （cellWidth = offsetWidth / 32，cellHeight = offsetHeight）。
 * 复刻同一逻辑，可在 Terminal 创建前算出网格尺寸 → 构造时直接传入正确
 * cols/rows，消灭「默认 80x24 → 未 fit 尺寸被发送给 PTY」的窗口。
 *
 * 网格计算对齐 FitAddon 公式，行尾不再扣额外格数（见 TERMINAL_RIGHT_RESERVE_PX
 * 「右侧竖直黑带」实测）；行尾仍留一格呼吸空间（marginRows 默认 1）。
 */
import { logger } from '@/utils/frontendLogger'

/**
 * 行尾右缘预留宽（CSS px，网格计算的唯一真源）：**0 = 不预留**。
 *
 * 为什么改成 0（2026-10-03 真机 CDP 实测）：
 * 预留宽 = 「容器宽 − 网格宽」的整条差额，这块差额没有单元格绘制，露出的是终端
 * 底色 `--terminal-canvas-bg`。普通 shell 输出（pi）全程用终端底色，看不出来；
 * 但 TUI（opencode）用**自带底色**铺满自己的列宽，于是右侧多出一条**竖直黑带**。
 * 本机实测：容器 407px、fontSize 15 → 格宽 7.5px，旧口径再扣 6px 预留 + 1 列
 * → cols=52、网格 390px，黑带 17px ≈ 屏宽 4.2%（pi 会话无此现象，与用户反馈一致）。
 *
 * 改成 0 后的代价与兜底（三处都已就位，不留回归面）：
 * - 差额退化为「容器宽 mod 格宽」< 1 格（本机 ≤7.5px ≈ 1.8%），是终端固有的取整
 *   余量，桌面端同样存在，视觉上是一条发丝线；
 * - 行尾墨迹越界（hinting 亚像素）由 styles/terminal.css 的
 *   `.xterm-screen { clip-path: inset(0 -6px 0 0) }` 右扩 6px 承接，行级
 *   `overflow: visible` 让墨迹能落到差额里；
 * - 自绘滚动指示线（.scrollbar-track，right 2px + width 4px = 6px 足迹）改为
 *   **覆盖**在最后一列之上：它本就是 `pointer-events: none` 的覆盖层，半透明且
 *   仅滚动中短暂可见（原口径下它整体落在预留区内，与画布零重叠）。
 *
 * 消费方必须同源：computeGridSize / getXtermScaledDimensions 的 marginCols 入参
 * / TerminalView 的 overviewRuler.width。
 */
export const TERMINAL_RIGHT_RESERVE_PX = 0

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
 *
 * 附：CJK 字间距不是靠它调节的——严格等宽下每字恒占 2 格，字内字距由字体
 * 自身留白决定（见 TERMINAL_CJK_FONT_FAMILY 与 bustFontFamilyCache）。
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

/** 「字体已进入布局」逐帧复核的上限（帧）：超过即按未就绪放行，不阻断终端 */
const FONT_LAYOUT_PROBE_MAX_FRAMES = 12

/** 探针字符数：32 个 'W'（与 measureCellSize / xterm charMeasure 同法） */
const PROBE_CHARS = 32

/** 推进宽差异阈值（CSS px/字符）：小于它视为「还是同一套 fallback 字形」 */
const FONT_LAYOUT_EPSILON = 0.01

/**
 * 测量一段文本在指定字体栈下的单字符推进宽（CSS px/字符）。
 *
 * 与 measureCellSize 同法（隐藏内联块 + offsetWidth/字符数）；这里用
 * `display: inline-block` 而非块级，否则量到的是容器宽度而非文本宽度。
 */
function measureAdvancePerChar(text: string, fontSize: number, fontFamily: string): number {
  const el = document.createElement('span')
  el.style.cssText = [
    'position:absolute',
    'display:inline-block',
    'visibility:hidden',
    'left:-9999px',
    'top:0',
    `font-size:${fontSize}px`,
    `font-family:${fontFamily}`,
    'white-space:pre',
    'font-kerning:none',
  ].join(';')
  el.textContent = text
  document.body.appendChild(el)
  const width = el.offsetWidth
  el.remove()
  return width / text.length
}

/**
 * 纯判定：内置字体是否已真正进入布局。
 *
 * 「document.fonts.load() 已 resolve」不等于「排版已换上该字形」——内置字体声明为
 * `font-display: swap`，WebView 常在 load() 返回后的下一帧才把 fallback 换成
 * Sarasa。此时 xterm 的 charMeasure / WidthCache 量到的仍是系统等宽宽度。
 *
 * 判定口径：终端字体栈（内置族优先）与纯回退栈（monospace）的单字符推进宽
 * 不同 → 已换字形；相同 → 还在 fallback 上。
 */
export function isBundledFontInLayout(terminalAdvance: number, fallbackAdvance: number): boolean {
  if (!Number.isFinite(terminalAdvance) || !Number.isFinite(fallbackAdvance)) return false
  if (terminalAdvance <= 0 || fallbackAdvance <= 0) return false
  return Math.abs(terminalAdvance - fallbackAdvance) > FONT_LAYOUT_EPSILON
}

/**
 * 逐帧复核内置字体是否已进入布局（最多 FONT_LAYOUT_PROBE_MAX_FRAMES 帧）。
 *
 * 成功返回 true；用尽帧数仍判定未换字形返回 false（调用方按 fallback 继续，不阻断）。
 */
async function waitForFontInLayout(fontSize: number): Promise<boolean> {
  const probe = 'W'.repeat(PROBE_CHARS)
  for (let frame = 0; frame < FONT_LAYOUT_PROBE_MAX_FRAMES; frame++) {
    if (isBundledFontInLayout(measureAdvancePerChar(probe, fontSize, FONT_FAMILY), measureAdvancePerChar(probe, fontSize, 'monospace'))) {
      return true
    }
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  }
  return false
}

/**
 * 等内置 CJK 等宽字体「既已加载、又已进入排版」后才允许首次测量（返回是否两者都成立）。
 *
 * 为什么必须等：字体未就绪时 measureCellSize / xterm 内部 charMeasure 量到的是
 * fallback 的格宽（0.6em 级），字体到位后变成内置的 0.5em —— 同一屏先后按两套
 * 度量算列数/行数，表现为行尾错位、满行被裁、fit 反复横跳。等到位再首次测量，
 * 「测量口径 = 渲染口径」才成立。
 *
 * 为什么还要复核排版（2026-10-03 真机 CDP 实测）：本机 `document.fonts.load()`
 * resolve + `check()` 为 true 时，排版仍在用系统等宽（真机 rows letter-spacing
 * 被算成 -1.5px = 格宽 7.5 − fallback 9.0）。该补偿由 DomRenderer 按「每字符」
 * 施加，CJK 一并被压窄 1.5px → 汉字逐字重叠（用户反馈「中文间距几乎没有」）。
 * 复核通过后，调用方还须用 bustFontFamilyCache 强制 xterm 失效重测一次。
 *
 * 失败不阻断终端：超时/不支持 document.fonts/复核用尽帧数时返回 false，按
 * fallback 栈继续（与引入内置字体前的行为一致）。
 */
export async function ensureTerminalFontLoaded(fontSize: number): Promise<boolean> {
  if (typeof document === 'undefined' || !document.fonts) return false
  // 测量元素用 32 个 W（与本文件 measureCellSize / xterm 内部同法），按此提示
  // 浏览器需要哪些字形；两串字体都 load：内置族名 + 完整栈
  const probe = 'W'.repeat(PROBE_CHARS)
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
    if (!document.fonts.check(`${fontSize}px "${TERMINAL_CJK_FONT_FAMILY}"`)) return false
    const inLayout = await waitForFontInLayout(fontSize)
    if (!inLayout) {
      // 已加载但排版未换（或本机恰好同宽）→ 记 warn 继续，由 bustFontFamilyCache 兜底
      logger.warn(
        `[terminalMetrics] 内置终端字体已加载但排版未换（${fontSize}px），` +
          `按 fallback 度量创建后再强制 xterm 重测`,
      )
    }
    return inLayout
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
 * 字体串「同字体不同串」：强制 xterm 失效重测（宽度缓存 + 格宽）。
 *
 * 为什么需要（2026-10-03 真机 CDP 实测）：xterm 的 `WidthCache.setFont` 按
 * (fontFamily, fontSize, fontWeight, fontWeightBold) 去重——字体**内容**换了但
 * 这四个值没变时不会 `clear()`，于是缓存里留着 swap 之前的 fallback 宽度；
 * DomRenderer 随后的 `_setDefaultSpacing()` 把它当作「行宽补偿」写进
 * `rows { letter-spacing }`，而该补偿按每字符施加，CJK（2 格宽）被压窄同样的量。
 *
 * 追加一个尾部空格：CSS 解析结果完全等价，字符串不同 → xterm 的
 * onOptionChange 触发 charMeasure 重测 + WidthCache.clear() + 补偿重算。
 * 只能在 Terminal 构造/open 之后调用（构造期赋值等于没变）；幂等，重复赋值不会
 * 再改串（避免重复触发整屏重绘）。
 */
export function bustFontFamilyCache(fontFamily: string): string {
  return fontFamily.endsWith(' ') ? fontFamily : `${fontFamily} `
}

/**
 * 测量字体网格：32 个 'W' 的隐藏行内元素（与 xterm _measureElement 同法）。
 * lineHeight 与 Terminal 构造选项同源（TERMINAL_LINE_HEIGHT），测得的
 * offsetHeight 即「cell 高度 ≈ 字符高 × 行高倍率」的预估，与 xterm 渲染器
 * 的 css.cell.height 口径一致（亚像素舍入差异由 fit 收敛循环吸收）。
 *
 * @param letterSpacing - 终端字间距（px，与 xterm `letterSpacing` 选项同源）：
 *   xterm 的 `device.cell.width = char.width + letterSpacing`，预估网格必须叠加
 *   同一增量，否则「预估网格 ≠ 渲染网格」（列数多算）。默认 0 = 不加。
 * 字体未就绪时返回 0 尺寸，调用方回退默认值。
 */
export function measureCellSize(
  fontSize: number,
  fontFamily: string,
  lineHeight: number = TERMINAL_LINE_HEIGHT,
  letterSpacing = 0,
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
  // 字体未就绪（offsetWidth = 0）时保持 0 尺寸契约：只叠加间距会得到
  // 「间距本身」的伪格宽（>0），computeGridSize 的未就绪守卫会失守
  return { width: width > 0 ? width + letterSpacing : 0, height }
}

/**
 * 计算适配容器的网格尺寸（与 FitAddon.proposeDimensions 同公式）：
 *   cols = ⌊(容器宽 − marginCols×cellWidth) / cellWidth⌋
 *   rows = ⌊(容器高 − marginRows×cellHeight) / cellHeight⌋
 *
 * 列尾默认不再额外预留（TERMINAL_RIGHT_RESERVE_PX = 0：多留即多一条右侧竖直
 * 黑带，见该常量注释）；行尾默认留 1 格呼吸空间（内容底部与输入栏不贴死）。
 *
 * @param marginCols - 列尾额外预留的格数（默认 0）
 * @param marginRows - 行尾额外预留的格数（默认 1）
 * @param lineHeight - 行高倍率（默认 TERMINAL_LINE_HEIGHT，与渲染口径同源）
 * @param letterSpacing - 终端字间距（px，与 Terminal 构造选项同源，经
 *   measureCellSize 叠加进格宽；预估网格与渲染网格必须同一口径）
 * @returns 网格尺寸；字体未就绪（cell 尺寸为 0）时返回 { cols: 0, rows: 0 }
 */
export function computeGridSize(
  container: HTMLElement,
  fontSize: number,
  fontFamily: string,
  marginCols = 0,
  marginRows = 1,
  lineHeight: number = TERMINAL_LINE_HEIGHT,
  letterSpacing = 0,
): { cols: number; rows: number } {
  const cell = measureCellSize(fontSize, fontFamily, lineHeight, letterSpacing)
  if (cell.width <= 0 || cell.height <= 0) return { cols: 0, rows: 0 }
  const width = container.clientWidth - cell.width * marginCols
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
 *
 * @param letterSpacing - 终端字间距（px，经 computeGridSize 叠加进格宽）
 */
export function computeDeviceDefaultGridSize(fontSize: number, letterSpacing = 0): { cols: number; rows: number } {
  if (!Number.isFinite(fontSize) || fontSize <= 0) return FALLBACK_GRID
  const root = document.documentElement
  if (root.clientWidth <= 0 || root.clientHeight <= 0) return FALLBACK_GRID
  const grid = computeGridSize(root, fontSize, FONT_FAMILY, 0, 1, TERMINAL_LINE_HEIGHT, letterSpacing)
  if (grid.cols <= 0 || grid.rows <= 0) return FALLBACK_GRID
  return grid
}
