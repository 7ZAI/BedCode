/**
 * 代码高亮引擎 seam（ADR-0011 双引擎对比实验）
 *
 * 渲染管线只依赖 HighlightEngine 接口，引擎选择与管线逻辑解耦：
 * 桌面端注入 hljs 同步实现（现有 CSS token 低饱和配色沿用），
 * 移动端注入 Shiki 异步实现（P4，懒加载单例 + 深浅色双主题）。
 */
import hljs from 'highlight.js'

/** 高亮引擎契约：对已闭合代码块产出高亮（就地操作渲染产物 DOM）；
 * 允许异步实现（P4 Shiki 懒加载回填），同步实现返回 void，管线忽略返回值 */
export interface HighlightEngine {
  /** 就地高亮一个 code 元素（已闭合代码块的渲染产物） */
  highlightElement(code: HTMLElement): void | Promise<void>
}

/** hljs 实现（桌面端注入；异常输入降级纯文本，不影响消息渲染） */
export function createHljsHighlightEngine(): HighlightEngine {
  return {
    highlightElement(code) {
      try {
        hljs.highlightElement(code)
      } catch {
        // 异常输入降级为无高亮纯文本
      }
    },
  }
}
