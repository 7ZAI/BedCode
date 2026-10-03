/**
 * 会话聊天视图的 Markdown 子集渲染（纯函数，零依赖 / 零 DOM）
 *
 * 为什么自带渲染器而不是引 marked（README 票 B2）：
 * - **依赖面**：agent-hub 是**日志查看器**（只读、无流式、无输入框），为一个
 *   读记录的视图引入解析器 + sanitizer 两个运行时依赖不划算。双端 ai-chatbox
 *   各自已有 marked + DOMPurify，但那两处都在**流式对话主链路**上，诉求不同。
 * - **安全面**：本渲染器**先转义后拼标签**——输出只可能含本文件写下的白名单
 *   标签，原始文本的尖括号 / & / 引号全部实体化；链接不产生 href（宿主已撤
 *   shell/opener 权限且 CSP `connect-src 'none'`，可点链接只会变成骗人的死链），
 *   故无需 sanitizer 兜底，也不必信任任何第三方 HTML。
 *
 * 子集（够用即止，覆盖 agent 输出的常见形态）：
 * 段落（软换行 → br）/ ATX 标题 / 有序无序列表（含两级缩进嵌套）/ 引用 /
 * 分隔线 / 围栏代码块（反引号与波浪号）/ GFM 管道表格 /
 * 行内：行内码、粗体、斜体、删除线、链接（降级为不可点文本 + title 提示 URL）。
 *
 * 折叠态另走 {@link markdownPlainPreview}（剥标记的纯文本，配合 CSS clamp）：
 * 展开态才产出结构化 HTML——长消息默认折叠（B1）时根本不生成 HTML，省一次解析。
 */

/** HTML 实体表（覆盖标签定界与属性定界字符，防属性注入） */
const ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
}

