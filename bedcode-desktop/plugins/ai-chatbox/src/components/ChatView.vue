<template>
  <div class="h-full flex bg-[var(--bg-page)]">
    <!-- 配置页 / 聊天区切换（淡入淡出 + 轻微位移，避免闪现） -->
    <Transition name="page-fade" mode="out-in">
      <ProviderConfigPage
        v-if="showConfigPage"
        key="config"
        class="w-full"
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
      <div v-else key="chat" class="flex w-full min-w-0">
        <!-- 对话列表（可折叠：展开 w-52 / 折叠 w-9 窄条） -->
      <div
        class="flex-shrink-0 overflow-hidden transition-[width] duration-200 ease-in-out"
        :class="sidebarCollapsed ? 'w-9' : 'w-52'"
      >
        <ConversationList
          :conversations="conversations"
          :current-id="currentConvId"
          :loading="loadingHistory"
          :collapsed="sidebarCollapsed"
          @select="switchConversation"
          @new="onNewConversation"
          @rename="onRenameConversation"
          @delete="onDeleteConversation"
          @toggle-collapse="toggleSidebar"
        />
      </div>

      <!-- 聊天区 -->
      <div class="flex-1 flex flex-col min-w-0">
        <!-- 头部：对话标题 + 供应商/模型 + 设置 -->
        <header class="px-4 py-2 flex items-center justify-between border-b border-[var(--border)] bg-[var(--bg-card)]">
          <div class="flex items-center gap-2 min-w-0">
            <span
              class="text-sm font-medium text-[var(--text-primary)] truncate max-w-[10rem]"
              :title="currentTitle"
            >
              {{ currentTitle }}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <button
              v-if="hasProvider"
              class="p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors flex-shrink-0"
              :title="t('desktop.plugin.aiChatbox.providerConfig')"
              @click="showConfigPage = true"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
          </div>
        </header>

        <!-- 未配置供应商 -->
        <div v-if="!hasProvider" class="flex-1 flex flex-col items-center justify-center p-6 text-center">
          <div class="w-14 h-14 rounded-card bg-[var(--bg-card)] border border-[var(--border)] flex items-center justify-center mb-4 text-[var(--text-secondary)]">
            <svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
              <path d="M12 8V4H8" />
              <rect width="16" height="12" x="4" y="8" rx="2" />
              <path d="M2 14h2" />
              <path d="M20 14h2" />
              <path d="M15 13v2" />
              <path d="M9 13v2" />
            </svg>
          </div>
          <p class="text-sm text-[var(--text-secondary)] mb-1">{{ t('desktop.plugin.aiChatbox.pleaseConfigure') }}</p>
          <p class="text-xs text-[var(--text-tertiary)] mb-4">{{ t('desktop.plugin.aiChatbox.emptyHint') }}</p>
          <button
            class="px-4 py-2 text-sm bg-brand text-[var(--color-primary-contrast)] hover:bg-brand-hover rounded-btn transition-colors"
            @click="showConfigPage = true"
          >
            {{ t('desktop.plugin.aiChatbox.configureModel') }}
          </button>
        </div>

        <template v-else>
          <!-- 消息区 -->
          <div ref="messagesContainer" class="flex-1 overflow-y-auto p-4 space-y-4">
            <div v-if="messages.length === 0" class="flex flex-col items-center justify-center h-full text-center">
              <div class="w-12 h-12 rounded-full bg-[var(--bg-hover)] flex items-center justify-center mb-3 text-[var(--text-tertiary)]">
                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
                  <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                </svg>
              </div>
              <p class="text-sm text-[var(--text-tertiary)]">{{ t('desktop.plugin.aiChatbox.startNewChat') }}</p>
            </div>

            <!-- 全局错误条（授权失效/请求失败等） -->
            <div
              v-if="visibleError"
              class="flex items-center gap-2 px-3 py-2 text-xs rounded-btn border border-[var(--color-danger)]/30 bg-[var(--color-danger-light)] text-[var(--color-danger)]"
            >
              <span class="flex-1">{{ visibleError }}</span>
              <button class="text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]" @click="dismissError">
                ✕
              </button>
            </div>

            <ChatMessage
              v-for="(msg, i) in messages"
              :key="i"
              :message="msg"
              :streaming="isStreaming && i === messages.length - 1"
              :error-text="i === messages.length - 1 ? messageErrorText : ''"
              :show-reasoning="showReasoning"
              :code-line-height="pluginConfig.config.value.codeLineHeight"
              :code-font-size="pluginConfig.config.value.codeFontSize"
              :code-theme="pluginConfig.config.value.codeTheme"
              :assistant-provider="activeProvider"
              @delete="onDeleteMessage"
            />

            <!-- 重新生成（最后一条是 assistant 且非流式时） -->
            <div v-if="canRegenerate" class="flex justify-center">
              <button
                class="px-3 h-7 text-xs rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors"
                :title="t('desktop.plugin.aiChatbox.regenerate')"
                @click="regenerate"
              >
                {{ t('desktop.plugin.aiChatbox.regenerate') }}
              </button>
            </div>
          </div>

          <!-- 输入区：模型切换与发送按钮同处输入框内（切换后新消息立即生效） -->
          <div class="border-t border-[var(--border)] p-3 bg-[var(--bg-card)]">
            <ChatInput
              :disabled="sending || !hasProvider"
              :streaming="isStreaming"
              :placeholder="t('desktop.plugin.aiChatbox.inputPlaceholder')"
              @send="sendMessage"
              @stop="stopGeneration"
            >
              <template #toolbar>
                <Select
                  :model-value="currentModelKey"
                  :options="modelOptions"
                  size="sm"
                  class="model-picker w-64 flex-shrink-0"
                  @update:model-value="onModelChange"
                />
              </template>
            </ChatInput>
          </div>
        </template>
      </div>
    </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
