<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- 配置页模式 -->
    <ProviderConfigPage
      v-if="showConfigPage"
      :providers="providers"
      @back="showConfigPage = false"
      @add="addProvider"
      @update="updateProvider"
      @remove="removeProvider"
    />

    <!-- 聊天模式 -->
    <template v-else>
      <header class="px-4 py-2 flex items-center justify-between border-b border-[var(--border)] bg-[var(--bg-hover)]">
        <div class="flex items-center gap-2">
          <Select
            v-if="hasProvider"
            :model-value="activeProviderId"
            :options="providerOptions"
            size="sm"
            class="w-44 flex-shrink-0"
            @update:model-value="onProviderChange"
          />
          <span v-else class="text-xs text-[var(--text-tertiary)]">{{ t('desktop.plugin.aiChatbox.noProvider') }}</span>

          <!-- 模型选择 -->
          <Select
            v-if="hasProvider && currentModels.length > 1"
            :model-value="activeModel"
            :options="modelOptions"
            size="sm"
            class="w-44 flex-shrink-0"
            @update:model-value="onModelChange"
          />
        </div>
        <div class="flex items-center gap-1">
          <button
            class="p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors"
            :title="t('desktop.plugin.aiChatbox.modelConfig')"
            @click="showConfigPage = true"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </button>
          <button
            :disabled="!hasProvider"
            class="p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors disabled:opacity-50"
            :title="t('desktop.plugin.aiChatbox.newConversation')"
            @click="newConversation(activeProvider?.name || '')"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
          </button>
        </div>
      </header>

      <div v-if="!hasProvider" class="flex-1 flex flex-col items-center justify-center p-6 text-center">
        <div class="text-4xl mb-3">🤖</div>
        <p class="text-sm text-[var(--text-secondary)] mb-3">{{ t('desktop.plugin.aiChatbox.pleaseConfigure') }}</p>
        <button
          class="px-4 py-2 text-sm bg-brand hover:bg-brand-hover text-white rounded-btn transition-colors"
          @click="showConfigPage = true"
        >
          {{ t('desktop.plugin.aiChatbox.configureModel') }}
        </button>
      </div>

      <template v-else>
        <div ref="messagesContainer" class="flex-1 overflow-y-auto p-4 space-y-3">
          <div v-if="messages.length === 0" class="flex flex-col items-center justify-center h-full text-center">
            <div class="text-3xl mb-2">💬</div>
            <p class="text-sm text-[var(--text-tertiary)]">{{ t('desktop.plugin.aiChatbox.startNewChat') }}</p>
          </div>
          <ChatMessage
            v-for="(msg, i) in messages"
            :key="i"
            :message="msg"
            :streaming="isStreaming && i === messages.length - 1"
          />
        </div>
        <div class="border-t border-[var(--border)] p-3">
          <ChatInput
            :disabled="sending || !activeProvider"
            :placeholder="t('desktop.plugin.aiChatbox.inputPlaceholder')"
            @send="sendMessage"
          />
        </div>
      </template>

      <!-- 提示词优化弹窗 -->
      <PromptOptimizeDialog
        :show="optimizeShowDialog"
        :optimizing="optimizing"
        :original="optimizeOriginal"
        :optimized="optimizeOptimized"
        :error="optimizeError"
        @accept="acceptOptimized()"
        @cancel="cancelOptimize()"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * AI Chatbox 侧边栏面板
 */
import { ref, computed, watch, nextTick, onMounted, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ProviderConfigPage from './ProviderConfigPage.vue'
import PromptOptimizeDialog from './PromptOptimizeDialog.vue'
// 宿主共享下拉组件（替代原生 <select>，经 SDK 引用，样式随宿主主题 token）
import Select from '@bedcode/plugin-sdk-desktop/ui'
import { useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'
import { usePromptOptimizer } from '../composables/usePromptOptimizer'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

const { t } = useI18n()

// 通过 provide/inject 获取 PluginContext
const context = inject<PluginContext>('pluginContext')!

const {
  providers,
  activeProviderId,
  activeProvider,
  activeModel,
  hasProvider,
  loadConfig,
  setActiveProvider,
  setActiveModel,
  addProvider,
  updateProvider,
  removeProvider,
} = useAiConfig(context.storage.get, context.storage.set)

const {
  messages,
  sending,
  isStreaming,
  loadConversations,
  newConversation,
  sendMessage,
} = useAiChat(context)

const {
  showDialog: optimizeShowDialog,
  optimizing,
  originalText: optimizeOriginal,
  optimizedText: optimizeOptimized,
  errorMessage: optimizeError,
  acceptOptimized,
  cancelOptimize,
} = usePromptOptimizer(context)

const messagesContainer = ref<HTMLElement | null>(null)
const showConfigPage = ref(false)

/** 当前供应商的模型列表 */
const currentModels = computed(() => activeProvider.value?.models || [])

/** 供应商下拉选项（SDK Select 的 {value,label} 结构） */
const providerOptions = computed(() => providers.value.map(p => ({ value: p.id, label: p.name })))

/** 模型下拉选项（模型名既是值也是标签） */
const modelOptions = computed(() => currentModels.value.map(m => ({ value: m, label: m })))

// Select 的 update:model-value 载荷为 string|number，统一转 string 后走原有联动逻辑
function onProviderChange(value: string | number): void {
  setActiveProvider(String(value))
}

function onModelChange(value: string | number): void {
  setActiveModel(String(value))
}

watch(() => messages.value.length, () => {
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
