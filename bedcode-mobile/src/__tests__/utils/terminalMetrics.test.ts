/**
 * 终端字体度量辅助的纯逻辑测试
 *
 * 覆盖两件与「中文间距 / 右侧黑带」直接相关的事：
 * - isBundledFontInLayout：判定内置字体是否真的进入了排版（不是「已下载」）
 * - bustFontFamilyCache：强制 xterm 失效重测的字体串（同字体不同串）
 */
import { describe, it, expect } from 'vitest'
import {
  bustFontFamilyCache,
  ensureTerminalFontLoaded,
  isBundledFontInLayout,
  TERMINAL_RIGHT_RESERVE_PX,
} from '@/utils/terminalMetrics'

describe('isBundledFontInLayout', () => {
  it('推进宽不同 → 字体已进入排版（内置 0.5em vs fallback 0.6em）', () => {
    // 真机实测：fontSize 15 → 内置 7.5px/字符，系统等宽 fallback 9.0px/字符
    expect(isBundledFontInLayout(7.5, 9)).toBe(true)
  })

  it('推进宽相同 → 还在 fallback 上（load() 已 resolve 但排版未换）', () => {
    // 变异：把此分支改成 true 会让终端在字体换上之前就开始测量
    expect(isBundledFontInLayout(9, 9)).toBe(false)
  })

  it('亚像素级差异（< 0.01px）视为同一套字形，不误判为已就位', () => {
    expect(isBundledFontInLayout(7.5, 7.505)).toBe(false)
    expect(isBundledFontInLayout(7.5, 7.52)).toBe(true)
  })

  it('退化量测（0 / 负数 / NaN）一律判未就绪，不放行到错误度量', () => {
    expect(isBundledFontInLayout(0, 0)).toBe(false)
    expect(isBundledFontInLayout(7.5, 0)).toBe(false)
    expect(isBundledFontInLayout(-1, 9)).toBe(false)
    expect(isBundledFontInLayout(NaN, 9)).toBe(false)
    expect(isBundledFontInLayout(7.5, NaN)).toBe(false)
  })
})

describe('bustFontFamilyCache', () => {
  it('产生与原串不同的字符串（否则 xterm WidthCache 不会 clear）', () => {
    const family = '"Sarasa Mono SC", monospace'
    const busted = bustFontFamilyCache(family)
    expect(busted).not.toBe(family)
    expect(busted.startsWith(family)).toBe(true)
  })

  it('CSS 语义等价（去空白后与原串一致）→ 渲染字体不变，只触发重测', () => {
    const family = '"Sarasa Mono SC", monospace, "Cascadia Mono"'
    const normalized = (s: string) => s.replace(/\s+/g, ' ').trim()
    expect(normalized(bustFontFamilyCache(family))).toBe(normalized(family))
  })

  it('连续调用幂等（重复赋值不再改变串，避免反复触发全量重绘）', () => {
    const family = 'monospace'
    expect(bustFontFamilyCache(bustFontFamilyCache(family))).toBe(bustFontFamilyCache(family))
  })
})

describe('TERMINAL_RIGHT_RESERVE_PX', () => {
  it('行尾右缘不预留（锁 0：预留即右侧竖直黑带）', () => {
    expect(TERMINAL_RIGHT_RESERVE_PX).toBe(0)
  })
})

describe('ensureTerminalFontLoaded（无 document.fonts 时的降级）', () => {
  it('环境不支持 document.fonts → 返回 false 且不抛（按 fallback 栈继续）', async () => {
    const original = Object.getOwnPropertyDescriptor(document, 'fonts')
    // happy-dom 默认没有 document.fonts；显式置空以锁定「不支持即降级」语义
    Object.defineProperty(document, 'fonts', { value: undefined, configurable: true })
    try {
      await expect(ensureTerminalFontLoaded(15)).resolves.toBe(false)
    } finally {
      if (original) Object.defineProperty(document, 'fonts', original)
      else Reflect.deleteProperty(document, 'fonts')
    }
  })
})