/**
 * markdown 预处理单测（接缝 2）
 *
 * 补偿正确性（奇数 fence / 行尾未闭合行内码）、补偿幂等、闭合块检测
 * （含正文 ``` 字面量干扰）；检测规则与真实 marked 解析对拍（marked 为
 * 本插件实际渲染器，规则对齐以它为准）。
 */
import { describe, it, expect } from 'vitest'
import { marked } from 'marked'
import { getClosedCodeBlocks, patchIncompleteMarkdown } from '../utils/markdown'

/** 从真实 marked 词法结果提取 fenced 块信息（lang 取首词，与渲染的 language-* 类一致） */
function markedFencedBlocks(text: string): { lang: string; closed: boolean }[] {
  const fenced = (t: unknown) => t as { type: string; lang?: unknown; raw: string }
  return marked
    .lexer(text)
    // fenced 块带 lang（可能为空串），缩进代码块无 lang 属性——据此区分
    .filter(t => t.type === 'code' && typeof fenced(t).lang === 'string')
    .map(t => {
      const raw = fenced(t).raw
      const lines = raw.split('\n')
      // 开口 run（反引号/波浪线）：闭合判定按 marked 的 \1 反引用语义，闭合行
      // 必须以开口 run 原样开头（同字符、长度 >= 开口），其后可跟任意 ~/` 再仅空格
      const openRun = /^ {0,3}(`{3,}|~{3,})/.exec(lines[0])?.[1]
      // 去掉开口 fence 行后才是内容行：末行为闭合 fence 行才算已闭合
      // （raw 可能带尾换行；raw 仅开口行本身时 rest 为空 → 未闭合）
      const rest = lines.slice(1)
      const last = rest[rest.length - 1] === '' ? rest[rest.length - 2] : rest[rest.length - 1]
      const closed =
        last !== undefined &&
        openRun !== undefined &&
        // run 只含反引号/波浪线（非正则元字符），可直接拼接
        new RegExp('^ {0,3}' + openRun + '[~`]* *$').test(last)
      return { lang: String(fenced(t).lang).match(/^\S*/)?.[0] ?? '', closed }
    })
}

/** 与 marked 规则一致的语料（对拍用；blockquote/列表 fence 为已知边界，不入语料）
 *
 * 含刻意分歧用例：3~ 闭合行不能闭合 4~ 开口、~~~ 行不能闭合反引号 fence
 * （marked 的 \1 反引用跨字符不闭合）、CRLF 换行（marked lexer 入口归一化） */
const ALIGNED_CORPUS = [
  '```py\ncode\n```',
  '```py\ncode',
  'text\n```js extra\ncode\n```',
  '```\nx\n```\ntext\n```js\ny',
  '````\n```\n````',
  '```\n````\n```',
  '```\ncode```x\nmore',
  '```\ncode\n```  ',
  '```js\ncode\n```~',
  '```js`\ncode',
  '``` is a fence\nmore',
  '这里有三反引号 ``` 字面量\nmore',
  '    ```\ncode',
  '\t```\ncode',
  'text `abc`\n```py\nx\n```\n`def`',
  'plain text no fences',
  '',
  // 波浪线 fence（marked 原生语法）
  '~~~js\ncode\n~~~',
  '~~~js\ncode',
  '~~~js~\ncode',
  '~~~js`\ncode',
  '~~~\n```\n~~~',
  // 分歧用例：跨字符不闭合 / 闭合长度不足 / 4 开口 3 闭合
  '```\nx\n~~~\n```',
  '~~~~\ncode\n~~~',
  '````\ncode\n```',
  // CRLF：与 marked lexer 入口的 \r\n 归一化保持一致
  '```js\r\ncode\r\n```',
  'text\r\n```js\ny',
]

