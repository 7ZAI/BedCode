/**
 * 终端网格行列数 DPR 感知计算（对齐 VS Code getXtermScaledDimensions 语义）
 *
 * 为什么需要：fit addon 用「容器 CSS 尺寸 / 字体 cell CSS 尺寸」做整数地板，
 * 不感知 devicePixelRatio。高分屏（Windows 150%/200% 缩放、多屏 DPI 变化）下
 * 行列数会因 CSS 像素与物理像素换算偏差而不精确，导致文字模糊或行尾截断。
 *
 * 纯逻辑模块（Seam A）：零 DOM/零 GPU 依赖，仅数值计算；cols/rows 的换算
 * 完全可单测（100%/150%/200% 缩放）。ceil 行高、floor 列宽保证网格不溢出：
 * 列宽向下取整避免行尾截断，行高向上取整避免最后一行放不下被裁切。
 */

export interface XtermScaledDimensionsInput {
  /** 容器 CSS 宽度（px） */
  containerWidthCss: number
  /** 容器 CSS 高度（px） */
  containerHeightCss: number
  /** xterm 实测 cell CSS 宽度（px，来自渲染服务 dimensions.css.cell） */
  cellWidthCss: number
  /** xterm 实测 cell CSS 高度（px） */
  cellHeightCss: number
  /** window.devicePixelRatio（1/1.25/1.5/2/…） */
  devicePixelRatio: number
}

/**
 * 按 devicePixelRatio 精确计算终端网格 cols/rows。
 *
 * 换算口径与 VS Code 一致：容器宽高 × DPR 得到可用物理像素；cell 宽 × DPR
 * 得到物理字符宽度（BedCode 无 letterSpacing，缺省为 0）；cell 高 × DPR 向上
 * ceil 后作为物理行高基准。列宽 floor、行数线性 floor，分别防截断与防溢出。
 *
 * @returns 恒为合法维度（≥1）；入参退化（≤0/非有限数）时返回 {1,1} 兜底，
 *          与调用方 applyDprFit 的优雅降级（回退 fitAddon.fit()）互补。
 */
export function getXtermScaledDimensions(
  input: XtermScaledDimensionsInput,
): { cols: number; rows: number } {
  const { containerWidthCss, containerHeightCss, cellWidthCss, cellHeightCss, devicePixelRatio } =
    input
  if (
    !isFinite(containerWidthCss) ||
    !isFinite(containerHeightCss) ||
    !isFinite(cellWidthCss) ||
    !isFinite(cellHeightCss) ||
    containerWidthCss <= 0 ||
    containerHeightCss <= 0 ||
    cellWidthCss <= 0 ||
    cellHeightCss <= 0
  ) {
    return { cols: 1, rows: 1 }
  }

  // 可用物理像素 = CSS 像素 × DPR（xterm 经 canvas 换算，必须乘 DPR 才精确）
  const scaledWidthAvailable = containerWidthCss * devicePixelRatio
  const scaledHeightAvailable = containerHeightCss * devicePixelRatio

  // 物理字符宽度：cell 宽 × DPR（+ letterSpacing，BedCode 为 0）；列宽向下取整防行尾截断
  const scaledCharWidth = cellWidthCss * devicePixelRatio
  const cols = Math.max(Math.floor(scaledWidthAvailable / scaledCharWidth), 1)

  // 物理字符高度：cell 高 × DPR 向上取整→ceil 行高保证最后一行放得下不被裁切
  const scaledCharHeight = Math.ceil(cellHeightCss * devicePixelRatio)
  const rows = Math.max(Math.floor(scaledHeightAvailable / scaledCharHeight), 1)

  return { cols, rows }
}
