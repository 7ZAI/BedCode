<template>
  <div class="h-full flex flex-col bg-white dark:bg-dark-900">
    <header class="px-4 py-2 flex items-center justify-between border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800">
      <div class="flex items-center gap-2">
        <select
          v-if="config.hasProvider.value"
          :value="config.activeProviderName.value"
          class="bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-xs text-slate-700 dark:text-white outline-none"
          @change="config.setActiveProvider(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="p in config.providers.value" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <span v-else class="text-xs text-slate-400">未配置模型</span>
      </div>
      <div class="flex items-center gap-1">
        <button
          class="p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors"
          title="模型配置"
          @click="config.showProviderManager.value = !config.showProviderManager.value"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
        <button
          :disabled="!config.hasProvider.value"
          class="p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors disabled:opacity-50"
          title="新对话"
          @click="chat.newConversation(config.activeProviderName.value)"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
        </button>
      </div>
    </header>

    <ProviderManager
      v-if="config.showProviderManager.value"
      :providers="config.providers.value"
      :active-provider-name="config.activeProviderName.value"
      @set-active="config.setActiveProvider"
      @remove="config.removeProvider"
      @add="config.addProvider"
    />

    <div v-if="!config.hasProvider.value" class="flex-1 flex flex-col items-center justify-center p-6 text-center">
      <div class="text-4xl mb-3">🤖</div>
      <p class="text-sm text-slate-500 dark:text-dark-400 mb-3">请先配置 AI 模型</p>
      <button
        class="px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 text-white rounded-lg transition-colors"
        @click="config.showProviderManager.value = true"
      >
        配置模型
      </button>
    </div>

    <template v-else>
      <div ref="messagesContainer" class="flex-1 overflow-y-auto p-4 space-y-3">
        <div v-if="chat.messages.value.length === 0" class="flex flex-col items-center justify-center h-full text-center">
          <div class="text-3xl mb-2">💬</div>
          <p class="text-sm text-slate-400 dark:text-dark-500">开始新对话</p>
        </div>
        <ChatMessage
          v-for="(msg, i) in chat.messages.value"
          :key="i"
          :message="msg"
          :streaming="chat.isStreaming.value && i === chat.messages.value.length - 1"
        />
      </div>
      <div class="border-t border-slate-200 dark:border-dark-700 p-3">
        <ChatInput
          :disabled="chat.sending.value || !config.activeProvider.value"
          placeholder="输入消息..."
          @send="chat.sendMessage"
        />
      </div>
    </template>

    <!-- 提示词优化弹窗 -->
    <PromptOptimizeDialog
      :show="optimizerState.showDialog.value"
      :optimizing="optimizerState.optimizing.value"
      :original="optimizerState.originalText.value"
      :optimized="optimizerState.optimizedText.value"
      :error="optimizerState.errorMessage.value"
      @accept="optimizerState.acceptOptimized()"
      @cancel="optimizerState.cancelOptimize()"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onMounted, inject } from 'vue'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ProviderManager from './ProviderManager.vue'
import PromptOptimizeDialog from './PromptOptimizeDialog.vue'
import { useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'
import { usePromptOptimizer } from '../composables/usePromptOptimizer'
import type { PluginContext } from '../../../plugin/types'

// 通过 provide/inject 获取 PluginContext（由 PluginViewHost 或 index.ts provide）
const context = inject<PluginContext>('pluginContext')!

const config = useAiConfig(context.storage.get, context.storage.set)
const chat = useAiChat(context)

// 提示词优化状态（复用同一 context）
const optimizer = usePromptOptimizer(context)
const optimizerState = {
  showDialog: optimizer.showDialog,
  optimizing: optimizer.optimizing,
  originalText: optimizer.originalText,
  optimizedText: optimizer.optimizedText,
  errorMessage: optimizer.errorMessage,
  acceptOptimized: optimizer.acceptOptimized,
  cancelOptimize: optimizer.cancelOptimize,
}

const messagesContainer = ref<HTMLElement | null>(null)

watch(() => chat.messages.value.length, () => {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
    }
  })
})

onMounted(async () => {
  await config.loadConfig()
  await chat.loadConversations()
})
</script>
