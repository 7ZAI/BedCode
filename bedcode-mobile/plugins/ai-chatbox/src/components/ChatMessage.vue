<template>
  <!-- user 消息右对齐，assistant 消息左对齐（flex-row-reverse 实现左右分列） -->
  <div class="flex gap-2.5" :class="isUser ? 'flex-row-reverse' : ''" :style="codeStyle">
    <!-- 头像 -->
    <div
      class="w-9 h-9 rounded-xl flex items-center justify-center flex-shrink-0 text-sm mt-0.5"
      :class="isUser
        ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
        : 'bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)]'"
    >
      <svg v-if="isUser" class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
      </svg>
      <!-- 身份由图标表明，不显示文字 -->
      <svg v-else class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M9 17v2a2 2 0 002 2h2a2 2 0 002-2v-2M9 4h6M5 12h14a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4a1 1 0 011-1z" />
      </svg>
    </div>

    <div class="flex-1 min-w-0 space-y-1" :class="isUser ? 'text-right' : ''">
      <!-- 元信息行（user 反向排列；身份由头像图标表明，不再重复文字） -->
      <div class="flex items-center gap-2 text-xs text-[var(--mobile-text-muted)]" :class="isUser ? 'flex-row-reverse' : ''">
        <span v-if="message.model" class="font-mono">{{ message.model }}</span>
        <!-- token 用量 -->
        <span v-if="message.usage" class="font-mono rounded-md bg-[var(--mobile-bg-tertiary)] px-1.5 py-0.5">
          ↑{{ message.usage.promptTokens }} ↓{{ message.usage.completionTokens }} Σ{{ message.usage.totalTokens }}
        </span>
      </div>

      <!-- 思考过程块（assistant 专属，P3）：可折叠次级样式；流式期间默认展开；
           showReasoning=false 时不渲染；reasoning 取自消息字段（历史重开可见） -->
      <div
        v-if="!isUser && showReasoning !== false && message.reasoning"
        class="thinking-block"
      >
        <button
          class="thinking-toggle"
          :aria-expanded="reasoningExpanded"
          :aria-controls="`thinking-body-${message.id}`"
          :title="t('mobile.plugin.aiChatbox.thinkingProcess')"
          @click="reasoningExpanded = !reasoningExpanded"
        >
          <svg
            class="w-3 h-3 flex-shrink-0 transition-transform duration-200"
            :class="reasoningExpanded ? 'rotate-90' : ''"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
          <span class="truncate">{{ t('mobile.plugin.aiChatbox.thinkingProcess') }}</span>
        </button>
        <!-- 思考内容为模型 scratchpad 草稿，非成品 Markdown：纯文本展示（预换行），
             不经过渲染管线，天然免疫 prompt injection 的 HTML 注入 -->
        <div v-if="reasoningExpanded" :id="`thinking-body-${message.id}`" class="thinking-body">{{ message.reasoning }}</div>
      </div>

      <!-- 内容（user 右对齐气泡卡片；assistant Markdown 渲染 + Shiki 高亮） -->
      <div
        v-if="isUser"
        class="inline-block max-w-[85%] text-left whitespace-pre-wrap break-words px-3.5 py-2.5 text-[var(--font-size-base)] leading-relaxed text-[var(--mobile-text-primary)] rounded-2xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)]"
      >{{ message.content }}</div>
      <div v-else ref="contentRef" class="text-[var(--font-size-base)] leading-relaxed text-[var(--mobile-text-primary)] md-body" v-html="rendered" />

      <!-- 错误提示（assistant 无内容且带错误时） -->
      <div
        v-if="!isUser && !message.content && errorText"
        class="text-xs text-[var(--mobile-error)]"
      >{{ errorText }}</div>

      <!-- 操作条（移动端无 hover，常显：复制 / 删除 / 重新生成（仅最后一条 assistant）；仅图标无背景，与宿主页头按钮同款） -->
      <div class="flex items-center gap-0.5" :class="isUser ? 'justify-end' : ''">
        <button
          class="w-9 h-9 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
          :title="t('mobile.plugin.aiChatbox.copy')"
          :aria-label="t('mobile.plugin.aiChatbox.copy')"
          @click="copyContent"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9h11a2 2 0 012 2v9a2 2 0 01-2 2H9a2 2 0 01-2-2v-9a2 2 0 012-2zM5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
          </svg>
        </button>
        <button
          class="w-9 h-9 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
          :title="t('mobile.plugin.aiChatbox.delete')"
          :aria-label="t('mobile.plugin.aiChatbox.delete')"
          @click="$emit('delete', message)"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
        <!-- 重新生成（仅最后一条 assistant 且非流式）：跟在删除符号后面 -->
        <button
          v-if="showRegenerate"
          class="w-9 h-9 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
          :title="t('mobile.plugin.aiChatbox.regenerate')"
          :aria-label="t('mobile.plugin.aiChatbox.regenerate')"
          @click="$emit('regenerate')"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12a9 9 0 019-9 9.75 9.75 0 016.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 01-9 9 9.75 9.75 0 01-6.74-2.74L3 16M8 16H3v5" />
          </svg>
        </button>
      </div>

      <!-- 流式光标 -->
      <span
        v-if="streaming"
        class="inline-block w-2 h-4 ml-0.5 align-middle bg-[var(--mobile-accent)] animate-pulse"
      ></span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatMessage — 单条聊天消息（移动端）
 *
 * user 消息纯文本；assistant 消息 Markdown 渲染（marked + Shiki 代码高亮
 * + DOMPurify 消毒），支持整条复制、代码块语言标签 + 一键复制、删除、
 * token 用量显示、流式光标、思考过程折叠块。
 * 移动端无 hover：复制/删除为常显操作条。
 */
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import type { ChatMessage, CodeTheme } from '../types'
import { getClosedCodeBlocks, patchIncompleteMarkdown } from '../utils/markdown'
import {
  SHIKI_DARK_THEME,
  SHIKI_LIGHT_THEME,
  createShikiHighlightEngine,
  currentShikiTheme,
  type HighlightEngine,
} from '../utils/highlight'

