/**
 * 页签横滑仲裁（票 2026-10-10：全量 UI 下沉 —— app 域页签容器）
 *
 * 为什么需要：旧宿主靠 `MobileSwipeContainer` 在外层按 `data-swipe-zone` /
 * `data-zone-at-*` 仲裁嵌套横滑区（内层到边界后继续同向滑动交外层翻主页面）。
 * 该容器已随票 2026-10-09 阶段 B 退役删除，而内层区（任务工具箱的双页签）
 * 仍在按旧约定声明边界——仲裁责任因此落到 app 域自持。
 *
 * 机制（模块级单例，全应用一份）：
 * - 外层容器注册 `setSwipeDelegate`：内层到边界时把翻页交回外层
 * - 内层越界时调 `delegateSwipe(dir)`：外层在场则翻页并返回 true（已消费），
 *   外层不在场返回 false（调用方自行决定忽略）
 *
 * 为什么不用 DOM 标记：内层区的边界是随其自身页签变化的状态，用事件目标反查
 * 需要在 touchstart 才知道方向，而方向只有 touchend 才知道——起点判定做不了
 * 边界放行。改为「内层越界时显式上交」，时机准确且不依赖 DOM 约定。
 */

/** 外层页签容器注册的翻页回调（dir = 滑动方向） */
type SwipeDelegate = (dir: 'left' | 'right') => void

let delegate: SwipeDelegate | null = null

/** 外层页签容器注册翻页回调（挂载时注册、卸载时置 null） */
export function setSwipeDelegate(next: SwipeDelegate | null): void {
  delegate = next
}

/** 当前是否有外层容器在场（供内层区分「越界忽略」与「上交外层」） */
export function hasSwipeDelegate(): boolean {
  return delegate !== null
}

/**
 * 内层区越界时上交外层翻页
 *
 * @returns true = 外层已消费本次滑动；false = 外层不在场，调用方按「越界忽略」处理
 */
export function delegateSwipe(dir: 'left' | 'right'): boolean {
  if (!delegate) return false
  delegate(dir)
  return true
}