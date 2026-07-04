import { ref, shallowRef } from 'vue'
import { createHighlighterCore } from 'shiki/core'
import { createOnigurumaEngine } from 'shiki/engine/oniguruma'
import type { FileDiffLine } from './useHttpApi'

/** HTML 转义（降级时使用） */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/**
 * 自定义 transformer：为每行 .line 添加 data-line 属性
 *
 * Shiki 默认输出的 <span class="line"> 不含行号信息，
 * 此 transformer 注入 data-line="N" 以配合 CSS ::before 伪元素显示行号
 */
const addLineNumbers = () => ({
  name: 'add-line-numbers',
  line(node: any, line: number) {
    node.properties = node.properties || {}
    node.properties['data-line'] = String(line)
  },
})

// ==================== Language Map ====================

const EXT_LANG_MAP: Record<string, string> = {
  // Top 20 programming languages
  rs: 'rust', ts: 'typescript', tsx: 'tsx', js: 'javascript', jsx: 'jsx',
  py: 'python', java: 'java', go: 'go', c: 'c', h: 'c',
  cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp',
  cs: 'csharp', rb: 'ruby', php: 'php', swift: 'swift',
  kt: 'kotlin', kts: 'kotlin', scala: 'scala', dart: 'dart',
  lua: 'lua', r: 'r',
  // Web & Frontend
  vue: 'vue', svelte: 'svelte', css: 'css', scss: 'scss',
  less: 'less', html: 'html',
  // Data & Config
  json: 'json', toml: 'toml', yaml: 'yaml', yml: 'yaml',
  xml: 'xml', ini: 'ini', env: 'ini',
  // Shell & Scripting
  sh: 'shellscript', bash: 'shellscript', zsh: 'shellscript',
  fish: 'shellscript', ps1: 'powershell',
  // Markup & Docs
  md: 'markdown', mdx: 'mdx',
  // Database
  sql: 'sql',
  // Other common formats
  graphql: 'graphql', gql: 'graphql', dockerfile: 'dockerfile',
  makefile: 'makefile', cmake: 'cmake', nix: 'nix', zig: 'zig',
  asm: 'asm', elixir: 'elixir', ex: 'elixir', exs: 'elixir',
  haskell: 'haskell', hs: 'haskell', erlang: 'erlang', erl: 'erlang',
  clojure: 'clojure', clj: 'clojure',
}

/** 根据文件扩展名获取 Shiki 语言 ID */
export function getLangByFilename(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase() || ''
  return EXT_LANG_MAP[ext] || 'plaintext'
}

// ==================== Static Language Imports ====================
// 使用 @shikijs/langs 的静态 import，确保 Vite 在构建时打包
// 动态 import('shiki/langs/xxx.mjs') 在 Tauri WebView 中无法解析

import langRust from '@shikijs/langs/rust'
import langTypescript from '@shikijs/langs/typescript'
import langTsx from '@shikijs/langs/tsx'
import langJavascript from '@shikijs/langs/javascript'
import langJsx from '@shikijs/langs/jsx'
import langPython from '@shikijs/langs/python'
import langJava from '@shikijs/langs/java'
import langGo from '@shikijs/langs/go'
import langC from '@shikijs/langs/c'
import langCpp from '@shikijs/langs/cpp'
import langCsharp from '@shikijs/langs/csharp'
import langRuby from '@shikijs/langs/ruby'
import langPhp from '@shikijs/langs/php'
import langSwift from '@shikijs/langs/swift'
import langKotlin from '@shikijs/langs/kotlin'
import langScala from '@shikijs/langs/scala'
import langDart from '@shikijs/langs/dart'
import langLua from '@shikijs/langs/lua'
import langR from '@shikijs/langs/r'
import langVue from '@shikijs/langs/vue'
import langCss from '@shikijs/langs/css'
import langScss from '@shikijs/langs/scss'
import langHtml from '@shikijs/langs/html'
import langJson from '@shikijs/langs/json'
import langToml from '@shikijs/langs/toml'
import langYaml from '@shikijs/langs/yaml'
import langXml from '@shikijs/langs/xml'
import langIni from '@shikijs/langs/ini'
import langShellscript from '@shikijs/langs/shellscript'
import langPowershell from '@shikijs/langs/powershell'
import langMarkdown from '@shikijs/langs/markdown'
import langMdx from '@shikijs/langs/mdx'
import langSql from '@shikijs/langs/sql'
import langGraphql from '@shikijs/langs/graphql'
import langDockerfile from '@shikijs/langs/dockerfile'
import langMakefile from '@shikijs/langs/makefile'
import langCmake from '@shikijs/langs/cmake'
import langNix from '@shikijs/langs/nix'
import langZig from '@shikijs/langs/zig'
import langAsm from '@shikijs/langs/asm'
import langElixir from '@shikijs/langs/elixir'
import langHaskell from '@shikijs/langs/haskell'
import langErlang from '@shikijs/langs/erlang'
import langClojure from '@shikijs/langs/clojure'
import langSvelte from '@shikijs/langs/svelte'
import langLess from '@shikijs/langs/less'
import themeVitesseDark from '@shikijs/themes/vitesse-dark'
import themeVitesseLight from '@shikijs/themes/vitesse-light'
import themeOneDarkPro from '@shikijs/themes/one-dark-pro'
import themeOneLight from '@shikijs/themes/one-light'
import themeNord from '@shikijs/themes/nord'
import themeGithubDark from '@shikijs/themes/github-dark'
import themeGithubLight from '@shikijs/themes/github-light'
import themeMonokai from '@shikijs/themes/monokai'

