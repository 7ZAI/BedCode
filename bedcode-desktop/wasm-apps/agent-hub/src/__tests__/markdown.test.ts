/**
 * 聊天视图 Markdown 子集渲染单测（README 票 B2）
 *
 * 行为契约（每条对应 markdown.ts 的一个分支 / 一条安全约束）：
 *
 * | 契约 | 来源 | 规则 |
 * | --- | --- | --- |
 * | C-M01 | renderMarkdown 首行提前返回 | 空串 / 纯空白 → 空串（调用方不挂 markdown 类） |
 * | C-M02 | 段落分支 | 连续非空行合成一个 `p`，行内软换行 → `br` |
 * | C-M03 | 围栏分支 | 围栏内**不解析**任何 markdown，代码实体化后进 `pre > code` |
 * | C-M04 | 围栏开口 info | info 首词写进 `data-lang`；未闭合围栏渲染到文末（不吞后续块） |
 * | C-M05 | ATX 分支 | `#`…`######` → h1…h6；`#tag`（无空格）不算标题 |
 * | C-M06 | 列表分支 | 无序 → `ul`、有序 → `ol`；两级缩进 → 嵌套子列表 |
 * | C-M07 | 引用 / 分隔线分支 | `> ` → blockquote；`---`/`***` → `hr` |
 * | C-M08 | 表格分支 | 表头 + 分隔行 → thead；列数不足补空、过多截断（宽容不丢表） |
 * | C-M09 | renderInline 强调 | 行内码 > 强调 > 删除线；代码内容不被强调正则切进去 |
 * | C-M10 | 链接降级 | `[t](u)` → `span[data-md-link][title=u]`，**无 href**（零资源访问红线） |
 * | C-M11 | 安全：转义先行 | 原始 HTML 全实体化；图片不产生 `img`；属性里的引号被实体化 |
 * | C-M12 | 占位符隔离 | 正文里的裸数字不被行内码占位符还原吃掉（版本号 / 行号） |
 * | C-M13 | 换行归一 | CRLF 文本与 LF 文本渲染结果一致 |
 * | C-M14 | markdownPlainPreview | 剥标记留正文（**不做字符级截断**），链接保留 URL |
 */
import { describe, expect, it } from 'vitest'
import { escapeHtml, markdownPlainPreview, renderMarkdown } from '../utils/markdown'

