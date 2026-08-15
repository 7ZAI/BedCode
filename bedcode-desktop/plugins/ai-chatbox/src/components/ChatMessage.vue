<template>
  <!-- user 消息右对齐，assistant 消息左对齐（flex-row-reverse 实现左右分列） -->
  <div
    class="group flex gap-3"
    :class="[isUser ? 'flex-row-reverse' : '', themeClass]"
    :style="codeStyle"
  >
    <!-- 头像：user 人形图标；assistant 受支持供应商显示品牌 logo，其余 bot 图标 -->
    <div
      class="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0 text-sm mt-0.5"
      :class="isUser
        ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
        : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'"
    >
      <svg v-if="isUser" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
      </svg>
      <ProviderAvatar
        v-else-if="assistantProvider?.presetId"
        :preset-id="assistantProvider.presetId"
        :name="assistantProvider.name"
        :size="32"
      />
      <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M9 17v2a2 2 0 002 2h2a2 2 0 002-2v-2M9 4h6M5 12h14a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4a1 1 0 011-1z" />
      </svg>
    </div>

    <div class="flex-1 min-w-0 space-y-1" :class="isUser ? 'text-right' : ''">
      <!-- 元信息行（user 反向排列；身份由头像 logo/图标表明，不再重复文字） -->
      <div class="flex items-center gap-2 text-xs text-[var(--text-tertiary)]" :class="isUser ? 'flex-row-reverse' : ''">
        <span v-if="message.model" class="font-mono">{{ message.model }}</span>
        <!-- token 用量 -->
        <span v-if="message.usage" class="font-mono rounded-tag bg-[var(--bg-hover)] px-1.5 py-0.5">
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
          :title="t('desktop.plugin.aiChatbox.thinkingProcess')"
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
          <span class="truncate">{{ t('desktop.plugin.aiChatbox.thinkingProcess') }}</span>
        </button>
        <!-- 思考内容为模型 scratchpad 草稿，非成品 Markdown：纯文本展示（预换行），
             不经过渲染管线，天然免疫 prompt injection 的 HTML 注入 -->
        <div v-if="reasoningExpanded" class="thinking-body">{{ message.reasoning }}</div>
      </div>

      <!-- 内容：user / assistant 均 Markdown 渲染（主流布局）；user 为右对齐气泡，assistant 全宽文本 -->
      <div
        v-if="isUser"
        class="md-body inline-block max-w-[85%] text-left rounded-input px-3.5 py-2.5 border border-[var(--border-input)] bg-[var(--bg-input)]"
        v-html="rendered"
      />
      <div v-else ref="contentRef" class="text-sm leading-relaxed text-[var(--text-primary)] md-body" v-html="rendered" />

      <!-- 错误提示（assistant 无内容且带错误时） -->
      <div
        v-if="!isUser && !message.content && errorText"
        class="text-xs text-[var(--color-danger)]"
      >{{ errorText }}</div>

      <!-- 底部操作行（悬停显示；复制/删除置于消息末尾，主流布局） -->
      <div class="flex justify-end">
        <div class="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-0.5 px-1 py-0.5 rounded-btn bg-[var(--bg-card)] border border-[var(--border)] shadow-sm">
          <button
            class="p-1 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] rounded transition-colors"
            :title="t('desktop.plugin.aiChatbox.copyMessage')"
            @click="copyContent"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3" />
            </svg>
          </button>
          <button
            class="p-1 text-[var(--text-tertiary)] hover:text-[var(--color-danger)] rounded transition-colors"
            :title="t('desktop.plugin.aiChatbox.deleteMessage')"
            @click="$emit('delete', message)"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
      </div>

      <!-- 流式光标 -->
      <span
        v-if="streaming"
        class="inline-block w-2 h-4 ml-0.5 align-middle bg-[var(--color-primary)] animate-pulse"
      ></span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatMessage — 单条聊天消息（桌面端）
 *
 * user / assistant 均 Markdown 渲染（marked + highlight.js 代码高亮，DOMPurify 消毒），
 * 与主流聊天应用一致：复制/删除操作置于消息末尾（悬停显示），
 * 代码块带语言标签 + 一键复制头部。
 */
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import type { ChatMessage, CodeTheme } from '../types'
import { getClosedCodeBlocks, patchIncompleteMarkdown } from '../utils/markdown'
import { createHljsHighlightEngine, type HighlightEngine } from '../utils/highlight'
import ProviderAvatar from './ProviderAvatar.vue'

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
  /** 当前供应商（assistant 头像 logo 来源；无 presetId 时显示 bot 图标） */
  assistantProvider?: { presetId?: string; name: string } | null
}>()

