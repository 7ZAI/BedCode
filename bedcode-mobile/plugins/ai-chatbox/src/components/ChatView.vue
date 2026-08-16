<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 配置页 / 聊天区切换（淡入淡出 + 轻微位移，避免闪现） -->
    <Transition name="page-fade" mode="out-in">
      <ProviderConfigPage
        v-if="showConfigPage"
        key="config"
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
      <div v-else key="chat" class="flex flex-col w-full h-full min-w-0">
        <!-- 头部工具条：对话列表 + 标题 + 新对话/指令/设置 -->
      <header class="flex items-center justify-between px-2 pb-2 pt-1 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl" :style="{ paddingTop: `${safeAreaTop}px` }">
        <button
          class="w-11 h-11 -ml-1 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
          :title="t('mobile.plugin.aiChatbox.conversations')"
          @click="showConversationDrawer = true"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
          </svg>
        </button>

        <div class="flex items-center gap-1.5 min-w-0 flex-1 justify-center">
          <span class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate max-w-[12rem]">
            {{ currentTitle }}
          </span>
        </div>

        <div class="flex items-center">
          <button
            v-if="hasProvider"
            class="w-11 h-11 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
            :title="t('mobile.plugin.aiChatbox.newConversation')"
            @click="onNewConversation"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
            </svg>
          </button>
          <button
            class="w-11 h-11 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
            :title="t('mobile.plugin.aiChatbox.pluginSettings')"
            @click="showSettingsSheet = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3" />
              <path stroke-linecap="round" stroke-linejoin="round" d="M14 2v4M8 10v4M16 18v4" />
            </svg>
          </button>
          <button
            class="w-11 h-11 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
            :title="t('mobile.plugin.aiChatbox.providerConfig')"
            @click="showConfigPage = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </button>
        </div>
      </header>

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
            :show-reasoning="showReasoning"
            :code-line-height="pluginConfig.config.value.codeLineHeight"
            :code-font-size="pluginConfig.config.value.codeFontSize"
            :code-theme="pluginConfig.config.value.codeTheme"
            :show-regenerate="canRegenerate && i === messages.length - 1"
            @delete="onDeleteMessage"
            @regenerate="regenerate"
          />
        </div>

        <!-- 输入区：模型 pill + 输入框内联（DeepSeek/Claude 式，键盘避让 safe area） -->
        <div class="mobile-input-bar border-t border-[var(--mobile-border)] px-3 pt-2 pb-[max(0.75rem,env(safe-area-inset-bottom))] bg-[var(--mobile-bg-secondary)]/95 backdrop-blur-xl">
          <ChatInput
            :disabled="sending || !hasProvider"
            :streaming="isStreaming"
            :placeholder="t('mobile.plugin.aiChatbox.inputPlaceholder')"
            :model-value="currentModelKey"
            :model-options="modelOptions"
            :show-model="hasProvider"
            @send="sendMessage"
            @stop="stopGeneration"
            @update:model-value="onModelChange"
          />
        </div>
      </template>

      <!-- 对话列表面板（左侧抽屉，DeepSeek 式滑入） -->
      <Teleport to="body">
        <Transition name="drawer">
          <div v-if="showConversationDrawer" class="fixed inset-0 z-50">
            <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="showConversationDrawer = false"></div>
            <!-- 抽屉面板贴边全高，内容须避开状态栏/导航栏：用宿主 JS 安全区值（inject safeArea）做内边距——
            Android WebView 不支持 CSS env(safe-area-inset-*)，CSS 变量类在真机无效；背景铺满屏幕边缘保持一体 -->
            <div class="drawer-panel absolute left-0 top-0 bottom-0 w-[82vw] max-w-[320px] flex flex-col overflow-hidden rounded-r-2xl bg-[var(--mobile-bg-card)] shadow-[var(--mobile-card-shadow)]" :style="drawerPanelStyle">
              <ConversationList
                :conversations="conversations"
                :current-id="currentConvId"
                :loading="loadingHistory"
                @select="onSelectConversation"
                @new="onNewConversation"
                @close="showConversationDrawer = false"
                @rename="onRenameConversation"
                @delete="onDeleteConversation"
              />
            </div>
          </div>
        </Transition>
      </Teleport>
      </div>
    </Transition>

    <!-- 插件级配置弹层（代码渲染 + 思考模式；即改即存写宿主 storage） -->
    <PluginSettingsSheet
      v-model="showSettingsSheet"
      :config="pluginConfig.config.value"
      :safe-area-bottom="safeAreaBottom"
      @change="pluginConfig.saveConfig"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * AI Chatbox 面板（移动端 navtab）— 消息流 + 输入区 + 对话列表抽屉 + 供应商配置
 */
