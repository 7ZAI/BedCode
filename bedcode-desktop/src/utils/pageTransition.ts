/**
 * 页面过渡动效 —— 全端唯一切换点（代码级，无设置项）
 *
 * 桌面端所有「整视图 / 分区切换」（宿主路由 + 四个 wasm 应用）共用宿主
 * `src/style.css` 里的 `.page-*` + `.page-swap` 过渡体系；效果差异只由本模块
 * 决定：把选定效果写到 `<html data-page-fx="...">`，CSS 按该属性挑一组
 * 入场起点 / 出场终点。改这里一个常量即可全端换效果。
 *
 * 为什么不做成用户设置：页面过渡是应用级观感，不是用户偏好；做成设置项会给
 * 「每个用户看到不同的切换手感」留下空间，也就没有单一事实源可守。当前需求
 * 明确只要代码层面切换。
 */

/** 可选效果。顺序即文档顺序，新增效果须同时在 `src/style.css` 补对应规则。 */
export const PAGE_TRANSITION_EFFECTS = ['fade', 'slide-up', 'slide-left', 'zoom'] as const

export type PageTransitionEffect = (typeof PAGE_TRANSITION_EFFECTS)[number]

/** 缺省效果：无 `data-page-fx` 属性时 CSS 也按它渲染，两者必须一致。 */
export const DEFAULT_PAGE_TRANSITION_EFFECT: PageTransitionEffect = 'slide-up'

/** 当前生效效果。改这一行 = 全端换一种页面过渡。 */
export const PAGE_TRANSITION_EFFECT: PageTransitionEffect = DEFAULT_PAGE_TRANSITION_EFFECT

/**
 * 把效果写到根元素上。必须在首次绘制前调用（`main.ts` 里 `app.mount` 之前），
 * 否则首屏会先按 CSS 缺省效果渲染一帧再切。
 *
 * 效果名写错时**抛错而不是静默回退**：CSS 侧没有兜底分支，一个不存在的效果名
 * 会让过渡类全部落空（瞬时硬切，无任何动效），静默回退只会把这类改动永远藏住。
 *
 * 错误文案用 ASCII（task 2026-10-04 OCR D-06）：本函数在 `app.mount` 之前同步
 * 执行，vue-i18n 尚未初始化，i18n key 在这里解析不出可读文本；且 throw 的载荷
 * 会进控制台，中文字符串在部分终端/日志编码下乱码。
 *
 * @param effect 效果名；缺省取 `PAGE_TRANSITION_EFFECT`
 * @throws {Error} 效果名不在 `PAGE_TRANSITION_EFFECTS` 内（启动路径在
 *   `main.ts` catch；测试路径保留 throw 以精确断言）
 */
export function applyPageTransitionEffect(
  root: HTMLElement,
  effect: PageTransitionEffect = PAGE_TRANSITION_EFFECT,
): void {
  if (!(PAGE_TRANSITION_EFFECTS as readonly string[]).includes(effect)) {
    throw new Error(
      `unknown page transition effect "${effect}"; available: ${PAGE_TRANSITION_EFFECTS.join(' / ')}`,
    )
  }
  root.dataset.pageFx = effect
}
