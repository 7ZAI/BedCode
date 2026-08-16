/**
 * 代码高亮引擎 seam（ADR-0011 双引擎对比实验）— 移动端 Shiki 实现
 *
 * 渲染管线只依赖 HighlightEngine 接口，引擎选择与管线逻辑解耦：
 * 桌面端注入 hljs 同步实现，移动端注入 Shiki 异步实现（本文件）。
 *
 * 接入模式复制宿主文件查看器（useCodeHighlight）：
 * - createHighlighterCore + createOnigurumaEngine(import('shiki/wasm'))
 * - @shikijs/langs 静态导入——Tauri WebView 不支持动态 import，构建期全量打包
 * - 懒加载单例：首次高亮才加载 WASM，加载失败降级纯文本（不白屏）
 * - 深浅色模式切换两套内置主题（vitesse 对，与宿主文件查看器观感对齐），
 *   不映射 CSS token——主题色由 Shiki 行内样式直接提供
 *
 * 流式管线适配：v-html 每帧重建 DOM，`classList` 幂等检查跨帧失效，改为
 * 「内容+语言+主题」三键缓存——闭合块高亮结果异步回填，下一帧直接命中缓存，
 * 避免流式期间对同一块重复请求（P4 计划风险点第三条）。
 */
import {
  createHighlighterCore,
  type HighlighterCore,
  type LanguageRegistration,
} from 'shiki/core'
import { createOnigurumaEngine } from 'shiki/engine/oniguruma'

// ==================== Static Language Imports ====================
// 与宿主 useCodeHighlight 的语言集对齐（聊天代码块常见语言全覆盖）
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
import langSvelte from '@shikijs/langs/svelte'
import langCss from '@shikijs/langs/css'
import langScss from '@shikijs/langs/scss'
import langLess from '@shikijs/langs/less'
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

import themeVitesseDark from '@shikijs/themes/vitesse-dark'
import themeVitesseLight from '@shikijs/themes/vitesse-light'
import themeGithubLight from '@shikijs/themes/github-light'
import themeGithubDark from '@shikijs/themes/github-dark'
import themeDracula from '@shikijs/themes/dracula'

/** 语言模块：@shikijs/langs 子路径默认导出即语言注册数组；联合命名空间形态
 * 仅为兼容构建器 interop（本插件经 vite 打包恒为数组，见 normalizeLangModule） */
type LangModule = LanguageRegistration[] | { default: LanguageRegistration[] }

/** 语言模块映射：语言 ID → 语言注册 */
const LANG_MODULES: Record<string, LangModule> = {
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
  svelte: langSvelte,
  css: langCss,
  scss: langScss,
  less: langLess,
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
}

/** Markdown fence 常见缩写 → Shiki 语言 id（fence 语言由模型自由书写，须归一化） */
const LANG_ALIASES: Record<string, string> = {
  py: 'python',
  js: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'tsx',
  rs: 'rust',
  sh: 'shellscript',
  bash: 'shellscript',
  zsh: 'shellscript',
  fish: 'shellscript',
  ps1: 'powershell',
  yml: 'yaml',
  md: 'markdown',
  mdx: 'mdx',
  kt: 'kotlin',
  kts: 'kotlin',
  cs: 'csharp',
  cpp: 'cpp',
  hpp: 'cpp',
  'c++': 'cpp',
  h: 'c',
  hh: 'cpp',
  cc: 'cpp',
  rb: 'ruby',
  ex: 'elixir',
  exs: 'elixir',
  hs: 'haskell',
  erl: 'erlang',
  clj: 'clojure',
  vue: 'vue',
  html: 'html',
  xml: 'xml',
  'dockerfile': 'dockerfile',
  makefile: 'makefile',
  cmake: 'cmake',
}

/** 高亮引擎契约：对已闭合代码块产出高亮（就地操作渲染产物 DOM） */
export interface HighlightEngine {
  /** 就地高亮一个 code 元素（已闭合代码块的渲染产物）；异步实现返回 Promise */
  highlightElement(code: HTMLElement): void | Promise<void>
}

// ==================== Shiki 单例（懒加载） ====================

/** 归一化语言模块：vite 直出数组；被构建器包成 { default } 命名空间时取默认导出 */
function normalizeLangModule(mod: LangModule): LanguageRegistration[] {
  return Array.isArray(mod) ? mod : mod.default
}

let highlighterPromise: Promise<HighlighterCore> | null = null

/** 懒加载单例：首个高亮请求触发 WASM 加载；失败时缓存 reject 的 Promise，
 * 后续调用直接走降级路径（不重复加载），避免反复白屏闪烁 */
function getHighlighter(): Promise<HighlighterCore> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterCore({
      // 主题集合与 CodeTheme 选项一一对应（github-light/github-dark/dracula 为具名风格）
      themes: [themeVitesseDark, themeVitesseLight, themeGithubLight, themeGithubDark, themeDracula],
      langs: Object.values(LANG_MODULES).map(normalizeLangModule),
      engine: createOnigurumaEngine(import('shiki/wasm')),
    })
  }
  return highlighterPromise
}

/** 深浅色主题（与宿主文件查看器 vitesse 对观感对齐；缓存键含主题，切换后重算） */
export const SHIKI_DARK_THEME = 'vitesse-dark'
export const SHIKI_LIGHT_THEME = 'vitesse-light'