import { ref, computed, watch, nextTick, onMounted, inject } from 'vue'
import type { Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ConversationList from './ConversationList.vue'
import ProviderConfigPage from './ProviderConfigPage.vue'
import PluginSettingsSheet from './PluginSettingsSheet.vue'
import { modelKey, useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'
import { usePluginConfig } from '../composables/usePluginConfig'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { ChatMessage as ChatMessageType, ConversationMeta } from '../types'

const { t } = useI18n()

// 宿主注入 PluginContext（PluginViewHost provide）
const context = inject<PluginContext>('pluginContext')!

const config = useAiConfig(context)
// 插件级全局配置（thinkingMode/reasoningEffort/showReasoning；移动宿主暂无配置页，
// 读取 storage key `config` 合并默认值，缺失时全走默认）
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
const showConversationDrawer = ref(false)
const showSettingsSheet = ref(false)
const dismissedError = ref('')

// 宿主注入的安全区 JS 值（App.vue useEdgeToEdge provide）：Android WebView 不支持
// CSS env(safe-area-inset-*)，CSS 变量类（mobile-header-safe 等）在真机拿到 0；
// dev-shell/桌面无 provide 时为 undefined → 回退 0（桌面无安全区）
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar: number }>>('safeArea')
const safeAreaTop = computed(() => safeArea?.value?.top || 0)
const safeAreaBottom = computed(() => safeArea?.value?.navigationBar || safeArea?.value?.bottom || 0)
const drawerPanelStyle = computed(() => ({
  paddingTop: `${safeAreaTop.value}px`,
  paddingBottom: `${safeAreaBottom.value}px`,
}))

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

// 配置加载完成前按 false 处理：避免用户已设 showReasoning=false 时，历史消息的
// 思考块在首帧用默认值闪现后再消失（storage 读取为异步，与消息加载并行）
const showReasoning = computed(
  () => !pluginConfig.loading.value && pluginConfig.config.value.showReasoning,
)

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
  showConversationDrawer.value = false
  await newConversation()
}

/** 切换对话：关闭抽屉后再加载（避免抽屉遮挡消息区滚动动画） */
async function onSelectConversation(id: string): Promise<void> {
  showConversationDrawer.value = false
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

// 自动滚动到底部（reasoning 流写入时正文可能仍为空，须一并跟踪才能跟上思考期增长）
watch(() => messages.value.length + (chat.streamingContent.value?.length || 0) + (chat.streamingReasoning.value?.length || 0), () => {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
    }
  })
})

onMounted(async () => {
  await loadConfig()
  // 插件配置与对话列表并行加载（缺失时 usePluginConfig 内部已回退默认值）
  await Promise.all([pluginConfig.loadConfig(), loadConversations()])
})
</script>

<style scoped>
/* 左侧抽屉过渡：平移 + 淡入（GPU 合成属性） */
.drawer-enter-active,
.drawer-leave-active {
  transition: opacity 0.2s ease;
}
.drawer-enter-active .drawer-panel,
.drawer-leave-active .drawer-panel {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}
.drawer-enter-from .drawer-panel,
.drawer-leave-to .drawer-panel {
  transform: translateX(-100%);
}
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
