<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 配置页模式（全屏覆盖） -->
    <ProviderConfigPage
      v-if="showConfigPage"
      class="w-full h-full"
      :providers="providers"
      :active-provider-id="activeProviderId"
      :fetch-models="config.fetchModels"
      :test-connection="config.testConnection"
      @back="showConfigPage = false"
      @add="addProvider"
      @update="updateProvider"
      @remove="removeProvider"
    />

    <!-- 聊天模式 -->
    <template v-else>
      <!-- 头部工具条：对话列表 + 标题 + 指令/设置 -->
      <header class="mobile-header-safe flex items-center justify-between px-2 pb-2 pt-1 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl">
        <button
          class="w-11 h-11 -ml-1 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
          :title="t('mobile.plugin.aiChatbox.conversations')"
          @click="showConversationSheet = true"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
          </svg>
        </button>

        <div class="flex items-center gap-1.5 min-w-0 flex-1 justify-center">
          <span class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate max-w-[12rem]">
            {{ currentTitle }}
          </span>
          <span
            v-if="currentConversation?.systemPrompt"
            class="flex-shrink-0 text-[10px] px-1.5 py-0.5 rounded-md bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-muted)]"
            :title="t('mobile.plugin.aiChatbox.systemPrompt')"
          >
            {{ t('mobile.plugin.aiChatbox.systemPromptOn') }}
          </span>
        </div>

        <div class="flex items-center">
          <button
            v-if="hasProvider"
            class="w-11 h-11 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
            :title="t('mobile.plugin.aiChatbox.systemPrompt')"
            @click="showSystemPromptEditor = !showSystemPromptEditor"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14 2v6h6" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 13H8" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 17H8" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 9H8" />
            </svg>
          </button>
          <button
            class="w-11 h-11 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
            :title="t('mobile.plugin.aiChatbox.providerConfig')"
            @click="showConfigPage = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </button>
        </div>
      </header>

      <!-- system prompt 编辑器（内联面板） -->
      <div
        v-if="showSystemPromptEditor"
        class="px-4 py-3 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-card)]"
      >
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs font-medium text-[var(--mobile-text-secondary)]">
            {{ t('mobile.plugin.aiChatbox.systemPrompt') }}
          </span>
          <button
            class="h-8 px-2 text-xs text-[var(--mobile-accent)] active:opacity-80"
            @click="clearSystemPrompt"
          >
            {{ t('mobile.plugin.aiChatbox.clear') }}
          </button>
        </div>
        <textarea
          v-model="systemPromptDraft"
          rows="3"
          class="w-full px-3 py-2 text-[var(--font-size-sm)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-border)] rounded-xl placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none focus:border-[var(--mobile-input-focus)] transition-colors"
          :placeholder="t('mobile.plugin.aiChatbox.systemPromptPlaceholder')"
        ></textarea>
        <div class="flex justify-end gap-2 mt-2">
          <button
            class="h-11 px-4 text-xs rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
            @click="showSystemPromptEditor = false"
          >
            {{ t('mobile.plugin.aiChatbox.cancel') }}
          </button>
          <button
            class="h-11 px-4 text-xs rounded-xl bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 transition-opacity"
            @click="applySystemPrompt"
          >
            {{ t('mobile.plugin.aiChatbox.save') }}
          </button>
        </div>
      </div>

      <!-- 未配置供应商 -->
      <div v-if="!hasProvider" class="flex-1 flex flex-col items-center justify-center p-6 text-center">
        <div class="w-16 h-16 rounded-2xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] flex items-center justify-center mb-4 text-[var(--mobile-text-secondary)]">
          <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
            <path d="M12 8V4H8" />
            <rect width="16" height="12" x="4" y="8" rx="2" />
            <path d="M2 14h2" />
            <path d="M20 14h2" />
            <path d="M15 13v2" />
            <path d="M9 13v2" />
          </svg>
        </div>
        <p class="text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] mb-1">{{ t('mobile.plugin.aiChatbox.pleaseConfigure') }}</p>
        <p class="text-xs text-[var(--mobile-text-muted)] mb-4">{{ t('mobile.plugin.aiChatbox.emptyHint') }}</p>
        <button
          class="h-12 px-5 text-[var(--font-size-base)] bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 rounded-xl transition-opacity"
          @click="showConfigPage = true"
        >
          {{ t('mobile.plugin.aiChatbox.configureModel') }}
        </button>
      </div>

      <template v-else>
        <!-- 消息区 -->
        <div ref="messagesContainer" class="flex-1 overflow-y-auto p-4 space-y-4 overscroll-behavior-none">
          <div v-if="messages.length === 0" class="flex flex-col items-center justify-center h-full text-center">
            <div class="w-14 h-14 rounded-full bg-[var(--mobile-bg-tertiary)] flex items-center justify-center mb-3 text-[var(--mobile-text-muted)]">
              <svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </div>
            <p class="text-[var(--font-size-sm)] text-[var(--mobile-text-muted)]">{{ t('mobile.plugin.aiChatbox.startNewChat') }}</p>
          </div>

          <!-- 全局错误条（授权失效/请求失败等） -->
          <div
            v-if="visibleError"
            class="flex items-center gap-2 px-3 py-2.5 text-xs rounded-xl border border-[var(--mobile-error)]/30 bg-[var(--mobile-error-muted)] text-[var(--mobile-error)]"
          >
            <span class="flex-1">{{ visibleError }}</span>
            <button class="w-8 h-8 flex items-center justify-center text-[var(--mobile-text-muted)] active:opacity-80" @click="dismissError">
              ✕
            </button>
          </div>

          <ChatMessage
            v-for="(msg, i) in messages"
            :key="i"
            :message="msg"
            :streaming="isStreaming && i === messages.length - 1"
            :error-text="i === messages.length - 1 ? messageErrorText : ''"
            @delete="onDeleteMessage"
          />

          <!-- 重新生成（最后一条是 assistant 且非流式时） -->
          <div v-if="canRegenerate" class="flex justify-center">
            <button
              class="h-11 px-4 text-xs rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
              @click="regenerate"
            >
              {{ t('mobile.plugin.aiChatbox.regenerate') }}
            </button>
          </div>
        </div>

        <!-- 输入区：模型 pill + 输入框内联（DeepSeek/Claude 式，键盘避让 safe area） -->
        <div class="mobile-input-bar border-t border-[var(--mobile-border)] px-3 pt-2 pb-[max(0.75rem,env(safe-area-inset-bottom))] bg-[var(--mobile-bg-secondary)]/95 backdrop-blur-xl">
          <ChatInput
            :disabled="sending || !hasProvider"
            :streaming="isStreaming"
            :placeholder="t('mobile.plugin.aiChatbox.inputPlaceholder')"
            :model-value="activeModel"
            :model-options="modelOptions"
            :show-model="hasProvider"
            @send="sendMessage"
            @stop="stopGeneration"
            @update:model-value="onModelChange"
          />
        </div>
      </template>

      <!-- 对话列表面板（底部抽屉） -->
      <Teleport to="body">
        <Transition name="sheet">
          <div v-if="showConversationSheet" class="fixed inset-0 z-50">
            <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="showConversationSheet = false"></div>
            <div class="sheet-panel absolute bottom-0 left-0 right-0 max-h-[70vh] flex flex-col rounded-t-2xl overflow-hidden bg-[var(--mobile-bg-card)] shadow-[var(--mobile-card-shadow)]">
              <div class="w-10 h-1 rounded-full mx-auto mt-2 mb-1 flex-shrink-0 bg-[var(--mobile-bg-tertiary)]"></div>
              <div class="flex-1 overflow-hidden">
                <ConversationList
                  :conversations="conversations"
                  :current-id="currentConvId"
                  :loading="loadingHistory"
                  @select="onSelectConversation"
                  @new="onNewConversation"
                  @rename="onRenameConversation"
                  @delete="onDeleteConversation"
                />
              </div>
            </div>
          </div>
        </Transition>
      </Teleport>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * AI Chatbox 面板（移动端 navtab + toolbox 共用）— 消息流 + 输入区 + 对话列表抽屉 + 供应商配置
 */
