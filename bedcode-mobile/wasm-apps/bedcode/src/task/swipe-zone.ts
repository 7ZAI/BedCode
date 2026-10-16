/**
 * 任务页横滑区「横向滚动容器」判定（AutoTaskToolboxView 专用）
 *
 * 背景（实测缺陷，2026-10-14）：纵向滚动容器的 overflow-x 会被 CSS 规范计算为 auto
 * （overflow-y 可滚动时兄弟轴 visible 强算为 auto），任务列表里无空格长串（日志片段 /
 * URL / 错误码）横向溢出 6px（chromium 实测）即被旧实现判为「可横向滚动」→ 任务页
 * zone 内横滑整轮 skip（内层不切页签、外层不翻主页面）→ 表现为「左右滑动失效」。
 *
 * 判据：纵向滚动容器（scrollHeight > clientHeight）排除；其余容器仅当横向溢出
 * ≥24px（实体横排内容）才算横向滚动区——24px 容差同时抗 DPR 亚像素舍入。
 */

/**
 * 触摸目标到横滑区根之间是否存在真正的横向滚动容器
 *
 * @param target touchstart 的事件目标（shouldSkip 入参，见 SDK useSwipeTabs）
 * @param root 横滑区根元素（att-tab-panels）；祖先链走到 root 即停止（不含 root）
 */
export function insideHorizontallyScrollable(target: EventTarget | null, root: HTMLElement | null): boolean {
  let el = (target as HTMLElement | null)?.parentElement ?? null
  while (el && el !== root) {
    // 纵向滚动容器（内容高度超过可视高度）不是横向滚动区：跳过其自身，继续向上查
    if (el.scrollHeight > el.clientHeight) {
      el = el.parentElement
      continue
    }
    const overflowX = getComputedStyle(el).overflowX
    if (overflowX === 'auto' || overflowX === 'scroll') {
      // 24px 容差：抗无空格长串的微小溢出与真机 DPR 亚像素舍入
      if (el.scrollWidth > el.clientWidth + 24) return true
    }
    el = el.parentElement
  }
  return false
}