/**
 * 页签面板导航规则（纯函数，无 DOM）
 * -----------------------------------------------------------------------------
 * 运行面的四个页签面板常驻挂载（保留滚动位置与订阅），切换靠「停靠位 → 居中」的
 * 位移过渡完成。本文件回答两个问题：**某张面板此刻该停在哪一侧**（停靠位）、
 * **横滑一步该去哪一页**（相邻页）。都不碰 DOM，便于直接钉行为契约。
 *
 * 形态口径（同屏只允许一页可见）：非当前页完全停在屏外一侧，不做半屏叠放——
 * 半屏停靠会让两页同时可见，面板自身又无背景，视觉上就是「页面重叠」。
 * 几何真源在 `app.css`（停靠位 translateX(±100%) + 容器裁切），本文件只出类名。
 *
 * 为什么单独成文件：AppRoot.vue 里的判定掺着生命周期与手势，单测挂不动；
 * 抽成纯函数后停靠与取邻规则可被直接钉住（正例 / 反例 / 边界）。
 */
import type { AppTab } from './useAppTabs'

/** 停靠位 → app.css 里的类名 */
export const ACTIVE_DOCK = 'app-tab-panel--active'
export const LEFT_DOCK = 'app-tab-panel--left'
export const RIGHT_DOCK = 'app-tab-panel--right'

export interface DockInput {
  /** 待判定的面板 */
  id: AppTab
  /** 当前页签 */
  active: AppTab
  /** 页签视觉顺序（升序） */
  order: readonly AppTab[]
  /**
   * 入场起点侧：新挂载的面板在「切到居中」之前先停在这一侧。
   * 非入场（已挂载面板的常驻停靠）传 null。
   */
  enterFrom?: 'left' | 'right' | null
}

/**
 * 面板停靠位
 *
 * 判定优先级：入场 > 当前页 > 相对位置。
 * 入场优先是必须的——新面板此刻**就是**当前页，若按「当前页 → 居中」渲染，
 * 浏览器看不到「停靠 → 居中」的变化（新挂载元素不产生过渡），切页会变成硬切。
 *
 * 越界输入（id 不在 order 里）：indexOf 为 -1 会被判成「已过页」而停左侧，
 * 属不可达状态——面板只按页签集合挂载，集合本身就是 order 的真源。
 */
export function dockClassOf(input: DockInput): string {
  if (input.enterFrom) return input.enterFrom === 'left' ? LEFT_DOCK : RIGHT_DOCK
  if (input.id === input.active) return ACTIVE_DOCK
  const from = input.order.indexOf(input.id)
  const to = input.order.indexOf(input.active)
  return from < to ? LEFT_DOCK : RIGHT_DOCK
}

/** 新面板从哪一侧入场：目标在当前页右侧 → 从右滑入（反之从左） */
export function enterSideOf(id: AppTab, active: AppTab, order: readonly AppTab[]): 'left' | 'right' {
  return order.indexOf(id) > order.indexOf(active) ? 'right' : 'left'
}

/**
 * 横滑一步的相邻页签
 *
 * @param order 页签视觉顺序（可用性过滤后的呈现集合）
 * @param activeIndex 当前页在 order 中的序号
 * @param dir 'left' = 向后翻（下一页） / 'right' = 向前翻（上一页）
 * @returns 目标页签；越界（首屏再向前 / 末页再向后）返回 null，调用方保持当前页不变
 */
export function neighborTabId(
  order: readonly AppTab[],
  activeIndex: number,
  dir: 'left' | 'right',
): AppTab | null {
  const next = activeIndex + (dir === 'left' ? 1 : -1)
  return order[next] ?? null
}
