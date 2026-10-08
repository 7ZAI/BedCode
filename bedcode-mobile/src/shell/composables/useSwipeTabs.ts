/**
 * 弹窗 Tab 横向滑动切换 — 壳内机制副本
 * -----------------------------------------------------------------------------
 * 迁移自旧机制 `src/composables/useSwipeTabs.ts`，逻辑逐字复制（零外部依赖）。
 * 弹窗内手势切 Tab 是平台级交互，应用内页面复用同一形状即可，不各写一份。
 *
 * 内容区水平滑动超阈值触发 onSwitch；垂直主导时不记录 delta，
 * 不干扰内容区自身的垂直滚动。水平方向不调用 preventDefault，
 * 避免阻断滚动容器行为。
 */

export function useSwipeTabs(onSwitch: (dir: 'left' | 'right') => void) {
  let startX = 0
  let startY = 0
  let deltaX = 0

  /** 滑动方向判定阈值（px），低于阈值视为点按/轻微移动 */
  const THRESHOLD = 48

  function onTouchStart(e: TouchEvent) {
    const t = e.touches[0]
    startX = t.clientX
    startY = t.clientY
    deltaX = 0
  }

  function onTouchMove(e: TouchEvent) {
    const t = e.touches[0]
    const dx = t.clientX - startX
    const dy = t.clientY - startY
    // 仅水平主导时记录滑动，垂直主导（内容区上下滚动）时清零
    deltaX = Math.abs(dx) > Math.abs(dy) ? dx : 0
  }

  function onTouchEnd() {
    if (deltaX < -THRESHOLD) onSwitch('left')
    else if (deltaX > THRESHOLD) onSwitch('right')
    deltaX = 0
  }

  return { onTouchStart, onTouchMove, onTouchEnd }
}
