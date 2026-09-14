/**
 * MarkdownEditor 语法插入纯函数
 *
 * 工具栏各动作对 (text, selectionStart, selectionEnd) 的纯变换，返回新文本与新光标/选区。
 * 不依赖 DOM / vue-i18n，便于单元测试与在任意编辑器环境复用。
 *
 * 动作语义（i18n 中立的刻意取舍：占位文本内置英文，组件 title 可经 labels props 覆盖）：
 * - inline 动作（bold / italic / codeInline / link）：
 *   无选区 → 插入带占位文本的模板并选中占位；有选区 → 包裹选区。
 * - 行级动作（h1 / h2 / h3 / list / olist / quote）：
 *   作用于光标所在行，行首前缀 toggle（已有同族前缀则移除，否则插入；
 *   heading 族统一「先移除任意级别 # 前缀、再插目标级别」，支持逐级切换）。
 * - codeBlock：无选区 → 用 ``` 围栏包裹光标所在行；有选区 → 包裹选区。
 * - divider：在光标所在行之后插入独占一行的 --- 分隔线。
 */
export type MarkdownSyntaxAction =
  | 'bold'
  | 'italic'
  | 'h1'
  | 'h2'
  | 'h3'
  | 'list'
  | 'olist'
  | 'quote'
  | 'codeBlock'
  | 'codeInline'
  | 'link'
  | 'divider'

export interface SyntaxResult {
  text: string
  selectionStart: number
  selectionEnd: number
}

/** inline 包裹动作模板：open + placeholder + close（占位内置英文，保持 SDK i18n 中立） */
const INLINE_ACTIONS: Record<'bold' | 'italic' | 'codeInline', { open: string; close: string; placeholder: string }> = {
  bold: { open: '**', close: '**', placeholder: 'bold' },
  italic: { open: '*', close: '*', placeholder: 'italic' },
  codeInline: { open: '`', close: '`', placeholder: 'code' },
}