import { ref, computed, watch, nextTick, onMounted, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ConversationList from './ConversationList.vue'
import ProviderConfigPage from './ProviderConfigPage.vue'
import { useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { ChatMessage as ChatMessageType, ConversationMeta } from '../types'

const { t } = useI18n()

// 宿主注入 PluginContext（PluginViewHost provide）
const context = inject<PluginContext>('pluginContext')!

const config = useAiConfig(context)
const chat = useAiChat(context, config)

const {
  providers,
  activeProviderId,
  activeProvider,
  activeModel,
  hasProvider,
  loadConfig,
  addProvider,
  updateProvider,
  removeProvider,
  setActiveModel,
} = config

const {
  conversations,
  currentConvId,
  currentConversation,
  messages,
  sending,
  isStreaming,
  loadingHistory,
  lastError,
  loadConversations,
  newConversation,
  renameConversation,
  deleteConversation,
  sendMessage,
  stopGeneration,
  regenerate,
  switchConversation,
  setSystemPrompt,
} = chat

const messagesContainer = ref<HTMLElement | null>(null)
const showConfigPage = ref(false)
const showConversationSheet = ref(false)
const showSystemPromptEditor = ref(false)
const systemPromptDraft = ref('')
const dismissedError = ref('')

/** 当前对话标题（无对话选中时显示面板名；新对话占位显示默认文案） */
const currentTitle = computed(() => {
  if (!currentConversation.value) {
    return t('mobile.plugin.aiChatbox.title')
  }
  const title = currentConversation.value.title
  if (!title || title === 'mobile.plugin.aiChatbox.newConversation') {
    return t('mobile.plugin.aiChatbox.newConversation')
  }
  return title
})

const modelOptions = computed(() => activeProvider.value?.models.map(m => ({ value: m, label: m })) || [])

/** 最近一条消息的错误文本（assistant 空内容时显示） */
const messageErrorText = computed(() => {
  const last = messages.value[messages.value.length - 1]
  if (!last || last.role !== 'assistant' || last.content) return ''
  return lastError.value.startsWith('mobile.plugin.')
    ? t(lastError.value)
    : lastError.value
})

/** 全局错误条（请求失败/授权失效，非单消息错误） */
const visibleError = computed(() => {
  if (!lastError.value) return ''
  if (lastError.value === dismissedError.value) return ''
  return lastError.value.startsWith('mobile.plugin.')
    ? t(lastError.value)
    : lastError.value
})

const canRegenerate = computed(() =>
  !sending.value &&
  messages.value.length > 0 &&
  messages.value[messages.value.length - 1].role === 'assistant'
)

function onModelChange(value: string | number): void {
  setActiveModel(String(value))
}

async function onNewConversation(): Promise<void> {
  if (!hasProvider.value) return
  showConversationSheet.value = false
  await newConversation()
}

/** 切换对话：关闭抽屉后再加载（避免抽屉遮挡消息区滚动动画） */
async function onSelectConversation(id: string): Promise<void> {
  showConversationSheet.value = false
  await switchConversation(id)
}

async function onRenameConversation(conv: ConversationMeta, title: string): Promise<void> {
  await renameConversation(conv.id, title)
}

/** 删除对话：移动端先确认（宿主弹窗）再删 */
async function onDeleteConversation(conv: ConversationMeta): Promise<void> {
  const ok = await context.dialogs.showConfirm({
    title: t('mobile.plugin.aiChatbox.delete'),
    message: t('mobile.plugin.aiChatbox.confirmDeleteShort'),
    confirmText: t('mobile.plugin.aiChatbox.delete'),
    variant: 'danger',
  })
  if (!ok) return
  await deleteConversation(conv.id)
}

/** 删除单条消息（仅前端会话内删除；文件为 append-only 日志，保留历史） */
function onDeleteMessage(msg: ChatMessageType): void {
  const idx = messages.value.indexOf(msg)
  if (idx !== -1) {
    messages.value.splice(idx, 1)
  }
}

function dismissError(): void {
  dismissedError.value = lastError.value
}

async function applySystemPrompt(): Promise<void> {
  await setSystemPrompt(systemPromptDraft.value)
  showSystemPromptEditor.value = false
}

async function clearSystemPrompt(): Promise<void> {
  systemPromptDraft.value = ''
  await setSystemPrompt('')
}

// 自动滚动到底部
watch(() => messages.value.length + (chat.streamingContent.value?.length || 0), () => {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
    }
  })
})

onMounted(async () => {
  await loadConfig()
  await loadConversations()
})
</script>

<style scoped>
/* 底部抽屉过渡：位移 + 淡入（GPU 合成属性） */
.sheet-enter-active,
.sheet-leave-active {
  transition: opacity 0.2s ease;
}
.sheet-enter-active .sheet-panel,
.sheet-leave-active .sheet-panel {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.sheet-enter-from,
.sheet-leave-to {
  opacity: 0;
}
.sheet-enter-from .sheet-panel,
.sheet-leave-to .sheet-panel {
  transform: translateY(100%);
}
</style>