const props = defineProps<{
  message: ChatMessage
  streaming?: boolean
  errorText?: string
  /** 插件级 showReasoning 配置（false 时整体不渲染思考块） */
  showReasoning?: boolean
  /** 插件级代码块行距配置（0.5-2.0，缺省 1.6） */
  codeLineHeight?: number
  /** 插件级代码块字体大小（px，缺省 13） */
  codeFontSize?: number
  /** 插件级代码高亮主题（auto 跟随宿主深浅色，缺省 auto） */
  codeTheme?: CodeTheme
  /** 是否显示重新生成按钮（仅最后一条 assistant 且非流式） */
  showRegenerate?: boolean
}>()

defineEmits<{ delete: [message: ChatMessage]; regenerate: [] }>()

const { t } = useI18n()

// 高亮引擎 seam（ADR-0011）：移动端注入 Shiki 异步实现（懒加载单例 + 多主题包）；
// 主题解析器注入配置感知实现：具名主题直接锁定，auto 时跟随宿主 html.dark
const highlightEngine: HighlightEngine = createShikiHighlightEngine(
  undefined,
  () => resolveCodeTheme(props.codeTheme ?? 'auto'),
)

/** 具名主题 → Shiki 主题 id（与 highlight.ts 加载的集合一一对应） */
const CODE_THEME_NAMES: Record<Exclude<CodeTheme, 'auto'>, string> = {
  light: SHIKI_LIGHT_THEME,
  dark: SHIKI_DARK_THEME,
  'github-light': 'github-light',
  'github-dark': 'github-dark',
  dracula: 'dracula',
}

/** 代码主题解析：具名主题锁定 Shiki 主题包；auto 跟随宿主深浅色 */
function resolveCodeTheme(theme: CodeTheme): string {
  if (theme !== 'auto') return CODE_THEME_NAMES[theme]
  return currentShikiTheme()
}

const isUser = computed(() => props.message.role === 'user')

/** 代码渲染样式（行距 + 字体大小，CSS 变量下发到 :deep 代码块样式） */
const codeStyle = computed(() => ({
  '--md-code-lh': String(props.codeLineHeight ?? 1.6),
  '--md-code-font-size': `${props.codeFontSize ?? 13}px`,
}))

/** 思考块展开状态：流式期间默认展开（边生成边可见）；结束后保持用户当前折叠状态 */
const reasoningExpanded = ref(false)
watch(
  () => props.streaming,
  (streaming) => {
    if (streaming) reasoningExpanded.value = true
  },
  { immediate: true },
)
// 消息列表 shift（如删除中间消息后组件按 :key="i" 复用）会残留上一消息的折叠状态：
// 消息 id 变化即复位为「当前是否流式末位」——与 streaming watcher 同帧取值，
// 流式消息默认展开、其余折叠，结果一致
watch(
  () => props.message.id,
  () => {
    reasoningExpanded.value = props.streaming === true
  },
)

/** Markdown → HTML（breaks 让单换行也换行，贴合聊天场景）
 *
 * LLM 输出不可信：marked 保留原始 HTML，prompt injection 可注入
 * `<img onerror>` 等脚本在插件上下文执行（插件持宿主命令桥接能力），
 * 必须经 DOMPurify 消毒后再进 v-html。
 *
 * 流式期间先做未闭合标记补偿（fence/行尾行内码补全），保证每帧渲染的都是
 * 闭合形态——fence 未闭合时不至于把后续文本整段吞进代码块（布局跳动） */