describe('renderMarkdown：块级（C-M01 / C-M03–C-M08）', () => {
  it('C-M01 空串与纯空白返回空串（不产出空壳标签）', () => {
    expect(renderMarkdown('')).toBe('')
    expect(renderMarkdown('   \n\t\n')).toBe('')
  })

  it('C-M02 连续行合成一个段落，软换行渲染成 br', () => {
    expect(renderMarkdown('第一行\n第二行')).toBe('<p>第一行<br />第二行</p>')
    expect(renderMarkdown('上段\n\n下段')).toBe('<p>上段</p><p>下段</p>')
  })

  it('C-M03 围栏内不解析 markdown，代码实体化', () => {
    const html = renderMarkdown('```bash\nrm -rf **x** <b>y</b>\n```')
    expect(html).toContain('<pre><code data-lang="bash">')
    // ** 与尖括号都只是字面量代码
    expect(html).toContain('rm -rf **x** &lt;b&gt;y&lt;/b&gt;')
    expect(html).not.toContain('<strong>')
    expect(html).not.toContain('<b>')
  })

  it('C-M03 波浪号围栏同样成立，且代码里的反引号不被当行内码', () => {
    const html = renderMarkdown('~~~\nconst a = `x`\n~~~')
    expect(html).toBe('<pre><code>const a = `x`</code></pre>')
  })

  it('C-M04 info 只取首词；未闭合围栏渲染到文末且不抛', () => {
    expect(renderMarkdown('```ts extra\nx\n```')).toContain('data-lang="ts"')
    expect(renderMarkdown('```js\nconst a = 1')).toBe('<pre><code data-lang="js">const a = 1</code></pre>')
    expect(renderMarkdown('```js\nconst a = 1')).not.toContain('undefined')
  })

  it('C-M05 ATX 标题映射到 h1–h6；无空格的井号行是正文', () => {
    expect(renderMarkdown('# 一级')).toBe('<h1>一级</h1>')
    expect(renderMarkdown('###### 六级')).toBe('<h6>六级</h6>')
    expect(renderMarkdown('####### 七井号不是标题')).toContain('<p>')
    expect(renderMarkdown('#tag')).toBe('<p>#tag</p>')
  })

  it('C-M06 无序 / 有序列表与两级缩进子列表', () => {
    expect(renderMarkdown('- 一\n- 二')).toBe('<ul><li><p>一</p></li><li><p>二</p></li></ul>')
    expect(renderMarkdown('1. 甲\n2. 乙')).toBe('<ol><li><p>甲</p></li><li><p>乙</p></li></ol>')
    const nested = renderMarkdown('- 外\n  - 内')
    expect(nested).toBe('<ul><li><p>外</p><ul><li><p>内</p></li></ul></li></ul>')
  })

  it('C-M07 引用与分隔线', () => {
    expect(renderMarkdown('> 引用一行')).toBe('<blockquote><p>引用一行</p></blockquote>')
    expect(renderMarkdown('---')).toBe('<hr />')
    expect(renderMarkdown('***')).toBe('<hr />')
    // 少于三个符号不是分隔线（否则会把列表项吃掉）
    expect(renderMarkdown('--')).toBe('<p>--</p>')
  })

  it('C-M08 管道表格；列数不足补空、过多截断', () => {
    const html = renderMarkdown('| a | b |\n| --- | --- |\n| 1 | 2 |')
    expect(html).toBe(
      '<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>',
    )
    const ragged = renderMarkdown('| a | b |\n| --- | --- |\n| 1 |\n| 1 | 2 | 3 |')
    expect(ragged).toContain('<tr><td>1</td><td></td></tr>')
    expect(ragged).toContain('<tr><td>1</td><td>2</td></tr>')
    expect(ragged).not.toContain('<td>3</td>')
  })

  it('C-M08 表头列数与分隔行不一致时不当表格（回落成段落）', () => {
    const html = renderMarkdown('| a | b |\n| --- |\n| 1 | 2 |')
    expect(html).not.toContain('<table>')
    expect(html).toContain('<p>')
  })
})

describe('renderMarkdown：行内（C-M09 / C-M10 / C-M12）', () => {
  it('C-M09 强调 / 删除线 / 行内码', () => {
    expect(renderInlineText('**粗**')).toBe('<p><strong>粗</strong></p>')
    expect(renderInlineText('*斜*')).toBe('<p><em>斜</em></p>')
    expect(renderInlineText('~~删~~')).toBe('<p><del>删</del></p>')
    expect(renderInlineText('`a<b`')).toBe('<p><code>a&lt;b</code></p>')
  })

  it('C-M09 行内码里的星号不被强调规则切进去', () => {
    expect(renderInlineText('`**not bold**`')).toBe('<p><code>**not bold**</code></p>')
  })

  it('C-M10 链接降级为不可点文本（无 href），URL 挂 title', () => {
    const html = renderInlineText('看 [文档](https://example.com/a?b=1)')
    expect(html).toContain('<span data-md-link title="https://example.com/a?b=1">文档</span>')
    expect(html).not.toContain('href')
  })

  it('C-M12 正文裸数字不被占位符还原吃掉（回归：曾渲染成空串）', () => {
    const html = renderInlineText('升级到 2 并看第 42 行 与 `x`')
    expect(html).toContain('升级到 2')
    expect(html).toContain('第 42 行')
    expect(html).toContain('<code>x</code>')
  })
})

describe('renderMarkdown：安全（C-M11）与换行归一（C-M13）', () => {
  it('C-M11 原始 HTML 全实体化，不产生可执行标签', () => {
    const html = renderMarkdown('<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>')
    expect(html).not.toContain('<script')
    expect(html).not.toContain('<img')
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;')
  })

  it('C-M11 图片语法只留 alt（零资源访问红线：绝不产生 img src）', () => {
    const html = renderMarkdown('![截图](/tmp/a.png)')
    expect(html).toBe('<p>截图</p>')
    expect(html).not.toContain('src')
  })

  it('C-M11 链接 URL 里的引号被实体化（属性注入面）', () => {
    const html = renderInlineText('[x](https://a" onmouseover="alert(1))')
    expect(html).not.toContain('onmouseover="alert')
    expect(html).toContain('&quot;')
  })

  it('C-M11 javascript: URL 也不产生 href（只有 title 文本）', () => {
    const html = renderInlineText('[点我](javascript:alert(1))')
    expect(html).not.toContain('href')
    expect(html).toContain('javascript:alert(1)')
  })

  it('C-M13 CRLF 与 LF 渲染结果一致', () => {
    const lf = renderMarkdown('# 标题\n\n- 一\n- 二')
    const crlf = renderMarkdown('# 标题\r\n\r\n- 一\r\n- 二')
    expect(crlf).toBe(lf)
  })
})

