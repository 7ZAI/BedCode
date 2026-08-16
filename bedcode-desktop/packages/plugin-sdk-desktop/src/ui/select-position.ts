/**
 * Select 下拉面板定位纯函数（无 DOM 依赖，规则表驱动单测）
 *
 * 翻转规则（面板固定宽 = 触发器宽）：
 * - 下方空间足够 → 向下展开（现状）
 * - 下方不足但上方足够 → 向上展开
 * - 两侧都不足 → 收缩 maxHeight 到可用空间较大的一侧，面板内滚动
 * - 水平方向 clamp，保证面板不超出视口
 */

/** 面板与触发器的固定间距（px） */
export const SELECT_GAP = 4

/** 面板设计高度上限（与面板内列表 max-height 一致，px） */
export const SELECT_MAX_PANEL_HEIGHT = 240

/** 触发器矩形（仅需要定位用到的字段） */
export interface TriggerRect {
  top: number
  bottom: number
  left: number
  width: number
}

/** 视口尺寸 */
export interface ViewportSize {
  width: number
  height: number
}

/** 定位结果 */
export interface SelectPosition {
  top: number
  left: number
  /** 面板可用的最大高度（px；面板内滚动） */
  maxHeight: number
}

export function computeSelectPosition(
  triggerRect: TriggerRect,
  viewport: ViewportSize,
  panelHeight: number,
): SelectPosition {
  // 面板实际高度以设计上限截断（内容更长时交给面板内滚动）
  const height = Math.min(Math.max(panelHeight, 0), SELECT_MAX_PANEL_HEIGHT)

  const belowSpace = viewport.height - triggerRect.bottom - SELECT_GAP
  const aboveSpace = triggerRect.top - SELECT_GAP

  let top: number
  let maxHeight: number
  if (belowSpace >= height) {
    // 下方足够：保持现状向下展开
    top = triggerRect.bottom + SELECT_GAP
    maxHeight = height
  } else if (aboveSpace >= height) {
    // 下方不足但上方足够：向上展开
    top = triggerRect.top - SELECT_GAP - height
    maxHeight = height
  } else if (belowSpace >= aboveSpace) {
    // 两侧都不足：贴下方展开并收缩高度（下方空间更大时）；
    // maxHeight 再让出一个间距，保证面板底边与视口底边保留 4px 留白
    top = Math.max(triggerRect.bottom + SELECT_GAP, SELECT_GAP)
    maxHeight = Math.max(belowSpace - SELECT_GAP, 0)
  } else {
    // 两侧都不足且上方空间更大：向上展开并收缩高度；
    // maxHeight 让出触发器间距，面板底边不贴住触发器顶边
    maxHeight = Math.max(aboveSpace - SELECT_GAP, 0)
    top = Math.max(triggerRect.top - SELECT_GAP - maxHeight, SELECT_GAP)
  }

  // 水平 clamp：面板与视口边缘保持至少 SELECT_GAP 间距
  const left = Math.min(
    Math.max(triggerRect.left, SELECT_GAP),
    Math.max(viewport.width - triggerRect.width - SELECT_GAP, SELECT_GAP),
  )

  return { top, left, maxHeight }
}
