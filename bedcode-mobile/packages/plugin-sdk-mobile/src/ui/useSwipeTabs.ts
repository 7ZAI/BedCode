/**
 * 弹窗/页签内容区横向滑动切换（插件侧通用）
 *
 * 与宿主 src/composables/useSwipeTabs 同语义：内容区水平主导滑动超阈值触发
 * onSwitch；垂直主导（内容区上下滚动）时不记录位移，不干扰自身滚动。
 * 水平方向不调用 preventDefault，避免阻断滚动容器行为。
 *
 * options.shouldSkip 用于排除不应作为切页手势的触摸起点（如输入控件、
 * 可横向滚动容器），在 touchstart 时以事件目标判定一次，整轮手势生效。
 */

export interface SwipeTabsOptions {
  /** 返回 true 时该轮触摸跳过手势判定（touchstart 的目标元素入参） */
  shouldSkip?: (target: EventTarget | null) => boolean
}

export function useSwipeTabs(onSwitch: (dir: 'left' | 'right') => void, options?: SwipeTabsOptions) {
  let startX = 0
  let startY = 0
  let deltaX = 0
  let skipped = false

  /** 滑动方向判定阈值（px），低于阈值视为点按/轻微移动 */
  const THRESHOLD = 48

  function onTouchStart(e: TouchEvent) {
    const t = e.touches[0]
    startX = t.clientX
    startY = t.clientY
    deltaX = 0
    skipped = options?.shouldSkip?.(e.target) ?? false
  }

  function onTouchMove(e: TouchEvent) {
    if (skipped) return
    const t = e.touches[0]
    const dx = t.clientX - startX
    const dy = t.clientY - startY
    // 仅水平主导时记录位移，垂直主导（内容区上下滚动）时清零
    deltaX = Math.abs(dx) > Math.abs(dy) ? dx : 0
  }

  function onTouchEnd() {
    if (!skipped) {
      if (deltaX < -THRESHOLD) onSwitch('left')
      else if (deltaX > THRESHOLD) onSwitch('right')
    }
    deltaX = 0
    skipped = false
  }

  return { onTouchStart, onTouchMove, onTouchEnd }
}