describe('patchIncompleteMarkdown 补偿正确性', () => {
  it('奇数个 fence：末尾追加闭合行', () => {
    expect(patchIncompleteMarkdown('```py\ncode')).toBe('```py\ncode\n```')
    expect(patchIncompleteMarkdown('text\n```\ncode')).toBe('text\n```\ncode\n```')
    // 多块场景：只补最后一个未闭合块
    expect(patchIncompleteMarkdown('```a\nx\n```\n```b\ny')).toBe('```a\nx\n```\n```b\ny\n```')
  })

  it('偶数个 fence（已闭合）：原样返回', () => {
    expect(patchIncompleteMarkdown('```\nx\n```')).toBe('```\nx\n```')
    expect(patchIncompleteMarkdown('```py\ncode\n```')).toBe('```py\ncode\n```')
  })

  it('闭合 fence 长度与开口对齐（4 反引号开口须 4 反引号闭合）', () => {
    expect(patchIncompleteMarkdown('````\nx')).toBe('````\nx\n````')
    // 3 反引号行不能闭合 4 反引号开口的块（marked 闭合规则），末尾仍需补 4 个
    expect(patchIncompleteMarkdown('````\n```\nx')).toBe('````\n```\nx\n````')
    // 3 开口 + 4 闭合合法：第一块闭合、第二块未闭合 → 只补末尾
    expect(patchIncompleteMarkdown('```\n````\n```')).toBe('```\n````\n```\n```')
  })

  it('波浪线 fence：未闭合补等长波浪线闭合行', () => {
    expect(patchIncompleteMarkdown('~~~js\ncode')).toBe('~~~js\ncode\n~~~')
    // 跨字符不闭合：反引号 fence 里的 ~~~ 行是内容，不参与闭合判定
    expect(patchIncompleteMarkdown('```\nx\n~~~\n```')).toBe('```\nx\n~~~\n```')
    // 3~ 闭合行不能闭合 4~ 开口（marked \1 反引用），末尾仍补 4 个
    expect(patchIncompleteMarkdown('~~~~\ncode\n~~~')).toBe('~~~~\ncode\n~~~\n~~~~')
  })

  it('CRLF 文本：换行归一化为 \\n 后补偿（与 marked lexer 入口一致）', () => {
    expect(patchIncompleteMarkdown('```py\r\ncode')).toBe('```py\ncode\n```')
    // 已闭合的 CRLF 文本归一化后原样返回
    expect(patchIncompleteMarkdown('```py\r\ncode\r\n```')).toBe('```py\ncode\n```')
  })

  it('行尾未闭合行内码：补上等长闭合 run', () => {
    expect(patchIncompleteMarkdown('text `abc')).toBe('text `abc`')
    expect(patchIncompleteMarkdown('a `b` c `d')).toBe('a `b` c `d`')
    expect(patchIncompleteMarkdown('``abc')).toBe('``abc``')
    // 跨行配对预防：行尾开口 run 与下一行首 run 配对会把中间内容吞成 code span
    expect(patchIncompleteMarkdown('text `abc\n`def`')).toBe('text `abc`\n`def`')
  })

  it('已闭合行内码与 fence 内反引号：原样返回', () => {
    expect(patchIncompleteMarkdown('text `abc`\nmore')).toBe('text `abc`\nmore')
    expect(patchIncompleteMarkdown('a ``b`` c')).toBe('a ``b`` c')
    // fence 内容里的反引号是字面量，不做行内补偿
    expect(patchIncompleteMarkdown('```\n`abc\ncode`\n```')).toBe('```\n`abc\ncode`\n```')
  })

  it('行尾本身是反引号 run：不补（相邻 run 会合并变长，越补越坏）', () => {
    expect(patchIncompleteMarkdown('text `')).toBe('text `')
    expect(patchIncompleteMarkdown('`a`b`')).toBe('`a`b`')
    expect(patchIncompleteMarkdown('``a`')).toBe('``a`')
  })

  it('正文 fence 字面量（3+ run）：不误补', () => {
    expect(patchIncompleteMarkdown('use ``` in docs\nnext')).toBe('use ``` in docs\nnext')
    expect(patchIncompleteMarkdown('这里有三反引号 ``` 字面量\nmore')).toBe(
      '这里有三反引号 ``` 字面量\nmore',
    )
  })
})

describe('patchIncompleteMarkdown 幂等与无假阳性', () => {
  it('补偿幂等：二次补偿结果不变', () => {
    for (const t of ALIGNED_CORPUS) {
      const once = patchIncompleteMarkdown(t)
      expect(patchIncompleteMarkdown(once)).toBe(once)
    }
  })

  it('已闭合文本不被改动（无假阳性）', () => {
    const closedTexts = [
      '```py\ncode\n```',
      'text `abc`\nmore',
      'a ``b`` c',
      '```\n`abc\ncode`\n```',
      'use ``` in docs\nnext',
      '这里有三反引号 ``` 字面量\nmore',
      '    ```\ncode',
      'plain text',
      'text `\nnext',
    ]
    for (const t of closedTexts) {
      expect(patchIncompleteMarkdown(t)).toBe(t)
    }
  })
})

