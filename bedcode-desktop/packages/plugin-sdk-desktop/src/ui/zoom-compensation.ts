/**
 * fixed 定位覆盖层在根元素 CSS zoom 下的坐标换算
 *
 * 背景：Linux 桌面端 style.css 给 html.platform-linux 加了 zoom: 1.15（修正
 * WebKitGTK 字号观感）。WebKitGTK 2.52 采用标准化 zoom 语义（对齐 Chrome 128+）：
 * - getBoundingClientRect() 返回视觉坐标（已含 zoom 放大）；
 * - 而给 fixed 元素赋 top/left/width 等长度值时，渲染会再次乘以祖先链 zoom。
 * 两者叠加：把 gBCR 读数直接赋给 fixed 面板，会向右下漂移 (zoom-1)×坐标、
 * 尺寸放大 zoom 倍（WebKitGTK 2.52.6 实测：F=1.15，左缘漂 20.7px、宽 +15%，
 * 贴底场景面板下溢窗口底约 39px——用户视角即"没贴住触发器、上拉判定失效"）。
 *
 * 解法：自校准换算因子 F。放一个 set 宽 100px 的 fixed 探针读回 gBCR 宽度，
 * F = 读回值 / 100。核心不变式：视觉坐标 = 赋值 px × F = 设计坐标 × F——
 * gBCR 读数 ÷ F 得设计坐标；赋值用设计坐标（赋值 px 渲染时 ×F，与触发器
 * 同一坐标系）。若选择全程在视觉空间计算（如 Tooltip），则最终赋值 = 视觉值 ÷ F。
 * 两端引擎下都自洽：
 * - 标准化 zoom 引擎：F = zoom，gBCR(视觉)/F → 设计 px，赋值渲染 ×zoom 落回视觉位置；
 * - 旧式 zoom 引擎（used-value 乘法、gBCR 报本地坐标）：F = 1，读数本就是设计 px；
 * - 无 zoom 环境（macOS/Windows）：F = 1，行为与历史实现完全一致。
 * 页面缩放（Ctrl+=/-）对读数与赋值同比例缩放，不影响 F；根 zoom 每次打开时
 * 重新实测一次，开销量级为单次 gBCR 调用，可忽略。
 */

let probeEl: HTMLDivElement | null = null

/**
 * 获取当前文档下 fixed 赋值坐标 → 视觉坐标的换算因子（赋值 px = 设计 px / F）
 *
 * 探针元素常驻 body（不可见、不可交互）；测试框架清空 body 后下次调用自动重挂载。
 */
export function getFixedZoomCompensation(): number {
  if (!probeEl) {
    probeEl = document.createElement('div')
    probeEl.setAttribute('aria-hidden', 'true')
    probeEl.style.cssText =
      'position:fixed;top:0;left:0;width:100px;height:0;visibility:hidden;pointer-events:none;'
  }
  if (!probeEl.isConnected) {
    document.body.appendChild(probeEl)
  }
  const width = probeEl.getBoundingClientRect().width
  // 无布局引擎的环境（happy-dom/jsdom 测试）gBCR 恒为 0 → 退回 1，保持无缩放行为
  return Number.isFinite(width) && width > 0 ? width / 100 : 1
}

/** 重置探针缓存（测试隔离用；生产代码无需调用） */
export function resetFixedZoomProbe(): void {
  probeEl = null
}