defineEmits<{ delete: [message: ChatMessage] }>()

const { t } = useI18n()

// 高亮引擎 seam（ADR-0011）：桌面注入 hljs 同步实现；移动端换 Shiki 异步实现
const highlightEngine: HighlightEngine = createHljsHighlightEngine()

const isUser = computed(() => props.message.role === 'user')

/** 代码块行距档位 → line-height（CSS 变量下发，:deep 样式消费） */
const codeStyle = computed(() => ({
  '--md-code-lh': String(props.codeLineHeight ?? 1.6),
  // 代码块字体大小（px，CSS 变量下发，:deep 样式消费）
  '--md-code-font-size': `${props.codeFontSize ?? 13}px`,
}))

/** 宿主深浅色（MutationObserver 同步 html.dark 变化，驱动 auto 主题解析） */
const hostDark = ref(false)
/** 代码主题类：auto 解析为实际深浅（跟随宿主）；具名主题直接映射 */
const themeClass = computed(() => {
  const theme = props.codeTheme ?? 'auto'
  if (theme !== 'auto') return `code-theme-${theme}`
  return hostDark.value ? 'code-theme-dark' : 'code-theme-light'
})

let themeObserver: MutationObserver | null = null
onMounted(() => {
  // auto 语义 = 跟随宿主：html.dark 类变化时同步解析结果（CSS 变量组随类切换）
  hostDark.value = document.documentElement.classList.contains('dark')
  themeObserver = new MutationObserver(() => {
    hostDark.value = document.documentElement.classList.contains('dark')
  })
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
})
onUnmounted(() => {
  themeObserver?.disconnect()
  themeObserver = null
})

/** 思考块展开状态：流式期间默认展开（边生成边可见）；结束后保持用户当前折叠状态 */
const reasoningExpanded = ref(false)
watch(
  () => props.streaming,
  (streaming) => {
    if (streaming) reasoningExpanded.value = true
  },
  { immediate: true },
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
 * 按钮盖住仍在生长的代码块，也避免对增长中的块反复重扫（P2 延迟高亮） */
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
    // v-html 每帧重建 DOM，classList 检查仅在帧内重复触发时生效
    if (code && !code.classList.contains('hljs')) {
      highlightEngine.highlightElement(code)
    }
    if (pre.querySelector('.md-code-header')) return
    // 语言取自 marked 实际渲染的 language-* 类（HTML 解析器已解码实体），与
    // getClosedCodeBlocks.lang 同源；按 classList 取首 token，避免 "js,x" 等
    // 含标点的语言被 \w 正则截断（原有反解只显示前缀）
    const lang =
      Array.from(code?.classList ?? [])
        .find(c => c.startsWith('language-'))
        ?.slice('language-'.length) ?? ''
    const header = document.createElement('div')
    header.className = 'md-code-header'
    const langEl = document.createElement('span')
    langEl.className = 'md-code-lang'
    langEl.textContent = lang
    const btn = document.createElement('button')
    btn.className = 'md-copy-btn'
    btn.textContent = t('desktop.plugin.aiChatbox.copy')
    btn.addEventListener('click', () => {
      const text = code?.innerText ?? ''
      navigator.clipboard.writeText(text).catch(() => {})
      btn.textContent = t('desktop.plugin.aiChatbox.copied')
      setTimeout(() => {
        btn.textContent = t('desktop.plugin.aiChatbox.copy')
      }, 1500)
    })
    header.append(langEl, btn)
    pre.prepend(header)
  })
}

/** 复制整条消息 */
async function copyContent(): Promise<void> {
  await navigator.clipboard.writeText(props.message.content).catch(() => {})
}

