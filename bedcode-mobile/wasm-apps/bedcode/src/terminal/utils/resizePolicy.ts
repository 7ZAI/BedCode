/**
 * 终端网格 resize 触发策略（纯逻辑模块，Seam A）
 *
 * 为什么需要 ±1 列钳制：applyDprFit 的尺寸口径（容器 clientWidth × 渲染器
 * 反推的 css.cell 宽）与 xterm 当前网格之间存在 ±1~2 列的系统性测量偏差
 * （round 误差、DPR 非整数、滚动条/边框、每次 resize 后 xterm 重测字体的
 * subpixel 漂移）。若每次都按精确差触发 resize，点「刷新」（fit 一次）几乎必然
 * 触发一次 resize —— 而 WebGL 渲染器在 resize 时会 `_refreshCharAtlas()` 重建
 * 整个字符图集（warmUp 仅预热 ASCII 33~126），整屏非 ASCII 字形需按
 * requestIdleCallback 分片异步重新光栅化，表现为「前几次刷新格式混乱（字形
 * 缺失/错位），图集预热完成后才正常」。
 *
 * 为什么不是「触发与否」的布尔而返回解析后的网格：xterm 的
 * `Buffer.resize` 中 `_reflow` 以「列是否变化」为开关——
 *   `_reflow(newCols, newRows) { if (this._cols === newCols) return; ... }`
 * 列一变就走 `_reflowLarger` / `_reflowSmaller` 遍历并重写**整个 scrollback**
 * （移动端 `TERMINAL_SCROLLBACK = 10000`），成本随历史行数线性上升；行变化则不
 * 触发任何 reflow。因此「行变化顺带把 ±1 列漂移一并写进网格」会让每次键盘避让
 * 都付一次整缓冲重排——真机日志里可见列在 96↔97 之间随键盘弹出/收起翻转。
 * 这里返回 `{ cols, rows }`：列漂移裁掉（保持当前列），行变化照旧生效。
 *
 * 行方向为什么不钳制（包括此前「增大 ±1 钳制」的取舍修正）：
 * - 行缩小不钳制是既有语义（最后一行被裁/底部空带肉眼可见）；
 * - 行增大同样不钳制：终端网格贴底对齐后（.xterm-container .xterm bottom:0），
 *   网格小于容器的缺额全部暴露在顶部——标题栏与首行文字之间出现空带。字体度量
 *   就绪晚于首次 fit（charMeasure 初始估值偏大 → 行数偏少），若行增大被 ±1 钳制
 *   挡住，网格会永久卡在比容器少 1~2 行的状态，且容器尺寸不变时 ResizeObserver
 *   不会再触发重 fit，空带无法自愈（真机实测 40px ≈ 2.8 行空带）。
 * - 风暴风险可控：行变化由 ResizeObserver 的真实容器高度变化驱动（键盘避让走
 *   容器高度收缩），±1 行的测量抖动几乎不会发生；防抖器对等值喂入也直接忽略。
 */

/** 网格偏差在此阈值以内视为测量漂移，不写入网格（仅列方向） */
export const RESIZE_GRID_TOLERANCE = 1

/**
 * 解析本次应写入 xterm 的目标网格。
 *
 * 契约：
 * - 列偏差 ≤ 1 → 保持当前列（裁掉测量漂移，避免触发整缓冲 reflow）；
 *   列偏差 > 1 → 采用目标列（真实容器宽度/字号变化）；
 * - 行方向任何真实变化立即采用（双向同权，见文件头说明）；
 * - 解析结果与当前网格完全一致 → 返回 null（调用方不得 resize，避免无谓事件）。
 *
 * @returns 需要写入的网格；null = 无需 resize
 */
export function resolveGridResize(
  currentCols: number,
  currentRows: number,
  targetCols: number,
  targetRows: number,
): { cols: number; rows: number } | null {
  const cols = Math.abs(targetCols - currentCols) > RESIZE_GRID_TOLERANCE ? targetCols : currentCols
  // 行任何变化都采用；列被钳制时结果可能等于当前网格（此时不 resize）
  if (cols === currentCols && targetRows === currentRows) return null
  return { cols, rows: targetRows }
}

/**
 * 字符图集重建后的补刷延迟（ms）：等 async 分片把整屏非 ASCII 字形基本
 * 光栅化完成后，补一次全量 refresh 让屏幕恢复。
 *
 * 仅在 WebGL 渲染器激活时使用（DOM 渲染器无字符图集，refresh 只是重建 DOM 行，
 * 由渲染循环自身驱动，预热纯属多余的整屏重绘）。
 */
export const ATLAS_PREHEAT_DELAY_MS = 700
