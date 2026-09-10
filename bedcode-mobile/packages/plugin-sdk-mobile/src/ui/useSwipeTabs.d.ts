/** 手势跳过选项：排除不应作为切页手势的触摸起点 */
export interface SwipeTabsOptions {
  /** 返回 true 时该轮触摸跳过手势判定（touchstart 的目标元素入参） */
  shouldSkip?: (target: EventTarget | null) => boolean
}

/**
 * 弹窗/页签内容区横向滑动切换（插件侧通用）
 *
 * 水平主导滑动超阈值（48px）触发 onSwitch('left' | 'right')；
 * 垂直主导不干扰内容滚动。返回绑定到模板的 touchstart/touchmove/touchend 处理器。
 */
export declare function useSwipeTabs(
  onSwitch: (dir: 'left' | 'right') => void,
  options?: SwipeTabsOptions
): {
  onTouchStart: (e: TouchEvent) => void
  onTouchMove: (e: TouchEvent) => void
  onTouchEnd: () => void
}