/** 文本 / 属性值实体化（渲染器的唯一转义入口） */
export function escapeHtml(input: string): string {
  return input.replace(/[&<>"']/g, (c) => ESCAPES[c])
}

/** 换行归一化：JSONL 里的 CRLF 在块级判定（围栏 / 列表 / 表格）下必须先归一 */
function normalize(text: string): string {
  return text.replace(/\r\n?/g, '\n')
}

// ==================== 块级行判定 ====================

/** 围栏开口行：0-3 空格 + 3+ 反引号（info 不得含反引号）或 3+ 波浪号 */
const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})[ \t]*([^`\n]*)$/
/** 分隔线：3+ 相同的 - / * / _ */
const HR = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/
/** ATX 标题：可选闭合井号一并吃掉 */
const ATX = /^ {0,3}(#{1,6})[ \t]+(.*?)[ \t]*#*$/
/** 引用行 */
const QUOTE = /^ {0,3}>[ \t]?(.*)$/
/** 列表项：缩进 + 符号（- + * 或 1. 1)） + 空格 + 正文 */
const LIST_ITEM = /^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/
/** 表格分隔行（含竖线才是表） */
const TABLE_DELIM = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/

/**
 * 链接 / 图片语法：`[文字](目标)` 与 `![说明](目标)`
 *
 * 目标允许**一层括号**（`(?:[^()\s]|\([^()\s]*\))+`）：`[x](javascript:alert(1))`
 * 这类必须整体吃进来，否则只取到 `javascript:alert(1`、把尾括号漏成正文。
 */
const LINK = /\[([^\]]*)\]\(\s*((?:[^()\s]|\([^()\s]*\))+(?:\s+"[^"]*")?)\s*\)/g
/** 图片语法（同一目标形态，前缀多一个叹号） */
const IMAGE = /!\[([^\]]*)\]\(\s*((?:[^()\s]|\([^()\s]*\))+(?:\s+"[^"]*")?)\s*\)/g

/** 围栏闭合行：与开口同字符、长度 ≥ 开口、其后仅空白 */
function fenceCloseRe(char: string, len: number): RegExp {
  return new RegExp(`^ {0,3}${char}{${len},}[ \\t]*$`)
}

/** 缩进宽度（tab 按 4 空格计，列表层级判定用） */
function indentWidth(s: string): number {
  let w = 0
  for (const c of s) w += c === '\t' ? 4 : 1
  return w
}

/** 是否为块级起始（决定段落在哪里断开） */
function isBlockStart(lines: string[], i: number): boolean {
  const line = lines[i]
  if (!line.trim()) return true
  return (
    FENCE_OPEN.test(line) ||
    HR.test(line) ||
    ATX.test(line) ||
    QUOTE.test(line) ||
    LIST_ITEM.test(line) ||
    isTableStart(lines, i)
  )
}

/** 管道表格起始：当前行含竖线 + 下一行是「列数相同」的分隔行 */
function isTableStart(lines: string[], i: number): boolean {
  const head = lines[i]
  const delim = lines[i + 1]
  if (!head || !delim || !head.includes('|') || !delim.includes('|')) return false
  if (!TABLE_DELIM.test(delim)) return false
  return splitRow(delim).length === splitRow(head).length
}

/** 拆表格行：去首尾竖线后按未转义的竖线切列，转义竖线还原为字面竖线 */
function splitRow(row: string): string[] {
  return row
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split(/(?<!\\)\|/)
    .map((c) => c.trim().replace(/\\\|/g, '|'))
}

// ==================== 行内渲染 ====================

/**
 * 行内码占位符（NUL 包裹下标）
 *
 * 必须用**正文不可能出现的字符**兜住：占位符若只是裸下标，还原阶段的数字正则会
 * 把正文里的版本号 / 行号一并当成占位符替换成空串（本仓真实踩过：渲染后
 * 「升级到 2」变成「升级到 」）。NUL 由 {@link NUL} 运行时拼出，不落字面量。
 */
const NUL = String.fromCharCode(0)

function codeSlot(n: number): string {
  return `${NUL}${n}${NUL}`
}

/** 占位符还原（数字两侧夹 NUL，正文数字不会被误配） */
const CODE_SLOT_RE = new RegExp(`${NUL}(\\d+)${NUL}`, 'g')

/**
 * 行内标记渲染（优先级：行内码 > 链接/图片 > 强调）
 *
 * 行内码先抽成占位符再统一转义：既不会被强调 / 链接正则切进代码内容里，
 * 也不会把生成的标签再转义一次。
 */
export function renderInline(src: string): string {
  const codes: string[] = []
  let s = src.replace(/(`+)([^`]+?)\1/g, (_m, _run: string, code: string) => {
    codes.push(`<code>${escapeHtml(code.trim())}</code>`)
    return codeSlot(codes.length - 1)
  })
  s = escapeHtml(s)
  // 图片：只留 alt 文字（前端零资源访问红线：绝不产生 img src）
  s = s.replace(IMAGE, '$1')
  // 链接：降级为不可点文本，URL 挂 title 提示（无 href → 无 javascript: 面）
  s = s.replace(LINK, (_m, text: string, url: string) => {
    if (!url) return text
    return `<span data-md-link title="${url}">${text}</span>`
  })
  s = s
    .replace(/\*\*\*([^*]+)\*\*\*/g, '<strong><em>$1</em></strong>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^\w*])\*([^*\n]+)\*/g, '$1<em>$2</em>')
    .replace(/(^|[^\w_])__([^_\n]+)__/g, '$1<strong>$2</strong>')
    .replace(/(^|[^\w_])_([^_\n]+)_/g, '$1<em>$2</em>')
    .replace(/~~([^~]+)~~/g, '<del>$1</del>')
  // 还原行内码（NUL 占位符不受实体化影响）
  s = s.replace(CODE_SLOT_RE, (_m, n: string) => codes[Number(n)] ?? '')
  return s
}

// ==================== 块级渲染 ====================

/** 渲染整篇（空输入返回空串，调用方据此决定是否挂 markdown 类） */
export function renderMarkdown(src: string): string {
  if (!src.trim()) return ''
  return renderBlocks(normalize(src).split('\n'))
}

function renderBlocks(lines: string[]): string {
  const out: string[] = []
  let i = 0
  while (i < lines.length) {
    const line = lines[i]
    if (!line.trim()) {
      i++
      continue
    }
    const fence = FENCE_OPEN.exec(line)
    if (fence) {
      const marker = fence[1]
      const lang = fence[2].trim().split(/\s+/)[0] ?? ''
      const close = fenceCloseRe(marker[0], marker.length)
      const buf: string[] = []
      i++
      while (i < lines.length && !close.test(lines[i])) {
        buf.push(lines[i])
        i++
      }
      if (i < lines.length) i++ // 吃掉闭合行（未闭合时到文末自然结束）
      const attr = lang ? ` data-lang="${escapeHtml(lang)}"` : ''
      out.push(`<pre><code${attr}>${escapeHtml(buf.join('\n'))}</code></pre>`)
      continue
    }
    if (HR.test(line)) {
      out.push('<hr />')
      i++
      continue
    }
    const atx = ATX.exec(line)
    if (atx) {
      const lv = atx[1].length
      out.push(`<h${lv}>${renderInline(atx[2])}</h${lv}>`)
      i++
      continue
    }
    if (QUOTE.test(line)) {
      const buf: string[] = []
      while (i < lines.length) {
        const m = QUOTE.exec(lines[i])
        if (m) buf.push(m[1])
        else if (lines[i].trim() && buf.length) buf.push(lines[i].trim()) // 引用内懒续行
        else break
        i++
      }
      out.push(`<blockquote>${renderBlocks(buf)}</blockquote>`)
      continue
    }
    if (isTableStart(lines, i)) {
      const head = splitRow(lines[i])
      i += 2 // 表头 + 分隔行
      const body: string[][] = []
      while (i < lines.length && lines[i].trim() && lines[i].includes('|')) {
        const cells = splitRow(lines[i])
        // 列数不足补空、过多截断（宽容渲染，不丢整表）
        while (cells.length < head.length) cells.push('')
        body.push(cells.slice(0, head.length))
        i++
      }
      out.push(renderTable(head, body))
      continue
    }
    if (LIST_ITEM.test(line)) {
      const r = renderList(lines, i)
      out.push(r.html)
      i = r.next
      continue
    }
    // 段落：吃到空行或下一个块级起始；行内软换行渲染成 br
    const buf: string[] = []
    while (i < lines.length && lines[i].trim() && !isBlockStart(lines, i)) {
      buf.push(lines[i].trim())
      i++
    }
    if (!buf.length) {
      buf.push(lines[i].trim())
      i++
    }
    out.push(`<p>${buf.map(renderInline).join('<br />')}</p>`)
  }
  return out.join('')
}

/** 列表：从 start 起渲染一个列表块（同级聚合 + 更深缩进递归成子列表） */
function renderList(lines: string[], start: number): { html: string; next: number } {
  const first = LIST_ITEM.exec(lines[start])!
  const ordered = /\d/.test(first[2])
  const baseIndent = indentWidth(first[1])
  const items: string[] = []
  let i = start
  while (i < lines.length) {
    const m = LIST_ITEM.exec(lines[i])
    if (!m || indentWidth(m[1]) !== baseIndent) break
    const buf = [m[3]]
    i++
    while (i < lines.length) {
      const l = lines[i]
      if (!l.trim()) {
        // 空行后若仍是本列表（同级或更深）则继续，否则列表收尾
        let j = i
        while (j < lines.length && !lines[j].trim()) j++
        const nm = j < lines.length ? LIST_ITEM.exec(lines[j]) : null
        if (!nm || indentWidth(nm[1]) < baseIndent) break
        i = j
        continue
      }
      const nm = LIST_ITEM.exec(l)
      if (nm && indentWidth(nm[1]) === baseIndent) break // 同级下一项
      // 更深缩进的列表项 = 子列表（剥一级缩进后交给块级渲染去嵌套）；
      // 其余（缩进懒续行 / 段落续行）并入本项正文
      const drop = nm && indentWidth(nm[1]) > baseIndent ? baseIndent + 2 : 0
      buf.push(drop ? l.slice(Math.min(indentWidth(l), drop)) : l.trim())
      i++
    }
    items.push(`<li>${renderBlocks(buf)}</li>`)
  }
  const tag = ordered ? 'ol' : 'ul'
  return { html: `<${tag}>${items.join('')}</${tag}>`, next: i }
}

function renderTable(head: string[], body: string[][]): string {
  const th = head.map((c) => `<th>${renderInline(c)}</th>`).join('')
  const rows = body
    .map((cells) => `<tr>${cells.map((c) => `<td>${renderInline(c)}</td>`).join('')}</tr>`)
    .join('')
  return `<table><thead><tr>${th}</tr></thead><tbody>${rows}</tbody></table>`
}

// ==================== 折叠预览（剥标记的纯文本） ====================

/**
 * 折叠态预览文本：剥掉 markdown 标记，只留可读正文
 *
 * 与 {@link renderMarkdown} 分工：折叠预览交给 CSS line-clamp（对纯文本可靠），
 * 展开态才产出结构化 HTML——长消息默认收起时既不生成标签，也保留了 B1 已定的
 * 「3 行 + 省略号」预览手感。**不做字符级截断**（内容仍在 DOM 里，可选中可搜索）。
 *
 * **仍必须实体化**：调用方（`SessionLogsTab.vue`）对本函数的结果走 `v-html` 注入
 * ——折叠态是长消息（> `COLLAPSE_THRESHOLD_CHARS`）的**默认**路径，此前本函数
 * 只剥标记不转义，`<img onerror=…>` / `<svg onload=…>` 会被当 HTML 执行
 * （webview 内 `withGlobalTauri: true` → 可直接拿到 `invoke()` 面，等于绕过权限闸门）。
 * 转义放在剥标记**之前**（与 {@link renderInline} 同序），标记全是 markdown 字符，
 * 不受实体化影响；实体化后本函数仍只输出文本，不会产出任何标签。
 */
export function markdownPlainPreview(src: string): string {
  return escapeHtml(normalize(src))
    .split('\n')
    .map((line) => {
      let s = line
      s = s.replace(/^ {0,3}(?:`{3,}|~{3,})[ \t]*([^`\n]*)$/, '$1') // 围栏标记（保留 info）
      s = s.replace(/^ {0,3}#{1,6}[ \t]+/, '') // 标题井号
      s = s.replace(/^ {0,3}>[ \t]?/, '') // 引用
      s = s.replace(/^ {0,3}(?:[-*+]|\d{1,9}[.)])[ \t]+/, '') // 列表符号
      if (HR.test(s)) return ''
      if (s.includes('|') && TABLE_DELIM.test(s)) return '' // 表格分隔行
      s = s.replace(/\|/g, ' ') // 表格竖线
      s = s.replace(IMAGE, '$1')
      s = s.replace(LINK, (_m, text: string, url: string) =>
        text === url ? text : `${text} ${url}`,
      )
      s = s.replace(/`+/g, '') // 行内码围栏
      s = s.replace(/\*\*\*|\*\*|__|~~/g, '') // 强调标记
      return s
    })
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .replace(/[ \t]+$/gm, '')
    .trim()
}
