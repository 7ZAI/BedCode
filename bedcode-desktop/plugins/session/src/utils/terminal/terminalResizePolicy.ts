/**
 * 终端网格 resize 触发策略（纯逻辑模块，Seam A）
 *
 * 为什么需要 ±1 列钳制：applyDprFit 的尺寸口径（容器 clientWidth × 渲染器
 * 反推的 css.cell 宽）与 xterm 当前网格之间存在 ±1~2 列的系统性测量偏差
 * （round 误差、DPR 非整数、滚动条/边框）。若每次都按精确差触发 resize，
 * 点击"刷新"（fit 一次）几乎必然触发一次 resize —— 而 WebGL 渲染器在
 * resize 时会 `_refreshCharAtlas()` 重建整个字符图集（warmUp 仅预热 ASCII
 * 33~126），整屏非 ASCII 字形需按 requestIdleCallback 分片异步重新光栅化，
 * 表现为「前几次刷新格式混乱（字形缺失/错位），图集预热完成后才正常」。
 *
 * 因此：仅当目标网格与当前网格相差 > 1 列/行（真实拖窗/字号变化）才触发
 * resize；±1 以内的测量漂移裁掉。代价是拖窗恰好只差 1 列时不立即生效
 * （视觉差异 <1 字符宽，无感），继续同向拖拽跨过 2 列差时正常触发。
 */

/** 网格偏差在此阈值以内视为测量漂移，不触发 resize（列/行各自独立判定） */
export const RESIZE_GRID_TOLERANCE = 1

/**
 * 判定目标网格是否值得触发 resize。
 * @returns true = 真实尺寸变化，应 resize；false = 测量漂移，保持当前网格
 */
export function shouldApplyGridResize(
  currentCols: number,
  currentRows: number,
  targetCols: number,
  targetRows: number,
): boolean {
  return (
    Math.abs(targetCols - currentCols) > RESIZE_GRID_TOLERANCE ||
    Math.abs(targetRows - currentRows) > RESIZE_GRID_TOLERANCE
  )
}

/**
 * WebGL 字符图集重建后的补刷延迟（ms）：等 requestIdleCallback 分片
 * 把整屏非 ASCII 字形基本光栅化完成后，补一次全量 refresh 让屏幕恢复。
 */
export const ATLAS_PREHEAT_DELAY_MS = 700