/** 语言模块映射：语言 ID → 模块默认导出 */
const LANG_MODULES: Record<string, any> = {
  rust: langRust,
  typescript: langTypescript,
  tsx: langTsx,
  javascript: langJavascript,
  jsx: langJsx,
  python: langPython,
  java: langJava,
  go: langGo,
  c: langC,
  cpp: langCpp,
  csharp: langCsharp,
  ruby: langRuby,
  php: langPhp,
  swift: langSwift,
  kotlin: langKotlin,
  scala: langScala,
  dart: langDart,
  lua: langLua,
  r: langR,
  vue: langVue,
  css: langCss,
  scss: langScss,
  html: langHtml,
  json: langJson,
  toml: langToml,
  yaml: langYaml,
  xml: langXml,
  ini: langIni,
  shellscript: langShellscript,
  powershell: langPowershell,
  markdown: langMarkdown,
  mdx: langMdx,
  sql: langSql,
  graphql: langGraphql,
  dockerfile: langDockerfile,
  makefile: langMakefile,
  cmake: langCmake,
  nix: langNix,
  zig: langZig,
  asm: langAsm,
  elixir: langElixir,
  haskell: langHaskell,
  erlang: langErlang,
  clojure: langClojure,
  svelte: langSvelte,
  less: langLess,
}

// ==================== Highlighter Singleton ====================

const THEME = 'vitesse-dark'

const THEME_MODULES = [
  themeVitesseDark,
  themeVitesseLight,
  themeOneDarkPro,
  themeOneLight,
  themeNord,
  themeGithubDark,
  themeGithubLight,
  themeMonokai,
]

let highlighterInstance: Awaited<ReturnType<typeof createHighlighterCore>> | null = null
let initPromise: Promise<void> | null = null

async function ensureHighlighter(): Promise<NonNullable<typeof highlighterInstance>> {
  if (highlighterInstance) return highlighterInstance

  if (!initPromise) {
    initPromise = (async () => {
      // 将所有语言模块的 default export 展开为数组
      const langImports = Object.values(LANG_MODULES).map(mod => mod.default ?? mod)

      highlighterInstance = await createHighlighterCore({
        themes: THEME_MODULES,
        langs: langImports,
        engine: createOnigurumaEngine(import('shiki/wasm')),
      })
    })()
  }

  await initPromise
  return highlighterInstance!
}

// ==================== Composable ====================

export function useCodeHighlight() {
  const highlightedHtml = shallowRef<string>('')
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  async function highlight(code: string, lang: string, theme?: string): Promise<void> {
    isLoading.value = true
    error.value = null

    try {
      const highlighter = await ensureHighlighter()

      // 语言已在初始化时全部加载，不支持的语言降级为 plaintext
      if (!highlighter.getLoadedLanguages().includes(lang)) {
        lang = 'plaintext'
      }

      const resolvedTheme = theme || THEME

      const html = highlighter.codeToHtml(code, {
        lang,
        theme: resolvedTheme,
        transformers: [addLineNumbers()],
      })
      highlightedHtml.value = html
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      highlightedHtml.value = ''
    } finally {
      isLoading.value = false
    }
  }

  async function highlightDiff(lines: FileDiffLine[], lang: string, theme?: string): Promise<void> {
    isLoading.value = true
    error.value = null

    try {
      const highlighter = await ensureHighlighter()

      if (!highlighter.getLoadedLanguages().includes(lang)) {
        lang = 'plaintext'
      }

      const resolvedTheme = theme || THEME

      // 将所有行内容拼接为完整代码段，整体高亮以保留语法上下文
      const fullCode = lines.map(l => l.content).join('\n')
      const html = highlighter.codeToHtml(fullCode, {
        lang,
        theme: resolvedTheme,
        transformers: [addLineNumbers()],
      })

      // 解析高亮后的 HTML，按行拆分并包裹 diff 结构
      const parser = new DOMParser()
      const doc = parser.parseFromString(html, 'text/html')
      const codeEl = doc.querySelector('code')
      const highlightedLines = codeEl
        ? Array.from(codeEl.querySelectorAll('.line')).map(el => el.innerHTML)
        : lines.map(l => escapeHtml(l.content))

      // 构建 diff HTML
      const diffHtml = lines.map((line, i) => {
        const highlighted = highlightedLines[i] || escapeHtml(line.content)
        const oldNo = line.oldLineNo != null ? String(line.oldLineNo) : ''
        const newNo = line.newLineNo != null ? String(line.newLineNo) : ''
        const marker = line.type === 'removed' ? '-' : line.type === 'added' ? '+' : ' '
        return `<div class="diff-line diff-${line.type}" data-old-line="${oldNo}" data-new-line="${newNo}">` +
          `<span class="diff-line-no diff-old-no">${oldNo}</span>` +
          `<span class="diff-line-no diff-new-no">${newNo}</span>` +
          `<span class="diff-marker">${marker}</span>` +
          `<span class="diff-content">${highlighted}</span>` +
          `</div>`
      }).join('\n')

      highlightedHtml.value = diffHtml
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      highlightedHtml.value = ''
    } finally {
      isLoading.value = false
    }
  }

  return { highlightedHtml, isLoading, error, highlight, highlightDiff }
}