const rendered = computed(() => {
  const patched = patchIncompleteMarkdown(props.message.content)
  const html = marked.parse(patched, { async: false, breaks: true }) as string
  return DOMPurify.sanitize(html)
})

const contentRef = ref<HTMLElement | null>(null)

/** 代码块高亮 + 注入语言标签/复制按钮头部（渲染后执行）
 *
 * 只处理已闭合块：流式期间未闭合块渲染为纯文本 pre（fence 补偿已保证其
 * 位于代码块容器内），闭合后下一帧自然获得高亮与头部——避免语言标签/复制
 * 按钮盖住仍在生长的代码块，也避免对增长中的块反复重扫（P2 延迟高亮）。
 * Shiki 为异步引擎：缓存命中同步回填，未命中懒加载 WASM 后异步回填 */
function enhanceCodeBlocks(): void {
  const container = contentRef.value
  if (!container) return
  const blocks = getClosedCodeBlocks(props.message.content)
  // 未闭合块只会是文本中最后一个 fenced 块：fence 未闭合意味着其后内容全在块内，
  // 补偿闭合后它必然是渲染结果的最后一个 pre（缩进代码块等都在它之前）
  const lastUnclosed = blocks.length > 0 && !blocks[blocks.length - 1].closed
  const pres = Array.from(container.querySelectorAll<HTMLElement>('pre'))
  pres.forEach((pre, i) => {
    // 未闭合块：纯文本 pre，不高亮、不注入头部
    if (lastUnclosed && i === pres.length - 1) return
    const code = pre.querySelector('code')
    if (code && !pre.querySelector('.md-code-header')) {
      // 语言取自 marked 实际渲染的 language-* 类（HTML 解析器已解码实体），与
      // getClosedCodeBlocks.lang 同源；按 classList 取首 token，避免 "js,x" 等
      // 含标点的语言被 \w 正则截断
      const lang =
        Array.from(code.classList)
          .find(c => c.startsWith('language-'))
          ?.slice('language-'.length) ?? ''
      const header = document.createElement('div')
      header.className = 'md-code-header'
      const langEl = document.createElement('span')
      langEl.className = 'md-code-lang'
      langEl.textContent = lang
      const btn = document.createElement('button')
      btn.className = 'md-copy-btn'
      btn.textContent = t('mobile.plugin.aiChatbox.copy')
      btn.addEventListener('click', () => {
        const text = code.innerText ?? ''
        navigator.clipboard.writeText(text).catch(() => {})
        btn.textContent = t('mobile.plugin.aiChatbox.copied')
        setTimeout(() => {
          btn.textContent = t('mobile.plugin.aiChatbox.copy')
        }, 1500)
      })
      header.append(langEl, btn)
      pre.prepend(header)
    }
    // v-html 每帧重建 DOM，data-highlighted 标记仅帧内幂等；跨帧幂等由引擎缓存保证
    if (code && !code.dataset.highlighted) {
      highlightEngine.highlightElement(code)
    }
  })
}

/** 主题切换：清除旧主题的高亮标记后重扫（缓存按主题分键，切换后自动重算） */
function onThemeChanged(): void {
  const container = contentRef.value
  if (!container) return
  container
    .querySelectorAll<HTMLElement>('code[data-highlighted]')
    .forEach(c => delete c.dataset.highlighted)
  enhanceCodeBlocks()
}

/** 宿主深浅色切换 = html.dark 类（useTheme.ts）；observer 监听类变化触发重高亮 */
let themeObserver: MutationObserver | null = null

/** 复制整条消息 */
async function copyContent(): Promise<void> {
  await navigator.clipboard.writeText(props.message.content).catch(() => {})
}

onMounted(() => {
  enhanceCodeBlocks()
  themeObserver = new MutationObserver(onThemeChanged)
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
})
onUnmounted(() => {
  themeObserver?.disconnect()
  themeObserver = null
})
// 插件级 codeTheme 配置切换（auto ↔ light/dark）：清除旧主题高亮标记后重扫
//（高亮缓存按主题分键，强制主题与宿主切换间不互相污染）
watch(() => props.codeTheme, onThemeChanged)
// flush: 'post'：等组件 DOM patch 完成后再注入——默认 'pre' 的 watch 会在新 DOM
// 渲染前执行，注入落在上一帧 DOM 上、随后被 v-html 整段覆盖
watch(() => props.message.content, enhanceCodeBlocks, { flush: 'post' })
</script>

