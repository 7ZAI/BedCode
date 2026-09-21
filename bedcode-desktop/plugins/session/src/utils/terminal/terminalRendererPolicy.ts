/**
 * 终端渲染器与透明度决策（纯逻辑模块，Seam A）
 *
 * 为什么需要这个模块：addon-webgl 0.19.0 对 `allowTransparency` 没有运行时
 * 监听（`_setTransparency` 是零调用的死代码），渲染层的 alpha 标志与 canvas
 * 的 `{ alpha }` 属性在 `getContext` 之后不可变。因此"是否透明 / 用哪个
 * 渲染器"必须在初始化时一次性定死，后续切换靠 dispose + 重建渲染器实现
 * （spec D-1）。判定逻辑抽成纯函数后可以单测，这类渲染缺陷才不会以隐性
 * 方式回归。
 *
 * 为什么把透明与渲染器绑在一起决策：背景图开启时 DOM 渲染器透明天然正确
 * （无 texture atlas、无 alpha 分支、无帧缓冲优化），两条残影通路
 * （alpha 帧缓冲 + 部分行更新、无条件透明的 .xterm-viewport）一次切断；
 * 而 WebGL 在这场景下即便修好 alpha 切换也根治不了 scrollback 透明洞。
 * 见 spec D-2 的路线 B（默认）。
 */

/** 渲染器路线：'A'（背景图保留 WebGL + 透明重连）/'B'（背景图强制 DOM，recommended） */
export type RendererRoute = 'A' | 'B'

/** decideRenderer 的输入：平台、背景图状态、Linux 是否强制 DOM 渲染器（route A 才用到） */
export interface DecideRendererInput {
  isLinux: boolean
  hasBackgroundImage: boolean
  linuxUseDomRenderer: boolean
  /** 默认 'B'（spec D-2 建议路线） */
  route?: RendererRoute
}

/** decideRenderer 的输出：三个布尔互相约束（useDom XOR useWebgl），用对象返回便于断言单字段 */
export interface RendererDecision {
  useWebgl: boolean
  allowTransparency: boolean
  useDom: boolean
}

/**
 * 决策渲染器与透明度。
 *
 * route 'B'（默认）：`hasBackgroundImage` 与 `(isLinux && linuxUseDomRenderer)`
 * 任一成立即用 DOM 渲染器 —— 背景图场景走 DOM 让透明天然正确、直接切断残影；
 * Linux 无画布渲染场景保持既有 `LINUX_USE_DOM_RENDERER` 事实选择（Linux DOM
 * 无 atlas，atlas 预热对其是 no-op）。`allowTransparency = hasBackgroundImage`：
 * DOM 渲染器下透明由 DOM 层实现，改动即时生效，不依赖 canvas alpha。
 *
 * route 'A'：仅 `(isLinux && linuxUseDomRenderer)` 用 DOM，其余一律 WebGL，
 * 透明按背景图开——保留大输出吞吐，代价是把透明残影问题留给 D-1 重建修复，
 * 且 scrollback 透明洞无法根除（spec D-2）。
 *
 * 为什么不能由调用方分别推导：两个开关（渲染器、透明度）强耦合，历史上散落
 * 在组件里的条件式推导正是"只改 allowTransparency + refresh"错误修法的来源，
 * 收敛到这里一次定义清楚。
 */
export function decideRenderer({
  isLinux,
  hasBackgroundImage,
  linuxUseDomRenderer,
  route = 'B',
}: DecideRendererInput): RendererDecision {
  if (route === 'A') {
    const useDom = isLinux && linuxUseDomRenderer
    return {
      useWebgl: !useDom,
      allowTransparency: hasBackgroundImage,
      useDom,
    }
  }

  const useDom = hasBackgroundImage || (isLinux && linuxUseDomRenderer)
  return {
    useWebgl: !useDom,
    allowTransparency: hasBackgroundImage,
    useDom,
  }
}

/**
 * atlas 预热的初始帧预算（帧）。
 *
 * 为什么用帧而不是固定延时（替代 `ATLAS_PREHEAT_DELAY_MS = 700`）：图集重建
 * 后 `warmUp()` 只预热 ASCII 33~125，非 ASCII 字形按 IdleTaskQueue 分片异步
 * 光栅化；`beginFrame()` 在 atlas 页合并时触发全量重绘，因此 rAF 迭代刷新能
 * 自然跟上光栅化进度，低配机不再猜小、高性能机不再空等。8 帧 ≈ 133ms @60fps，
 * 是从 idle 队列首批任务 + 一次页合并的经验值，属可调参数（spec D-4、ticket 03）。
 */
export const ATLAS_PREHEAT_FRAME_BUDGET = 8

/**
 * 计算下一次 rAF 迭代的剩余帧预算。
 *
 * 每次迭代消费 1 帧（返回 prev - 1），返回 0 表示迭代停止。传 0 或负数都
 * 返回 0 —— 上层用返回值作循环前进条件，一旦预算被外部状态污染（如组件
 * 销毁、会话切换后残留的计时器继续回调），必须立刻收敛到停止而非继续
 * 递减或死循环。
 */
export function decideAtlasRefreshFrames(prev: number): number {
  if (prev <= 0) {
    return 0
  }
  return prev - 1
}