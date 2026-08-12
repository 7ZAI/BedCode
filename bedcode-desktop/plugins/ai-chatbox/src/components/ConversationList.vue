<template>
  <div class="flex flex-col h-full bg-[var(--bg-card)] border-r border-[var(--border)]">
    <!-- 折叠态：窄条只保留展开 + 新建按钮 -->
    <div v-if="collapsed" class="flex flex-col items-center gap-1 py-2">
      <button
        class="p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors"
        :title="t('desktop.plugin.aiChatbox.expandConversations')"
        @click="$emit('toggle-collapse')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
        </svg>
      </button>
      <button
        class="p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors"
        :title="t('desktop.plugin.aiChatbox.newConversation')"
        @click="$emit('new')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
      </button>
    </div>

    <!-- 展开态 -->
    <template v-else>
      <!-- 头部 -->
      <div class="flex items-center justify-between px-3 py-2.5 border-b border-[var(--border)]">
        <span class="text-sm font-medium text-[var(--text-secondary)]">
          {{ t('desktop.plugin.aiChatbox.conversations') }}
        </span>
        <div class="flex items-center gap-0.5">
          <button
            class="p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors"
            :title="t('desktop.plugin.aiChatbox.newConversation')"
            @click="$emit('new')"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
          </button>
          <button
            class="p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors"
            :title="t('desktop.plugin.aiChatbox.collapseConversations')"
            @click="$emit('toggle-collapse')"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 19l-7-7 7-7m8 14l-7-7 7-7" />
            </svg>
          </button>
        </div>
      </div>

      <!-- 列表 -->
      <div class="flex-1 overflow-y-auto py-1 flex flex-col">
        <div
          v-for="conv in conversations"
          :key="conv.id"
          class="group flex items-center gap-1 px-2 py-1.5 mx-1 rounded-btn cursor-pointer transition-colors"
          :class="conv.id === currentId
            ? 'bg-[var(--bg-hover)]'
            : 'hover:bg-[var(--bg-hover)]'"
          @click="$emit('select', conv.id)"
        >
          <!-- 标题（双击进入重命名模式） -->
          <template v-if="editingId === conv.id">
            <input
              :ref="el => { if (el) renameInputRef = el as HTMLInputElement }"
              v-model="renameDraft"
              class="flex-1 min-w-0 px-1.5 py-0.5 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-brand rounded focus:outline-none"
              @keydown.enter="commitRename(conv)"
              @keydown.esc="editingId = null"
              @click.stop
            />
          </template>
          <span
            v-else
            class="flex-1 min-w-0 truncate text-sm text-[var(--text-primary)]"
            :title="conv.title"
            @dblclick.stop="startRename(conv)"
          >{{ displayTitle(conv.title) }}</span>

          <!-- 操作（悬停显示） -->
          <span class="opacity-0 group-hover:opacity-100 transition-opacity flex items-center flex-shrink-0">
            <button
              class="p-0.5 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] rounded"
              :title="t('desktop.plugin.aiChatbox.rename')"
              @click.stop="startRename(conv)"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
              </svg>
            </button>
            <button
              class="p-0.5 text-[var(--text-tertiary)] hover:text-[var(--color-danger)] rounded"
              :title="t('desktop.plugin.aiChatbox.delete')"
              @click.stop="$emit('delete', conv)"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
            </button>
          </span>
        </div>

        <div v-if="!loading && conversations.length === 0" class="flex-1 flex flex-col items-center justify-center px-4 text-center">
          <div class="w-12 h-12 rounded-card bg-[var(--bg-hover)] flex items-center justify-center mb-3 text-[var(--text-tertiary)]">
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
              <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
            </svg>
          </div>
          <p class="text-xs text-[var(--text-tertiary)]">{{ t('desktop.plugin.aiChatbox.noConversations') }}</p>
          <p class="text-xs text-[var(--text-tertiary)]/70 mt-2">{{ t('desktop.plugin.aiChatbox.noConversationsHint') }}</p>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * ConversationList — 对话列表侧栏
 *
 * 列表 + 新建按钮；项支持双击/按钮重命名、删除；标题为新对话占位文案时显示默认文案。
 */
import { ref, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ConversationMeta } from '../types'

defineProps<{
  conversations: ConversationMeta[]
  currentId: string
  loading?: boolean
  /** 折叠态：只显示窄条操作列（展开/新建） */
  collapsed?: boolean
}>()

const emit = defineEmits<{
  select: [id: string]
  new: []
  rename: [conv: ConversationMeta, title: string]
  delete: [conv: ConversationMeta]
  /** 折叠/展开切换（状态由父级持有并持久化） */
  'toggle-collapse': []
}>()

const { t } = useI18n()

const editingId = ref<string | null>(null)
const renameDraft = ref('')
const renameInputRef = ref<HTMLInputElement | null>(null)

/** 新对话占位标题（i18n key）显示为默认文案 */
function displayTitle(title: string): string {
  if (!title || title === 'desktop.plugin.aiChatbox.newConversation') {
    return t('desktop.plugin.aiChatbox.newConversation')
  }
  return title
}

function startRename(conv: ConversationMeta): void {
  editingId.value = conv.id
  renameDraft.value = conv.title === 'desktop.plugin.aiChatbox.newConversation' ? '' : conv.title
  nextTick(() => {
    renameInputRef.value?.focus()
  })
}

function commitRename(conv: ConversationMeta): void {
  if (renameDraft.value.trim()) {
    emit('rename', conv, renameDraft.value.trim())
  }
  editingId.value = null
}
</script>
