/**
 * 渲染帧让出工具（TerminalView 拆分产物）
 *
 * 遮罩撤除/入场收尾等场景需要「末批内容已 commit 上屏」再继续：写入管线经 rAF
 * 合批，仅 await 一个微任务会让遮罩在内容上屏前淡出，透出逐批写入的闪烁过程。
 *
 * 测试环境（happy-dom 无 rAF）立即 resolve，避免用例悬挂。
 */
export function nextPaintFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => resolve())
    } else {
      resolve()
    }
  })
}