/** 当前主题：宿主深浅色切换 = html.dark 类（useTheme.ts），插件侧读类判定 */
export function currentShikiTheme(): string {
  return typeof document !== 'undefined' &&
    document.documentElement.classList.contains('dark')
    ? SHIKI_DARK_THEME
    : SHIKI_LIGHT_THEME
}

/**
 * Shiki 高亮核心（纯函数，node 集成测试目标）：产出完整 `<pre>` 高亮 HTML
 *
 * 语言未加载 / 别名未知 / 高亮抛错一律降级 plaintext（Shiki 内置，
 * 不抛错）——聊天代码块语言来自模型输出，不可信，不能因单个块失败阻塞渲染。
 */
export async function highlightCode(code: string, lang: string, theme: string): Promise<string> {
  const highlighter = await getHighlighter()
  const resolved = resolveLang(lang, highlighter)
  try {
    return highlighter.codeToHtml(code, { lang: resolved, theme })
  } catch {
    // 兜底：仍用 Shiki 的 plaintext（转义 + 行结构），失败只可能是引擎异常
    return highlighter.codeToHtml(code, { lang: 'plaintext', theme })
  }
}

/** 语言归一化：别名映射 + 未加载语言降级 plaintext（未加载直接调用会抛错） */
function resolveLang(lang: string, highlighter: HighlighterCore): string {
  const normalized = LANG_ALIASES[lang.trim().toLowerCase()] ?? lang.trim().toLowerCase()
  return highlighter.getLoadedLanguages().includes(normalized) ? normalized : 'plaintext'
}

// ==================== DOM 引擎（组件注入点） ====================

/** 已高亮块缓存：key = `${theme}|${lang}|${content}`。v-html 每帧重建 DOM，
 * 跨帧命中缓存即可同步回填，流式期间同一块只高亮一次（P4 异步回填要点） */
const highlightCache = new Map<string, string>()
/** 缓存上限：长会话中代码块数量有限，达到上限清空重来（避免无限增长） */
const CACHE_MAX = 200
/** in-flight 高亮请求：缓存未命中且同 key 请求在途时复用同一 Promise。
 * 流式期间 v-html 每帧重建 DOM，WASM 首载窗口内相邻帧会重复请求同一块，
 * 去重避免重复 codeToHtml 计算（缓存写入与失败告警由发起者负责） */
const inflightHighlights = new Map<string, Promise<string>>()

/** 从 code 元素的 language-* 类提取 fence 语言（与 marked 渲染的类同源） */
function langFromElement(code: HTMLElement): string {
  return (
    Array.from(code.classList)
      .find(c => c.startsWith('language-'))
      ?.slice('language-'.length) ?? ''
  )
}

/** 把 Shiki 产出的 `<pre>` 级 HTML 剥离外层，回填到 code 元素并打标记 */
function applyHighlight(code: HTMLElement, html: string): void {
  // v-html 每帧重建容器：等待期间元素可能已被替换（isConnected=false），
  // 丢弃过期结果——同一内容的新元素下一帧会命中缓存
  if (!code.isConnected) return
  const template = document.createElement('template')
  template.innerHTML = html
  const highlighted = template.content.querySelector('code')
  if (highlighted) {
    code.innerHTML = highlighted.innerHTML
  }
  code.classList.add('shiki')
  code.dataset.highlighted = '1'
}

/** Shiki 引擎实现：缓存命中同步回填；未命中懒加载 WASM 后异步回填。
 * @param highlight 高亮函数注入点（默认 highlightCode；测试注入可控实现以覆盖 in-flight 去重）
 * @param getTheme 主题解析器注入点（默认 currentShikiTheme 读 html.dark；
 * ChatMessage 注入配置感知解析器实现插件级 codeTheme 强制浅/深色） */
export function createShikiHighlightEngine(
  highlight: typeof highlightCode = highlightCode,
  getTheme: () => string = currentShikiTheme,
): HighlightEngine {
  return {
    highlightElement(code) {
      const content = code.textContent ?? ''
      if (!content.trim()) return
      const theme = getTheme()
      const lang = langFromElement(code)
      // 分隔符用不可打印控制符：代码内容可能含任意可见字符，不能用普通分隔符拼接
      const key = theme + '\u0000' + lang + '\u0000' + content
      const cached = highlightCache.get(key)
      if (cached !== undefined) {
        applyHighlight(code, cached)
        return
      }
      const pending = inflightHighlights.get(key)
      if (pending) {
        // 同 key 在途（首载窗口内相邻帧重复请求同一块）：复用 Promise，
        // 完成时只回填本元素（缓存写入与失败告警由发起者负责）
        void pending
          .then(html => applyHighlight(code, html))
          .catch(() => {
            // 失败已由发起者的 catch 告警，此处仅防未处理拒绝
          })
        return
      }
      // 异步路径：首个请求触发 WASM 加载，完成回填 + 落缓存；失败不注入任何
      // 高亮（marked 已转义，代码块保持纯文本 pre，符合降级约定）
      const promise = highlight(content, lang, theme).then(html => {
        inflightHighlights.delete(key)
        if (highlightCache.size >= CACHE_MAX) highlightCache.clear()
        highlightCache.set(key, html)
        return html
      })
      inflightHighlights.set(key, promise)
      void promise
        .then(html => applyHighlight(code, html))
        .catch(() => {
          console.warn('[AI Chatbox] Shiki highlight failed, keep plaintext:', lang)
        })
    },
  }
}

/** 清空高亮缓存与在途请求（测试用；运行时无需调用） */
export function clearHighlightCacheForTest(): void {
  highlightCache.clear()
  inflightHighlights.clear()
}
