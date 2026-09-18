/**
 * 页面级 pan 守卫（终端页专用）
 *
 * 为什么需要：AndroidManifest adjustNothing 下键盘不压缩布局视口（ICB 恒为
 * 全屏高），键盘弹出时「布局视口 − 可视视口」的差值构成 visual viewport
 * 可平移空间——用户在输入区等任何非滚动面上滑动，浏览器会把整个页面
 * （含标题栏）向键盘方向拖走，露出键盘上方的空白（真机实测 offsetTop 可达
 * 290+px 且 scrollTo 无法复位）。根容器高度收缩消除了布局内的双重补偿，
 * 但 pan 空间由 ICB 决定，只能从手势源头阻断。
 *
 * 规则：从触点向上遍历至守卫根节点——存在能沿手势方向继续滚动的原生滚动
 * 容器（textarea 超行内容、快捷条横滑、侧栏文件列表、补全面板等）则放行；
 * 否则 preventDefault 阻断页面级 pan。注意不能用 CSS touch-action 在祖先
 * 上一刀切：touch-action 沿祖先链取交集，祖先 none 会连带禁掉 textarea/
 * 快捷条自身的原生滚动。
 *
 * 手势轴锁定（真机教训）：轴在首次显著位移（>6px）时锁定，整段手势不再
 * 改判——滑动起手的纵向噪声若逐事件改判成纵向分支，会对横向手势误发
 * preventDefault，Chromium 一旦被 prevent 即取消整段原生滚动（快捷条
 * 左右滑动失效的根因）。
 */

/** 手势轴判定阈值（px）：位移超过该值才锁定方向 */
const AXIS_THRESHOLD_PX = 6

/** 判断元素是否为「能沿手势方向继续滚动」的原生纵向滚动容器 */
export function nativeScrollCanConsume(el: Element, dy: number): boolean {
  if (!(el instanceof HTMLElement)) return false
  const overflowY = getComputedStyle(el).overflowY
  if (overflowY !== 'auto' && overflowY !== 'scroll') return false
  // 手指上滑（dy<0）→ 内容上移 → 需要下方还有余量；反之上方有余量
  return dy < 0
    ? el.scrollTop + el.clientHeight < el.scrollHeight - 1
    : el.scrollTop > 1
}

/**
 * 手势链上是否存在原生横向滚动容器（快捷条等）：存在即放行——页面本身
 * 没有横向滚动空间，水平手势不会演变成页面 pan，无需余量检查。
 * 注意 quick-bar 为 direction:rtl，其 scrollLeft 语义为负向，任何基于
 * scrollLeft 的余量判断都必须区分 RTL——这里刻意不做余量检查来规避。
 */
export function hasNativeHorizontalScroller(from: Element | null, root: Element): boolean {
  let el = from
  while (el && el !== root) {
    if (el instanceof HTMLElement) {
      const overflowX = getComputedStyle(el).overflowX
      if (overflowX === 'auto' || overflowX === 'scroll') return true
    }
    el = el.parentElement
  }
  return false
}

/** 手势链上是否存在能沿手势方向继续滚动的原生纵向滚动容器 */
export function verticalChainCanConsume(from: Element | null, root: Element, dy: number): boolean {
  let el = from
  while (el && el !== root) {
    if (nativeScrollCanConsume(el, dy)) return true
    el = el.parentElement
  }
  return false
}

/** 守卫句柄：dispose 解除监听 */
export interface ViewportPanGuard {
  dispose(): void
}

/**
 * 在根元素上挂接页面 pan 守卫（capture 阶段）：
 * - 横向手势：链上存在原生横向滚动容器即放行
 * - 纵向手势：链上滚动容器能沿方向继续滚则放行，否则 preventDefault 阻断
 *   页面 pan（含可视视口平移）；无余量时一并阻断，防滚动链接进页面 pan
 * - touchstart/touchend 仅记录/复位手势状态（passive，不拦截）
 * preventDefault 只抑制浏览器默认行为，不影响其他 JS 触摸监听
 */
export function attachViewportPanGuard(root: HTMLElement): ViewportPanGuard {
  let startX = 0
  let startY = 0
  let axis: 'h' | 'v' | null = null

  const onTouchStart = (e: TouchEvent) => {
    startX = e.touches[0].clientX
    startY = e.touches[0].clientY
    axis = null
  }

  const onTouchMove = (e: TouchEvent) => {
    const touch = e.touches[0]
    const dx = touch.clientX - startX
    const dy = touch.clientY - startY
    if (axis === null) {
      if (Math.abs(dx) < AXIS_THRESHOLD_PX && Math.abs(dy) < AXIS_THRESHOLD_PX) return
      axis = Math.abs(dx) > Math.abs(dy) ? 'h' : 'v'
    }
    const target = e.target instanceof Element ? e.target : null
    if (axis === 'h') {
      if (hasNativeHorizontalScroller(target, root)) return
    } else if (verticalChainCanConsume(target, root, dy)) {
      return
    }
    e.preventDefault()
  }

  const onTouchEnd = () => {
    axis = null
  }

  root.addEventListener('touchstart', onTouchStart, { capture: true, passive: true })
  root.addEventListener('touchmove', onTouchMove, { capture: true, passive: false })
  root.addEventListener('touchend', onTouchEnd, { capture: true, passive: true })

  return {
    dispose() {
      root.removeEventListener('touchstart', onTouchStart, { capture: true } as EventListenerOptions)
      root.removeEventListener('touchmove', onTouchMove, { capture: true } as EventListenerOptions)
      root.removeEventListener('touchend', onTouchEnd, { capture: true } as EventListenerOptions)
    },
  }
}
