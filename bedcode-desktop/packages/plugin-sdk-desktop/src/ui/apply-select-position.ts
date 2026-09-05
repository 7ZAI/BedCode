/**
 * Select 面板 DOM 定位封装（定位引擎的 DOM 接入层）
 *
 * 职责：把「视觉坐标测量 → 设计空间计算 → 设计 px 写回」三步收敛为一个入口，
 * Select.vue 展开面板时调用 applySelectPanelPosition() 即可，不感知缩放细节。
 * 翻转/收缩/夹持规则见 select-position.ts（纯函数，规则表驱动单测）。
 *
 * zoom 不变式（WebKitGTK 2.52.6 根 zoom 1.15 实测，F 自校准见 zoom-compensation.ts）：
 * - gBCR 读数 = 设计值 × F（视觉坐标）
 * - DOM 写回 px 渲染 = 设计值 × F（与触发器同一坐标系）
 * - 故：读数 ÷ F → 纯函数（设计空间）→ 写回计算结果。
 *   无 zoom 环境 F=1，全部换算退化为恒等，与非 Linux 端行为逐位一致。
 *
 * 注：评估过 Floating UI 1.8.0，其内部在根 zoom 下混用 gBCR（视觉值）与
 * offsetHeight（设计值）计算位置（实测 flip 上翻 y 偏差 36.6 视觉 px，输出
 * 补偿无法修复），故保留本自校准实现。
 */
import { computeSelectPosition, SELECT_MAX_PANEL_HEIGHT } from './select-position'
import { getFixedZoomCompensation } from './zoom-compensation'

/** 参与定位的面板三要素 */
export interface SelectPanelElements {
  /** 触发器（定位参照物） */
  trigger: HTMLElement
  /** 面板定位层（fixed；写入 top/left/width） */
  panel: HTMLElement
  /** 面板内选项列表（写入 maxHeight，并作为自然高测量目标） */
  list: HTMLElement
}

/** 面板设计高度上限（转发自纯函数模块，组件层不重复定义） */
export { SELECT_MAX_PANEL_HEIGHT }

/**
 * 测量并定位下拉面板（同步；调用时机 = 面板可见后的同一帧内）
 *
 * 写入均为设计 px：无 zoom 端与历史行为一致；Linux 端渲染时 ×F 恰好
 * 落回触发器的视觉坐标系，面板与触发器严丝合缝。
 */
export function applySelectPanelPosition(elements: SelectPanelElements): void {
  const { trigger, panel, list } = elements
  const f = getFixedZoomCompensation()

  // 先定宽再测高：面板宽度与触发器一致（视觉宽 ÷F），长选项标签的换行
  // 高度才能在测量时生效，避免首开时按未定宽的自然宽误判面板高度
  const rect = trigger.getBoundingClientRect()
  panel.style.width = `${rect.width / f}px`

  // 解除上次收缩残留的 maxHeight 再测自然高（同步块内完成，无中间渲染）：
  // 否则翻转判定与 maxHeight 会按上次收缩后的高度失真。
  // 退回值必须在 ÷F 之后兜底（视觉 0 ÷F 仍为 0），否则 zoom 端会误得 240/F
  list.style.maxHeight = ''
  const measuredHeight = panel.getBoundingClientRect().height / f
  const naturalHeight = measuredHeight || SELECT_MAX_PANEL_HEIGHT

  const pos = computeSelectPosition(
    { top: rect.top / f, bottom: rect.bottom / f, left: rect.left / f, width: rect.width / f },
    { width: window.innerWidth / f, height: window.innerHeight / f },
    naturalHeight,
  )

  panel.style.top = `${pos.top}px`
  panel.style.left = `${pos.left}px`
  list.style.maxHeight = `${pos.maxHeight}px`
}