describe('getClosedCodeBlocks 闭合块检测', () => {
  it('块语言 / 闭合状态', () => {
    expect(getClosedCodeBlocks('```py\ncode\n```')).toEqual([{ lang: 'py', closed: true }])
    expect(getClosedCodeBlocks('```py\ncode')).toEqual([{ lang: 'py', closed: false }])
    expect(getClosedCodeBlocks('```a\nx\n```\n```js\ny')).toEqual([
      { lang: 'a', closed: true },
      { lang: 'js', closed: false },
    ])
    expect(getClosedCodeBlocks('```')).toEqual([{ lang: '', closed: false }])
    expect(getClosedCodeBlocks('plain text')).toEqual([])
    expect(getClosedCodeBlocks('')).toEqual([])
  })

  it('语言提取与 marked 的 language-* 类一致（info 首词 + 转义取消）', () => {
    expect(getClosedCodeBlocks('```js extra\nx\n```')[0].lang).toBe('js')
    expect(getClosedCodeBlocks('```\nx\n```')[0].lang).toBe('')
    expect(getClosedCodeBlocks('``` js\nx\n```')[0].lang).toBe('js')
    expect(getClosedCodeBlocks('```js,foo\nx\n```')[0].lang).toBe('js,foo')
    expect(getClosedCodeBlocks('```js\\,x\nx\n```')[0].lang).toBe('js,x')
  })

  it('干扰排除：正文字面量 / info 含反引号 / 缩进 / blockquote fence', () => {
    // 正文中间的反引号 run 不是 fence（fence 必须在行首）
    expect(getClosedCodeBlocks('正文里 ``` 字面量\nmore')).toEqual([])
    expect(getClosedCodeBlocks('use ``` in docs\nnext')).toEqual([])
    // info string 含反引号时 marked 不按反引号 fence 解析（如 "```js`"）
    expect(getClosedCodeBlocks('```js`\ncode')).toEqual([])
    // 4 空格 / tab 缩进是缩进代码块，不是 fence
    expect(getClosedCodeBlocks('    ```\ncode')).toEqual([])
    expect(getClosedCodeBlocks('\t```\ncode')).toEqual([])
    // blockquote 内 fence 为已知边界：本期不识别，维持 marked 原生行为
    expect(getClosedCodeBlocks('> ```\n> code')).toEqual([])
  })

  it('波浪线 fence：开口/闭合/字符与长度规则（与 marked 的 \\1 反引用一致）', () => {
    expect(getClosedCodeBlocks('~~~js\ncode\n~~~')).toEqual([{ lang: 'js', closed: true }])
    expect(getClosedCodeBlocks('~~~js\ncode')).toEqual([{ lang: 'js', closed: false }])
    // 波浪线 fence 的 info 允许含波浪线/反引号（marked 仅反引号 fence 限制 info）
    expect(getClosedCodeBlocks('~~~js~\ncode')[0].lang).toBe('js~')
    expect(getClosedCodeBlocks('~~~js`\ncode')[0].lang).toBe('js`')
    // 跨字符不闭合：~~~ 行是反引号 fence 的内容，反之亦然
    expect(getClosedCodeBlocks('```\nx\n~~~\n```')).toEqual([{ lang: '', closed: true }])
    expect(getClosedCodeBlocks('~~~\n```\n~~~')).toEqual([{ lang: '', closed: true }])
    // 闭合长度须 >= 开口（3~ 行不能闭合 4~ 开口的块）
    expect(getClosedCodeBlocks('~~~~\ncode\n~~~')).toEqual([{ lang: '', closed: false }])
  })

  it('CRLF 文本：归一化后闭合判定与 marked 一致', () => {
    expect(getClosedCodeBlocks('```py\r\ncode\r\n```')).toEqual([{ lang: 'py', closed: true }])
    expect(getClosedCodeBlocks('```py\r\ncode')).toEqual([{ lang: 'py', closed: false }])
  })
})

describe('与真实 marked 对拍', () => {
  it('闭合块检测与 marked 词法一致（语言与闭合状态）', () => {
    for (const t of ALIGNED_CORPUS) {
      const ours = getClosedCodeBlocks(t).map(b => ({ lang: b.lang, closed: b.closed }))
      expect(ours).toEqual(markedFencedBlocks(t))
    }
  })

  it('补偿后 marked 解析无未闭合块（每帧渲染的都是闭合形态）', () => {
    for (const t of ALIGNED_CORPUS) {
      const blocks = markedFencedBlocks(patchIncompleteMarkdown(t))
      expect(blocks.every(b => b.closed)).toBe(true)
    }
  })
})