onMounted(enhanceCodeBlocks)
// flush: 'post'：等组件 DOM patch 完成后再注入——默认 'pre' 的 watch 会在新 DOM
// 渲染前执行，注入落在上一帧 DOM 上、随后被 v-html 整段覆盖（宿主
// TerminalPreview 在 watch 回调里 nextTick 后操作 DOM 的惯例同理）
watch(() => props.message.content, enhanceCodeBlocks, { flush: 'post' })
</script>

<style scoped>
/* Markdown 正文样式（v-html 内容无 scoped 类，用 :deep 穿透） */
/* highlight.js 语法高亮配色：token 类 → CSS 变量（--hl-*），每套主题在根节点
 * code-theme-* 类上定义变量组；未定义时回落浅色基准值。容器背景/边框同样
 * 主题化（--hl-bg/--hl-border/--hl-header），auto 由 themeClass 解析为
 * code-theme-light/dark 跟随宿主（html.dark MutationObserver 驱动） */
.md-body :deep(.hljs-comment),
.md-body :deep(.hljs-quote) {
  color: var(--hl-comment, #64748b);
  font-style: italic;
}
.md-body :deep(.hljs-keyword),
.md-body :deep(.hljs-selector-tag) { color: var(--hl-keyword, #8a3b2e); }
.md-body :deep(.hljs-type),
.md-body :deep(.hljs-class) { color: var(--hl-type, #2f6f6a); }
.md-body :deep(.hljs-string),
.md-body :deep(.hljs-attr),
.md-body :deep(.hljs-template-variable) { color: var(--hl-string, #5a7a2f); }
.md-body :deep(.hljs-number),
.md-body :deep(.hljs-literal) { color: var(--hl-number, #a05a2c); }
.md-body :deep(.hljs-title),
.md-body :deep(.hljs-function) { color: var(--hl-title, #8a5a1d); }
.md-body :deep(.hljs-built_in) { color: var(--hl-builtin, #7a4a6b); }
.md-body :deep(.hljs-meta) { color: var(--hl-meta, #4b5563); }

/* 主题变量组：通用浅/深色 + GitHub 双套 + Dracula（背景/边框/头部随主题）。
 * 深色通用主题采用 One Dark / DeepSeek 深色观感：柔和深灰背景（非纯黑）+
 * 高对比 token（紫/绿/蓝/黄/橙），注释降饱和。具名主题保持官方色。
 * --hl-fg = 代码块基础文字色（未高亮文本/未覆盖 token 类兜底）：强制主题与
 * 宿主深浅相反时保证文字始终可见，不继承宿主正文色；--hl-header-fg 同理
 * 用于头部语言标签与复制按钮 */
.code-theme-light {
  --hl-bg: #f7f7f5;
  --hl-border: #e5e4e1;
  --hl-header: #eef0f1;
  --hl-fg: #1f2328;
  --hl-header-fg: #57606a;
  --hl-header-border: #d0d7de;
}
.code-theme-dark {
  --hl-bg: #26262b;
  --hl-border: #3a3a41;
  --hl-header: #2e2e35;
  --hl-fg: #e6e6e6;
  --hl-header-fg: #9aa0a6;
  --hl-header-border: #3a3a41;
  --hl-comment: #8a9199;
  --hl-keyword: #c678dd;
  --hl-type: #e5c07b;
  --hl-string: #98c379;
  --hl-number: #d19a66;
  --hl-title: #61afef;
  --hl-builtin: #56b6c2;
  --hl-meta: #7f848e;
}
.code-theme-github-light {
  --hl-bg: #f6f8fa;
  --hl-border: #d0d7de;
  --hl-header: #eaeef2;
  --hl-fg: #1f2328;
  --hl-header-fg: #57606a;
  --hl-header-border: #d0d7de;
  --hl-comment: #6e7781;
  --hl-keyword: #cf222e;
  --hl-type: #953800;
  --hl-string: #0a3069;
  --hl-number: #0550ae;
  --hl-title: #8250df;
  --hl-builtin: #8250df;
  --hl-meta: #6e7781;
}
.code-theme-github-dark {
  --hl-bg: #0d1117;
  --hl-border: #30363d;
  --hl-header: #161b22;
  --hl-fg: #e6edf3;
  --hl-header-fg: #8b949e;
  --hl-header-border: #30363d;
  --hl-comment: #8b949e;
  --hl-keyword: #ff7b72;
  --hl-type: #ffa657;
  --hl-string: #a5d6ff;
  --hl-number: #79c0ff;
  --hl-title: #d2a8ff;
  --hl-builtin: #d2a8ff;
  --hl-meta: #8b949e;
}
.code-theme-dracula {
  --hl-bg: #282a36;
  --hl-border: #44475a;
  --hl-header: #2f3242;
  --hl-fg: #f8f8f2;
  --hl-header-fg: #8be9fd;
  --hl-header-border: #44475a;
  --hl-comment: #6272a4;
  --hl-keyword: #ff79c6;
  --hl-type: #8be9fd;
  --hl-string: #f1fa8c;
  --hl-number: #bd93f9;
  --hl-title: #50fa7b;
  --hl-builtin: #8be9fd;
  --hl-meta: #6272a4;
}

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
  color: var(--color-primary);
  text-decoration: underline;
}
.md-body :deep(blockquote) {
  border-left: 3px solid var(--border);
  padding-left: 0.75em;
  color: var(--text-secondary);
  margin: 0.375em 0;
}
.md-body :deep(code) {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 0.875em;
  background: var(--bg-hover);
  padding: 0.125em 0.375em;
  border-radius: 0.25rem;
}
.md-body :deep(pre) {
  position: relative;
  /* 容器背景/边框随代码主题（--hl-* 变量组）；auto 未定义时回落宿主 token */
  background: var(--hl-bg, var(--bg-hover));
  border: 1px solid var(--hl-border, var(--border));
  border-radius: 0.5rem;
  /* 顶部为代码块头部（语言标签 + 复制按钮）预留空间 */
  padding: 2.25rem 0.75rem 0.75rem;
  overflow-x: auto;
  margin: 0.5em 0;
}
.md-body :deep(pre code) {
  background: transparent;
  padding: 0;
  /* 基础文字色随代码主题（--hl-fg）：未高亮文本/未覆盖 token 类在强制主题与
   * 宿主深浅相反时仍保持可见，不继承宿主正文色 */
  color: var(--hl-fg, inherit);
  /* 字体大小由插件级配置 codeFontSize 决定（CSS 变量在消息根节点下发） */
  font-size: var(--md-code-font-size, 0.8125rem);
  /* 行距由插件级配置 codeLineHeight 决定（CSS 变量在消息根节点下发） */
  line-height: var(--md-code-lh, 1.6);
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
  border-bottom: 1px solid var(--border);
  border-radius: 0.5rem 0.5rem 0 0;
  background: var(--hl-header, var(--bg-input));
}
.md-body :deep(.md-code-lang) {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 0.6875rem;
  color: var(--hl-header-fg, var(--text-tertiary));
  text-transform: lowercase;
}
.md-body :deep(pre .md-copy-btn) {
  font-size: 0.6875rem;
  color: var(--hl-header-fg, var(--text-tertiary));
  background: transparent;
  border: 1px solid var(--hl-header-border, var(--border));
  border-radius: 0.25rem;
  padding: 0.125rem 0.5rem;
  cursor: pointer;
  transition: color 0.15s;
}
.md-body :deep(pre .md-copy-btn:hover) {
  color: var(--hl-fg, var(--text-secondary));
}

/* ==================== 思考过程块（P3） ==================== */
/* 次级样式：独立于正文的弱化容器，左缘品牌色竖条区分于代码块/引用 */
.thinking-block {
  border: 1px solid var(--border);
  border-left: 3px solid var(--color-primary);
  background: var(--bg-hover);
  border-radius: 0.5rem;
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
  color: var(--text-secondary);
  background: transparent;
  cursor: pointer;
  transition: color 0.15s;
}
.thinking-toggle:hover {
  color: var(--text-primary);
}
.thinking-body {
  padding: 0 0.625rem 0.625rem 1.375rem;
  font-size: 0.75rem;
  line-height: 1.6;
  color: var(--text-tertiary);
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
  border: 1px solid var(--border);
  padding: 0.375em 0.75em;
  text-align: left;
}
.md-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border);
  margin: 0.75em 0;
}
</style>
