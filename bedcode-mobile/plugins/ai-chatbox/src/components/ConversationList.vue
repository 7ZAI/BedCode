<template>
  <div class="flex flex-col h-full bg-[var(--mobile-bg-card)]">
    <!-- 头部：标题 + 关闭（左侧抽屉） -->
    <div class="flex items-center justify-between px-3 pt-2 pb-1 border-b border-[var(--mobile-border)]">
      <span class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] pl-1">
        {{ t('mobile.plugin.aiChatbox.conversations') }}
      </span>
      <button
        class="w-11 h-11 -mr-2 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-lg transition-opacity"
        :title="t('mobile.plugin.aiChatbox.close')"
        @click="$emit('close')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- 新对话（DeepSeek 式醒目入口） -->
    <div class="px-3 pt-2.5 pb-1.5">
      <button
        class="w-full h-11 flex items-center justify-center gap-1.5 text-[var(--font-size-base)] rounded-xl bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 transition-opacity"
        @click="$emit('new')"
      >
        <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
        </svg>
        {{ t('mobile.plugin.aiChatbox.newConversation') }}
      </button>
    </div>

    <!-- 列表 -->
    <div class="flex-1 overflow-y-auto pt-1 pb-2 flex flex-col">
      <div
        v-for="conv in conversations"
        :key="conv.id"
        class="flex items-center gap-1 px-2.5 py-2 min-h-[52px] rounded-xl mx-2 cursor-pointer transition-colors"
        :class="conv.id === currentId
          ? 'bg-[var(--mobile-accent-muted)]'
          : 'active:bg-[var(--mobile-bg-tertiary)]'"
        @click="$emit('select', conv.id)"
      >
        <!-- 标题 + 相对时间（重命名模式时隐藏） -->
        <template v-if="editingId !== conv.id">
          <div class="flex-1 min-w-0">
            <span class="block truncate text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ displayTitle(conv.title) }}
            </span>
            <span class="block mt-0.5 text-xs text-[var(--mobile-text-muted)]">
              {{ relativeTime(conv.updatedAt) }}
            </span>
          </div>
        </template>
        <!-- 重命名输入框 -->
        <template v-else>
          <input
            :ref="el => { if (el) renameInputRef = el as HTMLInputElement }"
            v-model="renameDraft"
            class="flex-1 min-w-0 px-2.5 h-10 text-[var(--font-size-base)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-focus)] rounded-lg focus:outline-none"
            @keydown.enter="commitRename(conv)"
            @keydown.esc="editingId = null"
            @click.stop
          />
        </template>

        <!-- 操作（常显：移动端无 hover） -->
        <span class="flex items-center flex-shrink-0">
          <button
            class="w-10 h-10 flex items-center justify-center text-[var(--mobile-text-muted)] active:opacity-80 rounded-lg"
            :title="t('mobile.plugin.aiChatbox.rename')"
            @click.stop="startRename(conv)"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </button>
          <button
            class="w-10 h-10 flex items-center justify-center text-[var(--mobile-text-muted)] active:opacity-80 rounded-lg"
            :title="t('mobile.plugin.aiChatbox.delete')"
            @click.stop="$emit('delete', conv)"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </span>
      </div>

      <div v-if="!loading && conversations.length === 0" class="flex-1 flex flex-col items-center justify-center px-4 text-center">
        <div class="w-12 h-12 rounded-xl bg-[var(--mobile-bg-tertiary)] flex items-center justify-center mb-3 text-[var(--mobile-text-muted)]">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5">
            <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
          </svg>
        </div>
        <p class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.plugin.aiChatbox.noConversations') }}</p>
        <p class="text-xs text-[var(--mobile-text-muted)]/70 mt-2">{{ t('mobile.plugin.aiChatbox.noConversationsHint') }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ConversationList — 对话列表面板（移动端左侧抽屉内使用，DeepSeek 式）
 *
 * 标题 + 关闭 + 醒目新对话按钮 + 列表；项支持重命名、删除，
 * 副行显示相对时间（updatedAt）；标题为新对话占位文案时显示默认文案。
 */
import { ref, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ConversationMeta } from '../types'

defineProps<{
  conversations: ConversationMeta[]
  currentId: string
  loading?: boolean
}>()

const emit = defineEmits<{
  select: [id: string]
  new: []
  close: []
  rename: [conv: ConversationMeta, title: string]
  delete: [conv: ConversationMeta]
}>()

const { t, locale } = useI18n()

const editingId = ref<string | null>(null)
const renameDraft = ref('')
const renameInputRef = ref<HTMLInputElement | null>(null)

/** 新对话占位标题（i18n key）显示为默认文案 */
function displayTitle(title: string): string {
  if (!title || title === 'mobile.plugin.aiChatbox.newConversation') {
    return t('mobile.plugin.aiChatbox.newConversation')
  }
  return title
}

/** 相对时间：刚刚 / n 分钟前 / n 小时前 / n 天前 / 超 30 天显示本地日期 */
function relativeTime(iso: string): string {
  if (!iso) return ''
  const ts = new Date(iso).getTime()
  if (Number.isNaN(ts)) return ''
  const diffMin = Math.floor((Date.now() - ts) / 60000)
  if (diffMin < 1) return t('mobile.plugin.aiChatbox.timeJustNow')
  if (diffMin < 60) return t('mobile.plugin.aiChatbox.timeMinutesAgo', { n: diffMin })
  const hours = Math.floor(diffMin / 60)
  if (hours < 24) return t('mobile.plugin.aiChatbox.timeHoursAgo', { n: hours })
  const days = Math.floor(hours / 24)
  if (days < 30) return t('mobile.plugin.aiChatbox.timeDaysAgo', { n: days })
  return new Date(ts).toLocaleDateString(locale.value === 'zh-CN' ? 'zh-CN' : 'en-US')
}

function startRename(conv: ConversationMeta): void {
  editingId.value = conv.id
  renameDraft.value = conv.title === 'mobile.plugin.aiChatbox.newConversation' ? '' : conv.title
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
