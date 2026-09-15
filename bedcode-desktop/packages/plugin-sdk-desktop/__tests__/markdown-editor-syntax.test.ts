import { describe, it, expect } from 'vitest'
import { applyMarkdownSyntax } from '../src/ui/markdown-editor-syntax'

/**
 * MarkdownEditor 语法插入纯函数测试
 *
 * 只测外部行为：每种动作 ×（无选区 / 有选区 / 光标在行中）的
 * 文本变换结果与选区落点，不测内部实现细节。
 */
describe('applyMarkdownSyntax', () => {
  describe('inline 动作：无选区 → 模板+占位选中', () => {
    it('bold：插入 **bold** 并选中占位', () => {
      const r = applyMarkdownSyntax('bold', 'hello world', 5, 5)
      expect(r.text).toBe('hello**bold** world')
      expect(r.selectionStart).toBe(7)
      expect(r.selectionEnd).toBe(11)
    })

    it('italic：插入 *italic* 并选中占位', () => {
      const r = applyMarkdownSyntax('italic', 'abc', 3, 3)
      expect(r.text).toBe('abc*italic*')
      expect(r.selectionStart).toBe(4)
      expect(r.selectionEnd).toBe(10)
    })

    it('codeInline：插入 `code` 并选中占位', () => {
      const r = applyMarkdownSyntax('codeInline', 'abc', 1, 1)
      expect(r.text).toBe('a`code`bc')
      expect(r.selectionStart).toBe(2)
      expect(r.selectionEnd).toBe(6)
    })

    it('link：插入 [link](https://) 并选中 link 占位', () => {
      const r = applyMarkdownSyntax('link', 'abc', 3, 3)
      expect(r.text).toBe('abc[link](https://)')
      expect(r.selectionStart).toBe(4)
      expect(r.selectionEnd).toBe(8)
    })
  })

  describe('inline 动作：有选区 → 包裹选区', () => {
    it('bold：包裹选区，选区保留在包裹内部', () => {
      const r = applyMarkdownSyntax('bold', 'hello world', 0, 5)
      expect(r.text).toBe('**hello** world')
      expect(r.selectionStart).toBe(2)
      expect(r.selectionEnd).toBe(7)
    })

    it('italic：包裹选区', () => {
      const r = applyMarkdownSyntax('italic', 'hello world', 6, 11)
      expect(r.text).toBe('hello *world*')
      expect(r.selectionStart).toBe(7)
      expect(r.selectionEnd).toBe(12)
    })

    it('codeInline：包裹选区', () => {
      const r = applyMarkdownSyntax('codeInline', 'use foo here', 4, 7)
      expect(r.text).toBe('use `foo` here')
      expect(r.selectionStart).toBe(5)
      expect(r.selectionEnd).toBe(8)
    })

    it('link：包裹选区并把光标/选区放到 URL 处待填', () => {
      const r = applyMarkdownSyntax('link', 'see doc', 4, 7)
      expect(r.text).toBe('see [doc](https://)')
      expect(r.selectionStart).toBe(10)
      expect(r.selectionEnd).toBe(18)
    })
  })

  describe('heading 行级动作：光标所在行', () => {
    it('h1：无前缀行加 # 前缀，光标保持在行内', () => {
      const r = applyMarkdownSyntax('h1', 'title\nbody', 3, 3)
      expect(r.text).toBe('# title\nbody')
      expect(r.selectionStart).toBe(5)
    })

    it('h1：已有 ## 前缀时替换为目标级别（先移除再插入）', () => {
      const r = applyMarkdownSyntax('h1', '## title', 4, 4)
      expect(r.text).toBe('# title')
    })

    it('h2：h1 前缀逐级切换为 h2', () => {
      const r = applyMarkdownSyntax('h2', '# title', 5, 5)
      expect(r.text).toBe('## title')
    })

    it('h3：已有 ### 前缀时移除（toggle 语义，回到正文）', () => {
      const r = applyMarkdownSyntax('h3', '### title', 6, 6)
      expect(r.text).toBe('title')
    })

    it('光标在行中（非行首）时作用于整行且光标跟随前缀偏移', () => {
      const r = applyMarkdownSyntax('h2', 'hello world', 6, 6)
      expect(r.text).toBe('## hello world')
      expect(r.selectionStart).toBe(9)
    })
  })

  describe('list / olist / quote 行级动作', () => {
    it('list：加 - 前缀，再次点击移除（toggle）', () => {
      const once = applyMarkdownSyntax('list', 'item', 0, 0)
      expect(once.text).toBe('- item')
      const twice = applyMarkdownSyntax('list', once.text, once.selectionStart, once.selectionEnd)
      expect(twice.text).toBe('item')
    })

    it('olist：加 1. 前缀，再次点击移除', () => {
      const once = applyMarkdownSyntax('olist', 'item', 2, 2)
      expect(once.text).toBe('1. item')
      const twice = applyMarkdownSyntax('olist', once.text, once.selectionStart, once.selectionEnd)
      expect(twice.text).toBe('item')
    })

    it('quote：加 > 前缀，再次点击移除', () => {
      const once = applyMarkdownSyntax('quote', 'cited', 3, 3)
      expect(once.text).toBe('> cited')
      const twice = applyMarkdownSyntax('quote', once.text, once.selectionStart, once.selectionEnd)
      expect(twice.text).toBe('cited')
    })
  })

  describe('codeBlock', () => {
    it('无选区：围栏包裹光标所在行', () => {
      const r = applyMarkdownSyntax('codeBlock', 'let x = 1', 4, 4)
      expect(r.text).toBe('```\nlet x = 1\n```')
      expect(r.selectionStart).toBe(8)
      expect(r.selectionEnd).toBe(8)
    })

    it('有选区：围栏包裹选区（选区保留）', () => {
      const r = applyMarkdownSyntax('codeBlock', 'const a = 1\nconst b = 2', 0, 23)
      expect(r.text).toBe('```\nconst a = 1\nconst b = 2\n```')
      expect(r.selectionStart).toBe(4)
      expect(r.selectionEnd).toBe(27)
    })
  })

  describe('divider', () => {
    it('光标行之后插入独占一行的 ---，光标移到分隔线后', () => {
      const r = applyMarkdownSyntax('divider', 'part one\npart two', 5, 5)
      expect(r.text).toBe('part one\n\n---\npart two')
      expect(r.selectionStart).toBe(14)
      expect(r.selectionEnd).toBe(14)
    })

    it('光标在空行时不叠加空行', () => {
      const r = applyMarkdownSyntax('divider', 'a\n\nb', 2, 2)
      expect(r.text).toBe('a\n\n---\nb')
    })

    it('末尾空行插入时补前置空行，避免 setext H2 误解析', () => {
      // 回归："hello\n---\n" 会被 marked 解析为 <h2>hello</h2>
      const r = applyMarkdownSyntax('divider', 'hello\n', 6, 6)
      expect(r.text).toBe('hello\n\n---\n')
      expect(r.selectionStart).toBe(11)
      expect(r.selectionEnd).toBe(11)
    })

    it('末尾已有空行时不叠加空行', () => {
      const r = applyMarkdownSyntax('divider', 'a\n\n', 3, 3)
      expect(r.text).toBe('a\n\n---\n')
    })

    it('空文本光标在行首：直接插入 --- 行', () => {
      const r = applyMarkdownSyntax('divider', '', 0, 0)
      expect(r.text).toBe('---\n')
    })
  })

  describe('边界与选区方向', () => {
    it('逆序选区（selectionStart > selectionEnd）自动收敛为正向', () => {
      const r = applyMarkdownSyntax('bold', 'hello world', 5, 0)
      expect(r.text).toBe('**hello** world')
    })

    it('越界选区收敛到文本边界（无选区时按光标插入模板）', () => {
      const r = applyMarkdownSyntax('bold', 'hi', 10, 99)
      expect(r.text).toBe('hi**bold**')
    })
  })
})
