import { ref, shallowRef } from 'vue'
import { createHighlighterCore } from 'shiki/core'
import { createOnigurumaEngine } from 'shiki/engine/oniguruma'

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

let highlighterInstance: Awaited<ReturnType<typeof createHighlighterCore>> | null = null
let initPromise: Promise<void> | null = null

async function ensureHighlighter(): Promise<NonNullable<typeof highlighterInstance>> {
  if (highlighterInstance) return highlighterInstance

  if (!initPromise) {
    initPromise = (async () => {
      // 将所有语言模块的 default export 展开为数组
      const langImports = Object.values(LANG_MODULES).map(mod => mod.default ?? mod)

      highlighterInstance = await createHighlighterCore({
        themes: [themeVitesseDark],
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

  async function highlight(code: string, lang: string): Promise<void> {
    isLoading.value = true
    error.value = null

    try {
      const highlighter = await ensureHighlighter()

      // 语言已在初始化时全部加载，不支持的语言降级为 plaintext
      if (!highlighter.getLoadedLanguages().includes(lang)) {
        lang = 'plaintext'
      }

      const html = highlighter.codeToHtml(code, {
        lang,
        theme: THEME,
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

  return { highlightedHtml, isLoading, error, highlight }
}