const LINE_HEADING = /^(#{1,6})\s+/
const LINE_LIST = /^-\s+/
const LINE_OLIST = /^\d+\.\s+/
const LINE_QUOTE = /^>\s+/

/** 行级前缀 toggle：已有则移除（返回负 delta），否则插入 */
function togglePrefix(line: string, removeRe: RegExp, insert: string): { line: string; delta: number } {
  const m = line.match(removeRe)
  if (m) {
    const removed = m[0].length
    return { line: line.slice(removed), delta: -removed }
  }
  return { line: insert + line, delta: insert.length }
}

/** heading：无前缀 → 插入目标级别；有同级别前缀 → 移除（toggle 回正文）；有异级别前缀 → 替换为目标级别 */
function setHeading(line: string, level: 1 | 2 | 3): { line: string; delta: number } {
  const prefix = '#'.repeat(level) + ' '
  const m = line.match(LINE_HEADING)
  if (!m) return { line: prefix + line, delta: prefix.length }
  if (m[1].length === level) return { line: line.slice(m[0].length), delta: -m[0].length }
  return { line: prefix + line.slice(m[0].length), delta: prefix.length - m[0].length }
}

/** 光标所在行边界（选区存在时以光标所在行 = selectionStart 所在行为准） */
function lineBounds(text: string, position: number): { start: number; end: number } {
  const start = text.lastIndexOf('\n', position - 1) + 1
  let end = text.indexOf('\n', position)
  if (end === -1) end = text.length
  return { start, end }
}

/** inline 包裹（无选区 → 模板+占位选中；有选区 → 包裹选区） */
function wrapInline(
  text: string,
  s: number,
  e: number,
  open: string,
  close: string,
  placeholder: string,
): SyntaxResult {
  if (s === e) {
    const insert = open + placeholder + close
    return {
      text: text.slice(0, s) + insert + text.slice(s),
      selectionStart: s + open.length,
      selectionEnd: s + open.length + placeholder.length,
    }
  }
  return {
    text: text.slice(0, s) + open + text.slice(s, e) + close + text.slice(e),
    selectionStart: s + open.length,
    selectionEnd: e + open.length,
  }
}

/** 行级前缀动作：作用于光标所在行，光标保持行内相对位置 */
function applyLinePrefix(
  text: string,
  s: number,
  transform: (line: string) => { line: string; delta: number },
): SyntaxResult {
  const { start, end } = lineBounds(text, s)
  const line = text.slice(start, end)
  const { line: newLine, delta } = transform(line)
  const rel = Math.min(Math.max(s - start + delta, 0), newLine.length)
  return {
    text: text.slice(0, start) + newLine + text.slice(end),
    selectionStart: start + rel,
    selectionEnd: start + rel,
  }
}

/** 代码块：无选区 → 围栏包裹光标所在行；有选区 → 包裹选区 */
function applyCodeBlock(text: string, s: number, e: number): SyntaxResult {
  const fence = '```'
  if (s === e) {
    const { start, end } = lineBounds(text, s)
    const insert = fence + '\n'
    return {
      text: text.slice(0, start) + insert + text.slice(start, end) + '\n' + fence + text.slice(end),
      selectionStart: s + insert.length,
      selectionEnd: s + insert.length,
    }
  }
  const insert = fence + '\n'
  return {
    text: text.slice(0, s) + insert + text.slice(s, e) + '\n' + fence + text.slice(e),
    selectionStart: s + insert.length,
    selectionEnd: e + insert.length,
  }
}

/** 链接：无选区 → [link](https://) 选中 link；有选区 → [选区](https://) 选中 URL 待填 */
function applyLink(text: string, s: number, e: number): SyntaxResult {
  const url = 'https://'
  if (s === e) {
    const placeholder = 'link'
    const insert = `[${placeholder}](${url})`
    return {
      text: text.slice(0, s) + insert + text.slice(s),
      selectionStart: s + 1,
      selectionEnd: s + 1 + placeholder.length,
    }
  }
  const selected = text.slice(s, e)
  const insert = `[${selected}](${url})`
  const urlStart = s + selected.length + 3
  return {
    text: text.slice(0, s) + insert + text.slice(e),
    selectionStart: urlStart,
    selectionEnd: urlStart + url.length,
  }
}

/**
 * 分隔线：在光标行位置插入独占一行的 ---（保证 --- 前有空行，避免被解析为 setext 标题）
 * - 非空行：行后插入「空行 + --- + 换行」
 * - 空行：光标行变成 ---（保留前空行结构）
 */
function applyDivider(text: string, s: number): SyntaxResult {
  const { start, end } = lineBounds(text, s)
  const line = text.slice(start, end)
  if (line.trim()) {
    const hasNewline = end < text.length
    const insertAt = hasNewline ? end + 1 : text.length
    const insert = hasNewline ? '\n---\n' : '\n\n---\n'
    const newText = text.slice(0, insertAt) + insert + text.slice(insertAt)
    const newSel = insertAt + insert.length
    return { text: newText, selectionStart: newSel, selectionEnd: newSel }
  }
  // 空行行：行前补空行再放 ---（后随原行尾换行），或末尾直接追加 ---\n
  if (end < text.length) {
    const newText = text.slice(0, start) + '\n---' + text.slice(end)
    const newSel = start + 4
    return { text: newText, selectionStart: newSel, selectionEnd: newSel }
  }
  // 末行空行：需保证 --- 与上一非空行之间有空行——否则 "hello\n---\n" 会被
  // marked 解析为 setext H2 标题（实测 <h2>hello</h2>）。仅当上一行非空时补
  // 前置空行；上一行已是空行（如 "a\n\n"）则维持原结构不叠空行。
  const prevBounds = start > 0 ? lineBounds(text, start - 1) : null
  const prevLine = prevBounds ? text.slice(prevBounds.start, prevBounds.end).trim() : ''
  const needGap = prevBounds !== null && prevLine !== ''
  const insert = needGap ? '\n---\n' : '---\n'
  const newText = text.slice(0, start) + insert + text.slice(end)
  const newSel = start + insert.length
  return { text: newText, selectionStart: newSel, selectionEnd: newSel }
}

/**
 * 对 (text, selectionStart, selectionEnd) 应用一次工具栏语法动作。
 * 输入区间按 text 长度收敛；输出保证 selectionStart <= selectionEnd 且不越界。
 */
export function applyMarkdownSyntax(
  action: MarkdownSyntaxAction,
  text: string,
  selectionStart: number,
  selectionEnd: number,
): SyntaxResult {
  // 各自收敛到文本边界后再归一化方向（逆序选区自动纠正）
  const rawS = Math.min(Math.max(selectionStart, 0), text.length)
  const rawE = Math.min(Math.max(selectionEnd, 0), text.length)
  const s = Math.min(rawS, rawE)
  const e = Math.max(rawS, rawE)

  switch (action) {
    case 'bold':
    case 'italic':
    case 'codeInline': {
      const { open, close, placeholder } = INLINE_ACTIONS[action]
      return wrapInline(text, s, e, open, close, placeholder)
    }
    case 'link':
      return applyLink(text, s, e)
    case 'codeBlock':
      return applyCodeBlock(text, s, e)
    case 'divider':
      return applyDivider(text, s)
    case 'h1':
      return applyLinePrefix(text, s, (line) => setHeading(line, 1))
    case 'h2':
      return applyLinePrefix(text, s, (line) => setHeading(line, 2))
    case 'h3':
      return applyLinePrefix(text, s, (line) => setHeading(line, 3))
    case 'list':
      return applyLinePrefix(text, s, (line) => togglePrefix(line, LINE_LIST, '- '))
    case 'olist':
      return applyLinePrefix(text, s, (line) => togglePrefix(line, LINE_OLIST, '1. '))
    case 'quote':
      return applyLinePrefix(text, s, (line) => togglePrefix(line, LINE_QUOTE, '> '))
  }
}
