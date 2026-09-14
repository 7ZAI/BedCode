/**
 * 终端网格 resize 触发策略（纯逻辑模块，Seam A）
 *
 * 为什么需要 ±1 列钳制：applyDprFit 的尺寸口径（容器 clientWidth × 渲染器
 * 反推的 css.cell 宽）与 xterm 当前网格之间存在 ±1~2 列的系统性测量偏差
 * （round 误差、DPR 非整数、滚动条/边框）。若每次都按精确差触发 resize，
 * 点「刷新」（fit 一次）几乎必然触发一次 resize —— 而 WebGL 渲染器在
 * resize 时会 `_refreshCharAtlas()` 重建整个字符图集（warmUp 仅预热 ASCII
 * 33~126），整屏非 ASCII 字形需按 requestIdleCallback 分片异步重新光栅化，
 * 表现为「前几次刷新格式混乱（字形缺失/错位），图集预热完成后才正常」。
 * 移动端默认 DOM 渲染器对 resize 后的全量重绘开销小，钳制依然有效：
 * 避免无谓的 resize 事件风暴（旋转/键盘避让触发容器尺寸微调时）打乱网格。
 *
 * 因此：列方向仅当目标网格与当前网格相差 > 1 列才触发 resize（±1 以内漂移裁掉）；
 * 行方向任何真实变化都立即生效（缩小/增大同权）。
 *
 * 为什么行方向不再钳制（包括此前「增大 ±1 钳制」的取舍修正）：
 * - 行缩小不钳制是既有语义（最后一行被裁/底部空带肉眼可见）；
 * - 行增大同样不再钳制：终端网格贴底对齐后（.xterm-container .xterm bottom:0），
 *   网格小于容器的缺额全部暴露在顶部——标题栏与首行文字之间出现空带。字体度量
 *   就绪晚于首次 fit（charMeasure 初始估值偏大 → 行数偏少），若行增大被 ±1 钳制
 *   挡住，网格会永久卡在比容器少 1~2 行的状态，且容器尺寸不变时 ResizeObserver
 *   不会再触发重 fit，空带无法自愈（真机实测 40px ≈ 2.8 行空带）。
 * - 风暴风险可控：行变化由 ResizeObserver 的真实容器高度变化驱动（键盘避让走
 *   transform 不改变容器高度），±1 行的测量抖动几乎不会发生；防抖器对等值喂入
 *   也直接忽略。
 */

/** 网格偏差在此阈值以内视为测量漂移，不触发 resize（仅列方向） */
export const RESIZE_GRID_TOLERANCE = 1

/**
 * 判定目标网格是否值得触发 resize。
 * 列方向：偏差 > 1 才触发（防 ±1 测量漂移，DPR 口径与当前网格存在 ±1~2 列
 * 系统性偏差，每次精确比较都 resize 会在刷新时重建字符图集）。
 * 行方向：任何真实变化都立即触发（双向同权）——终端高度 = rows × cellHeight，
 * 网格与容器高度不一致时，贴底对齐会把缺额暴露在顶部（行偏少 → 顶部空带）或
 * 裁掉最后一行（行偏多），都不值得为防风暴而延迟。
 * @returns true = 真实尺寸变化，应 resize；false = 列测量漂移，保持当前网格
 */
export function shouldApplyGridResize(
  currentCols: number,
  currentRows: number,
  targetCols: number,
  targetRows: number,
): boolean {
  const colDiff = Math.abs(targetCols - currentCols)
  if (colDiff > RESIZE_GRID_TOLERANCE) return true
  // 行方向：任何真实变化立即生效（不再区分缩小/增大，取消 ±1 行增大钳制）
  return targetRows !== currentRows
}

/**
 * 字符图集重建后的补刷延迟（ms）：等 async 分片把整屏非 ASCII 字形基本
 * 光栅化完成后，补一次全量 refresh 让屏幕恢复。DOM 渲染器下 refresh 只是
 * 重建 DOM 行，无害；WebGL 渲染器（USE_WEBGL_RENDERER 开启时）则必要。
 */
export const ATLAS_PREHEAT_DELAY_MS = 700