/**
 * AI Chatbox 侧边栏面板 — 对话列表 + 消息流 + 输入区 + 供应商配置
 */
import { ref, computed, watch, nextTick, onMounted, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ConversationList from './ConversationList.vue'
import ProviderConfigPage from './ProviderConfigPage.vue'
import Select from '@binblink/plugin-sdk-desktop/ui'
import { modelKey, useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'
import { usePluginConfig } from '../composables/usePluginConfig'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { ChatMessage as ChatMessageType, ConversationMeta } from '../types'

const { t } = useI18n()

// 宿主注入 PluginContext（PluginViewHost provide）
const context = inject<PluginContext>('pluginContext')!

const config = useAiConfig(context)
// 插件级全局配置（P3：thinkingMode/reasoningEffort/showReasoning，宿主配置页 schema 渲染）
const pluginConfig = usePluginConfig(context)
const chat = useAiChat(context, config, undefined, pluginConfig.config)

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
} = chat

const messagesContainer = ref<HTMLElement | null>(null)
const showConfigPage = ref(false)
const dismissedError = ref('')

/** 对话列表折叠态（persist 到插件 storage，跨会话记忆） */
const sidebarCollapsed = ref(false)
const SIDEBAR_COLLAPSED_KEY = 'chatSidebarCollapsed'

function toggleSidebar(): void {
  sidebarCollapsed.value = !sidebarCollapsed.value
  void context.storage.set(SIDEBAR_COLLAPSED_KEY, sidebarCollapsed.value)
}

/** 当前对话标题（无对话选中时显示面板名；新对话占位显示默认文案） */
const currentTitle = computed(() => {
  if (!currentConversation.value) {
    return t('desktop.plugin.aiChatbox.title')
  }
  const title = currentConversation.value.title
  if (!title || title === 'desktop.plugin.aiChatbox.newConversation') {
    return t('desktop.plugin.aiChatbox.newConversation')
  }
  return title
})

// 配置加载完成前按 false 处理：避免用户已设 showReasoning=false 时，历史消息的
// 思考块在首帧用默认值闪现后再消失（storage 读取为异步，与消息加载并行）
const showReasoning = computed(
  () => !pluginConfig.loading.value && pluginConfig.config.value.showReasoning,
)

/** 输入框模型选择：全供应商模型扁平化（供应商名 / 模型名 区分），value 为供应商限定复合键 */
const modelOptions = computed(() =>
  providers.value.flatMap(p =>
    p.models.map(m => ({ value: modelKey(p.id, m), label: `${p.name} / ${m}` })),
  ),
)

/** 当前选择在模型选择器中的复合键（无有效选择时为空串） */
const currentModelKey = computed(() =>
  activeProviderId.value && activeModel.value ? modelKey(activeProviderId.value, activeModel.value) : '',
)

/** 最近一条消息的错误文本（assistant 空内容时显示） */
const messageErrorText = computed(() => {
  const last = messages.value[messages.value.length - 1]
  if (!last || last.role !== 'assistant' || last.content) return ''
  return lastError.value.startsWith('desktop.plugin.')
    ? t(lastError.value)
    : lastError.value
})

/** 全局错误条（请求失败/授权失效，非单消息错误） */
const visibleError = computed(() => {
  if (!lastError.value) return ''
  if (lastError.value === dismissedError.value) return ''
  return lastError.value.startsWith('desktop.plugin.')
    ? t(lastError.value)
    : lastError.value
})

const canRegenerate = computed(() =>
  !sending.value &&
  messages.value.length > 0 &&
  messages.value[messages.value.length - 1].role === 'assistant'
)

function onModelChange(value: string | number): void {
  // 复合键（providerId::model），useAiConfig 内部解析并切换到对应供应商
  setActiveModel(String(value))
}

async function onNewConversation(): Promise<void> {
  if (!hasProvider.value) return
  await newConversation()
}

async function onRenameConversation(conv: ConversationMeta, title: string): Promise<void> {
  await renameConversation(conv.id, title)
}

async function onDeleteConversation(conv: ConversationMeta): Promise<void> {
  await deleteConversation(conv.id)
}

/** 删除单条消息（仅前端会话内删除；文件为 append-only 日志，保留历史） */
async function onDeleteMessage(msg: ChatMessageType): Promise<void> {
  const idx = messages.value.indexOf(msg)
  if (idx !== -1) {
    messages.value.splice(idx, 1)
  }
}

function dismissError(): void {
  dismissedError.value = lastError.value
}

// 自动滚动到底部（reasoning 流写入时正文可能仍为空，须一并跟踪才能跟上思考期增长）
watch(
  () =>
    messages.value.length +
    (chat.streamingContent.value?.length || 0) +
    (chat.streamingReasoning.value?.length || 0),
  () => {
    nextTick(() => {
      if (messagesContainer.value) {
        messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
      }
    })
  },
)

onMounted(async () => {
  await loadConfig()
  // 插件配置与供应商配置并行加载（缺失时 usePluginConfig 内部已回退默认值）
  await Promise.all([pluginConfig.loadConfig(), loadConversations()])
  // 恢复上次会话的列表折叠状态（存储缺失时保持展开）
  const saved = await context.storage.get<boolean>(SIDEBAR_COLLAPSED_KEY)
  if (saved !== null && saved !== undefined) {
    sidebarCollapsed.value = saved
  }
})
</script>

<style scoped>
/* 配置页 / 聊天区切换：淡入淡出 + 轻微纵向位移（mode="out-in" 先出后进） */
.page-fade-enter-active,
.page-fade-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.page-fade-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.page-fade-leave-to {
  opacity: 0;
  transform: translateY(-2px);
}
</style>