<style scoped>
/* Markdown 正文样式（v-html 内容无 scoped 类，用 :deep 穿透） */
/* Shiki 语法配色由内置主题行内样式提供（vitesse 对），此处不映射 CSS token；
   仅保留结构样式（容器/头部/行内码） */
.md-body :deep(h1),
.md-body :deep(h2),
.md-body :deep(h3),
.md-body :deep(h4) {
  font-weight: 600;
  margin: 0.75em 0 0.375em;
  line-height: 1.3;
}
.md-body :deep(h1) { font-size: 1.25em; }
.md-body :deep(h2) { font-size: 1.125em; }
.md-body :deep(h3) { font-size: 1em; }
.md-body :deep(p) { margin: 0.375em 0; }
.md-body :deep(ul),
.md-body :deep(ol) {
  margin: 0.375em 0;
  padding-left: 1.5em;
  list-style: revert;
}
.md-body :deep(li) { margin: 0.125em 0; }
.md-body :deep(a) {
  color: var(--mobile-accent);
  text-decoration: underline;
}
.md-body :deep(blockquote) {
  border-left: 3px solid var(--mobile-border);
  padding-left: 0.75em;
  color: var(--mobile-text-secondary);
  margin: 0.375em 0;
}
.md-body :deep(code) {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 0.875em;
  background: var(--mobile-bg-tertiary);
  padding: 0.125em 0.375em;
  border-radius: 0.25rem;
}
.md-body :deep(pre) {
  position: relative;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  /* 顶部为代码块头部（语言标签 + 复制按钮）预留空间 */
  padding: 2rem 0.75rem 0.75rem;
  overflow-x: auto;
  margin: 0.5em 0;
}
.md-body :deep(pre code) {
  background: transparent;
  padding: 0;
  /* 字体大小由插件级配置 codeFontSize 决定（CSS 变量在消息根节点下发） */
  font-size: var(--md-code-font-size, 0.8125rem);
  /* 行距由插件级配置 codeLineHeight 决定（CSS 变量在消息根节点下发） */
  line-height: var(--md-code-lh, 0.7);
}
/* Shiki 行结构：行块不换行（横向滚动由 pre 承担） */
.md-body :deep(pre code .line) {
  display: block;
  white-space: pre;
}
.md-body :deep(.md-code-header) {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 0.5rem;
  border-bottom: 1px solid var(--mobile-border);
  border-radius: 0.75rem 0.75rem 0 0;
  background: var(--mobile-bg-tertiary);
}
.md-body :deep(.md-code-lang) {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 0.6875rem;
  color: var(--mobile-text-muted);
  text-transform: lowercase;
}
.md-body :deep(pre .md-copy-btn) {
  font-size: 0.6875rem;
  color: var(--mobile-text-muted);
  background: transparent;
  border: 1px solid var(--mobile-border);
  border-radius: 0.25rem;
  padding: 0.125rem 0.5rem;
  cursor: pointer;
  transition: color 0.15s;
}
.md-body :deep(pre .md-copy-btn:active) {
  color: var(--mobile-text-secondary);
}

/* ==================== 思考过程块（P3） ==================== */
/* 次级样式：独立于正文的弱化容器，左缘品牌色竖条区分于代码块/引用 */
.thinking-block {
  border: 1px solid var(--mobile-border);
  border-left: 3px solid var(--mobile-accent);
  background: var(--mobile-bg-tertiary);
  border-radius: 0.75rem;
  overflow: hidden;
  margin: 0.5em 0;
}
.thinking-toggle {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  width: 100%;
  padding: 0.375rem 0.625rem;
  font-size: 0.75rem;
  color: var(--mobile-text-secondary);
  background: transparent;
  cursor: pointer;
  transition: color 0.15s;
  /* 触控目标 ≥ 44px（frontend-styles MOBILE.md 最小触控规范） */
  min-height: 2.75rem;
}
.thinking-toggle:active {
  color: var(--mobile-text-primary);
}
.thinking-body {
  padding: 0 0.625rem 0.625rem 1.375rem;
  font-size: 0.75rem;
  line-height: 1.6;
  color: var(--mobile-text-muted);
  white-space: pre-wrap;
  word-break: break-word;
  /* 长思考过程限高内滚，避免占满整条消息 */
  max-height: 18rem;
  overflow-y: auto;
}
.md-body :deep(table) {
  border-collapse: collapse;
  margin: 0.5em 0;
}
.md-body :deep(th),
.md-body :deep(td) {
  border: 1px solid var(--mobile-border);
  padding: 0.375em 0.75em;
  text-align: left;
}
.md-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--mobile-border);
  margin: 0.75em 0;
}
</style>
