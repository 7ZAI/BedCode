import { ref, shallowRef } from 'vue'
import { createHighlighterCore } from 'shiki/core'
import { createOnigurumaEngine } from 'shiki/engine/oniguruma'

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

// ==================== Highlighter Singleton ====================

const THEME = 'vitesse-dark'

// 常用语言列表，初始化时预加载
const PRELOAD_LANGS = [
  'rust', 'typescript', 'javascript', 'python', 'java', 'go',
  'c', 'cpp', 'csharp', 'ruby', 'php', 'swift', 'kotlin',
  'scala', 'dart', 'lua', 'r', 'vue', 'css', 'scss', 'html',
  'json', 'toml', 'yaml', 'xml', 'ini', 'shellscript', 'powershell',
  'markdown', 'mdx', 'sql', 'graphql', 'dockerfile', 'makefile',
  'cmake', 'nix', 'zig', 'asm', 'elixir', 'haskell', 'erlang',
  'clojure', 'plaintext', 'tsx', 'jsx', 'svelte', 'less',
]

let highlighterInstance: Awaited<ReturnType<typeof createHighlighterCore>> | null = null
let initPromise: Promise<void> | null = null

async function ensureHighlighter(): Promise<NonNullable<typeof highlighterInstance>> {
  if (highlighterInstance) return highlighterInstance

  if (!initPromise) {
    initPromise = (async () => {
      highlighterInstance = await createHighlighterCore({
        themes: [import('shiki/themes/vitesse-dark.mjs')],
        langs: PRELOAD_LANGS.map(lang => import(`shiki/langs/${lang}.mjs`)),
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

      // 动态加载未预加载的语言
      if (!highlighter.getLoadedLanguages().includes(lang)) {
        try {
          await highlighter.loadLanguage(await import(`shiki/langs/${lang}.mjs`))
        } catch {
          lang = 'plaintext'
        }
      }

      const html = highlighter.codeToHtml(code, { lang, theme: THEME })
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
