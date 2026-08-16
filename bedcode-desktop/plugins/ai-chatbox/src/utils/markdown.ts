/**
 * 流式 Markdown 未闭合标记补偿（纯函数，无 DOM / 运行时依赖）
 *
 * 模型输出逐 chunk 到达时，文本经常以未闭合的代码围栏（fence）或行内码收尾。
 * 直接喂给 marked 的后果：
 * - 未闭合 fence 会把其后全部文本吞进代码块，流式期间布局随内容增长跳动，
 *   语言标签/复制按钮头部也被错误注入到仍在生长的块上；
 * - 行尾未闭合的行内码会与下一行开头的反引号 run 跨行配对（marked 行内码
 *   允许含换行），把中间整行吞成 code span。
 * 补偿在解析前把文本修补为闭合形态，使每帧渲染的都是"完整"文本——下一帧
 * 基于全量新文本重新补偿，追加的闭合标记不会残留。
 *
 * 检测规则刻意与 marked 的 fence 解析（CommonMark 子集）对齐，对拍测试保证：
 * - 开口 fence：行首 0-3 空格 + 3+ 反引号或波浪线；反引号 fence 的 info
 *   string 不得含反引号（"```js`" 这类行 marked 不按 fence 解析，须同样排除），
 *   波浪线 fence 的 info 无此限制（"~~~js~" 合法，marked 原生规则）
 * - 闭合行：行首 0-3 空格 + 与开口同字符且不少于开口长度的 run + 可选 ~/` 尾
 *   + 仅空格（marked 的 \1 反引用 + [~`]* 尾规则：跨字符不闭合、可长不可短，
 *   如开口 3 个、闭合 "````" 或 "```~" 均合法）
 * - 行内码：行内反引号 run 按等长配对，不同长度 run 视为字面量；
 *   只补偿长度 1-2 的 run——正文里的 ``` / `` 字面量是 fence 提及而非行内码
 *
 * 已知边界（维持 marked 原生行为，不补偿）：
 * - blockquote / 列表内的 fence（如 "> ```"）与 4 空格缩进代码块
 */

/** fenced 代码块状态（供延迟高亮与头部注入判定） */
export interface FenceBlock {
  /** 语言（与 marked 渲染的 language-* 类一致：info string 首词，无则空串） */
  lang: string
  /** 是否已闭合（开口与闭合 fence 配对完整） */
  closed: boolean
}

/**
 * 开口 fence 行：0-3 空格 + 3+ 反引号（lookahead 排除 info 含反引号，与 marked 一致）
 * 或 3+ 波浪线（info 无限制，marked 原生规则）
 */
const FENCE_OPEN_RE = /^ {0,3}(`{3,}(?=[^`\n]*(?:\n|$))|~{3,})(.*)$/
/** 闭合行：0-3 空格 + 3+ 反引号/波浪线 + 可选 ~/` 尾 + 仅空格（marked 的 [~`]* 尾规则） */
const FENCE_CLOSE_RE = /^ {0,3}(`{3,}|~{3,})[~`]* *$/

/** 换行归一化：marked lexer 入口同样把 \r\n / \r 归一为 \n，避免 CRLF 文本下
 * 此处判未闭合而 marked 判闭合（闭合块被跳过注入） */
function normalizeNewlines(text: string): string {
  return text.replace(/\r\n?/g, '\n')
}

/** marked 对 info string 的规范化：trim + 反斜杠转义取消，再取首词作为 language 类 */
function extractLang(info: string): string {
  return (
    info
      .trim()
      .replace(/\\([\p{P}\p{S}])/gu, '$1')
      .match(/^\S*/)?.[0] ?? ''
  )
}

/** 扫描文本中的 fenced 代码块（开口/闭合状态机，规则与 marked 对齐） */
export function getClosedCodeBlocks(text: string): FenceBlock[] {
  const lines = normalizeNewlines(text).split('\n')
  const blocks: FenceBlock[] = []
  let inFence = false
  let fenceChar = ''
  let fenceLen = 0
  let lang = ''
  for (const line of lines) {
    if (!inFence) {
      const m = FENCE_OPEN_RE.exec(line)
      if (m) {
        inFence = true
        fenceChar = m[1][0]
        fenceLen = m[1].length
        lang = extractLang(m[2])
      }
    } else {
      const m = FENCE_CLOSE_RE.exec(line)
      // 闭合须与开口同字符（marked 的 \1 反引用：跨字符不闭合）且长度 >= 开口；
      // "```x" 这类带尾随文本的行只是块内容
      if (m && m[1][0] === fenceChar && m[1].length >= fenceLen) {
        blocks.push({ lang, closed: true })
        inFence = false
      }
    }
  }
  if (inFence) {
    blocks.push({ lang, closed: false })
  }
  return blocks
}

/** 补偿单行行尾未闭合的行内码：返回补上等长闭合 run 后的行（无需补偿时原样返回） */
function patchInlineCode(line: string): string {
  // 无反引号的行原样返回：每帧全量补偿时省一次 O(n) run 扫描
  if (!line.includes('`')) return line
  // 收集行内反引号 run（长度 + 结束下标，end 为开区间）
  const runs: { len: number; end: number }[] = []
  for (let i = 0; i < line.length; ) {
    if (line[i] !== '`') {
      i++
      continue
    }
    let j = i
    while (j < line.length && line[j] === '`') j++
    runs.push({ len: j - i, end: j })
    i = j
  }
  if (runs.length === 0) return line
  // 等长 run 配对的状态机：首个 run 开口，后续等长 run 闭合，不等长视为字面量
  let openLen = 0
  for (const r of runs) {
    if (openLen === 0) {
      openLen = r.len
    } else if (r.len === openLen) {
      openLen = 0
    }
  }
  if (openLen === 0) return line
  // 行尾本身是反引号 run 时无法追加（相邻 run 会合并变长，越补越坏），跳过——
  // 下一 chunk 到达时文本自然闭合
  if (runs[runs.length - 1].end >= line.length) return line
  // 只补偿 1-2 反引号的常规行内码；3+ run 在正文里基本是 fence 字面量提及
  if (openLen > 2) return line
  return line + '`'.repeat(openLen)
}

/** 把未闭合的 fence / 行尾行内码修补为闭合形态（已闭合文本原样返回） */
export function patchIncompleteMarkdown(text: string): string {
  const lines = normalizeNewlines(text).split('\n')
  const out: string[] = []
  let inFence = false
  let fenceChar = ''
  let fenceLen = 0
  for (const line of lines) {
    if (!inFence) {
      const m = FENCE_OPEN_RE.exec(line)
      if (m) {
        inFence = true
        fenceChar = m[1][0]
        fenceLen = m[1].length
        out.push(line)
      } else {
        // 块外的行才做行内码补偿（fence 内容里反引号是字面量）
        out.push(patchInlineCode(line))
      }
    } else {
      const m = FENCE_CLOSE_RE.exec(line)
      if (m && m[1][0] === fenceChar && m[1].length >= fenceLen) {
        inFence = false
      }
      out.push(line)
    }
  }
  // 文本以未闭合 fence 收尾：追加与开口同字符的等长闭合行（长度须 >= 开口长度，直接用开口长度）
  if (inFence) {
    out.push(fenceChar.repeat(fenceLen))
  }
  return out.join('\n')
}