describe('markdownPlainPreview：折叠预览（C-M14）', () => {
  it('剥掉块级与行内标记，只留可读正文', () => {
    const preview = markdownPlainPreview('# 标题\n\n- **要点**一\n- `code`二\n\n> 引用\n\n---')
    expect(preview).not.toContain('#')
    expect(preview).not.toContain('**')
    expect(preview).not.toContain('`')
    expect(preview).not.toContain('---')
    expect(preview).toContain('标题')
    expect(preview).toContain('要点一')
    expect(preview).toContain('code二')
    expect(preview).toContain('引用')
  })

  it('不做字符级截断：长文本整体保留（可全选 / 可搜索）', () => {
    const long = 'x'.repeat(2000)
    expect(markdownPlainPreview(long)).toBe(long)
  })

  it('链接保留 URL 可见（不可点也得看得见原文）', () => {
    expect(markdownPlainPreview('看 [文档](https://example.com)')).toBe('看 文档 https://example.com')
    // 文字与 URL 相同则不重复输出
    expect(markdownPlainPreview('[https://a.dev](https://a.dev)')).toBe('https://a.dev')
  })

  it('代码围栏标记剥掉但代码正文保留', () => {
    expect(markdownPlainPreview('```js\nconst a = 1\n```')).toContain('const a = 1')
  })

  it('空输入返回空串', () => {
    expect(markdownPlainPreview('')).toBe('')
    expect(markdownPlainPreview('\n \n')).toBe('')
  })

  // 调用方（SessionLogsTab）把本函数结果经 v-html 注入：折叠态是长消息的默认路径，
  // 不实体化就会把 agent 输出里的 HTML 当节点执行（webview 内可直接拿到 invoke() 面）
  it('C-M15 五个定界字符全实体化（折叠态经 v-html 注入，必须先转义）', () => {
    expect(markdownPlainPreview(`&<>"'`)).toBe('&amp;&lt;&gt;&quot;&#39;')
  })

  it('C-M16 反例：img/svg 等带事件处理器的标签不产生任何标签残留', () => {
    const out = markdownPlainPreview('<img src=x onerror=alert(1)>\n<svg onload=alert(2)></svg>')
    expect(out).not.toContain('<')
    expect(out).not.toContain('>')
    expect(out).toContain('&lt;img src=x onerror=alert(1)&gt;')
  })

  it('C-M17 边界：HTML 与 markdown 标记混排时两边都不得变成标签', () => {
    const out = markdownPlainPreview('<b>**粗体**</b> <i>*斜体*</i>')
    expect(out).not.toContain('<b>')
    expect(out).not.toContain('<i>')
    expect(out).toContain('&lt;b&gt;粗体&lt;/b&gt;')
    expect(out).not.toContain('**')
  })

  it('C-M18 属性定界字符也被实体化（防属性注入形态）', () => {
    const out = markdownPlainPreview('<a href="x" onclick="evil()">链接</a>')
    expect(out).not.toContain('onclick="evil()"')
    expect(out).toContain('&quot;x&quot;')
    expect(out).toContain('&quot;evil()&quot;')
  })

  it('C-M19 无特殊字符的正文不受影响（转义不引入噪声）', () => {
    expect(markdownPlainPreview('普通正文，没有需要转义的字符')).toBe('普通正文，没有需要转义的字符')
  })
})

describe('escapeHtml', () => {
  it('五个定界字符全实体化', () => {
    expect(escapeHtml(`&<>"'`)).toBe('&amp;&lt;&gt;&quot;&#39;')
  })
})

/** 走整篇渲染再取段落内文（避免直接测内部行内函数的边界） */
function renderInlineText(text: string): string {
  return renderMarkdown(text)
}
