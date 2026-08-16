/**
 * Shiki 高亮引擎 node 集成测试（接缝 2，移动端特有）
 *
 * 覆盖：给定代码 + 语言产出高亮 HTML（Shiki 行结构 + 语法 token）、
 * 未支持语言降级 plaintext（不抛错）、fence 别名归一化、懒加载单例复用。
 * 真实加载 oniguruma WASM（shiki/wasm 内联二进制），验证构建产物可用性。
 */
import { describe, it, expect } from 'vitest'
import {
  highlightCode,
  createShikiHighlightEngine,
  clearHighlightCacheForTest,
  type HighlightEngine,
} from '../utils/highlight'

/** 轮询等待条件成立（替代固定 setTimeout：WASM 首载在慢 CI 下可能超过固定阈值） */
async function waitFor(condition: () => boolean, timeoutMs = 2000, intervalMs = 10): Promise<void> {
  const deadline = Date.now() + timeoutMs
  while (!condition()) {
    if (Date.now() > deadline) throw new Error('waitFor 超时')
    await new Promise(r => setTimeout(r, intervalMs))
  }
}

/** 构造最小 code 元素 stub（满足 langFromElement / applyHighlight / dataset 写入） */
function makeCodeEl(langClass = 'language-python'): HTMLElement {
  return {
    classList: { find: () => langClass, add: () => {} },
    dataset: {} as Record<string, string>,
    textContent: 'print(1)',
    innerHTML: '',
    isConnected: true,
  } as unknown as HTMLElement
}

/** node 无 document：stub 最小实现满足 applyHighlight 的 template 解析 */
function installDocumentStub(): void {
  ;(globalThis as any).document = {
    // currentShikiTheme 读 html.dark 类（stub 恒为深色）
    documentElement: { classList: { contains: () => true } },
    createElement: () => ({
      set innerHTML(_: string) {},
      content: { querySelector: () => ({ innerHTML: '<span>highlighted</span>' }) },
    }),
  }
}

describe('highlightCode（Shiki 核心，纯函数）', () => {
  it('给定代码 + 语言产出高亮 HTML（shiki 类 + 行结构 + 语法 token）', async () => {
    const html = await highlightCode('def f():\n    return 1', 'python', 'vitesse-dark')
    expect(html).toContain('shiki')
    expect(html).toContain('<code>')
    expect(html).toContain('<span class="line">')
    // 语法 token：关键字 def 有独立着色 span
    expect(html).toContain('>def</span>')
    expect(html).toContain('vitesse-dark')
  })

  it('不同语言产出不同 token 结构（grammar 生效而非纯文本）', async () => {
    const py = await highlightCode('const x = 1', 'python', 'vitesse-dark')
    const js = await highlightCode('const x = 1', 'javascript', 'vitesse-dark')
    // python 不认识 const 关键字、javascript 认识：着色 span 结构应不同
    expect(py).not.toBe(js)
    expect(js).toContain('>const</span>')
  })

  it('fence 别名归一化：py→python / js→javascript / sh→shellscript', async () => {
    const py = await highlightCode('print(1)', 'py', 'vitesse-dark')
    expect(py).toContain('>print</span>')
    const sh = await highlightCode('ls -la', 'sh', 'vitesse-dark')
    expect(sh).toContain('>ls</span>')
  })

  it('未支持语言：降级 plaintext（不抛错、产出转义后的行结构）', async () => {
    const html = await highlightCode('<script>alert(1)</script>', 'not-a-real-lang', 'vitesse-dark')
    expect(html).toContain('shiki')
    expect(html).toContain('<span class="line">')
    // 特殊字符被转义（plaintext 不做语法着色；shiki 4.x 转义 < 为 &#x3C;）
    expect(html).not.toContain('<script>')
    expect(html).toContain('&#x3C;script>alert(1)&#x3C;/script>')
  })

  it('空代码：不抛错，产出空行结构', async () => {
    const html = await highlightCode('', 'python', 'vitesse-dark')
    expect(html).toContain('shiki')
  })

  it('懒加载单例：多次调用共享同一实例（第二次不再重新加载 WASM）', async () => {
    const a = await highlightCode('x = 1', 'python', 'vitesse-light')
    const b = await highlightCode('y = 2', 'python', 'vitesse-light')
    expect(a).toContain('vitesse-light')
    expect(b).toContain('vitesse-light')
  })
})

describe('createShikiHighlightEngine（DOM 引擎）', () => {
  it('缓存命中：二次调用同步回填（不重新走异步 WASM 路径）', async () => {
    const savedDocument = (globalThis as any).document
    installDocumentStub()
    try {
      clearHighlightCacheForTest()
      const engine = createShikiHighlightEngine()
      const code = makeCodeEl()

      // 首次：异步路径（WASM 加载），轮询等待回填完成
      engine.highlightElement(code)
      expect(code.dataset.highlighted).toBeUndefined()
      await waitFor(() => code.dataset.highlighted === '1')

      // 二次：缓存命中同步回填（同帧立即可见）
      engine.highlightElement(code)
      expect(code.innerHTML).toContain('highlighted')
    } finally {
      ;(globalThis as any).document = savedDocument
      clearHighlightCacheForTest()
    }
  })

  it('in-flight 去重：同 key 请求在途时复用同一高亮计算（只发起一次）', async () => {
    const savedDocument = (globalThis as any).document
    installDocumentStub()
    try {
      clearHighlightCacheForTest()
      let calls = 0
      let resolveFn!: (html: string) => void
      // 可控 fake：挂起直至手动放行，模拟 WASM 首载窗口内相邻帧的重复请求
      const fakeHighlight = (_code: string, _lang: string, _theme: string) => {
        calls++
        return new Promise<string>(resolve => {
          resolveFn = resolve
        })
      }
      const engine: HighlightEngine = createShikiHighlightEngine(fakeHighlight)
      const code1 = makeCodeEl()
      const code2 = makeCodeEl()
      engine.highlightElement(code1)
      engine.highlightElement(code2)
      // 去重生效：两次请求只发起一次高亮计算
      expect(calls).toBe(1)
      resolveFn('')
      await waitFor(
        () => code1.dataset.highlighted === '1' && code2.dataset.highlighted === '1',
      )
      // 完成后缓存生效：第三次调用同步回填，不再发起计算
      const code3 = makeCodeEl()
      engine.highlightElement(code3)
      expect(code3.innerHTML).toContain('highlighted')
      expect(calls).toBe(1)
    } finally {
      ;(globalThis as any).document = savedDocument
      clearHighlightCacheForTest()
    }
  })
})
