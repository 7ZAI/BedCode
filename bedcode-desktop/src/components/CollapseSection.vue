<template>
  <div class="collapse-section">
    <button
      class="w-full flex items-center gap-2 py-2.5 px-1 text-left transition-colors hover:bg-[var(--bg-hover)] rounded-md"
      @click="open = !open"
    >
      <!-- 展开/折叠箭头 -->
      <svg
        class="w-3.5 h-3.5 flex-shrink-0 text-[var(--text-tertiary)] transition-transform duration-200"
        :class="{ 'rotate-90': open }"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
      </svg>

      <!-- Emoji 图标（来自 CONTRIBUTION_KINDS 表） -->
      <span v-if="emoji" class="text-sm flex-shrink-0">{{ emoji }}</span>

      <!-- 标题 -->
      <span class="flex-1 text-[calc(12px*var(--ui-scale))] font-semibold uppercase tracking-[0.06em] text-[var(--text-secondary)]">
        {{ title }}
      </span>

      <!-- 数量徽章 -->
      <span
        v-if="badge !== undefined"
        class="text-[calc(11px*var(--ui-scale))] font-medium px-1.5 py-0.5 rounded-md bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
      >
        {{ badge }}
      </span>

      <!-- 右侧操作插槽（如"前往配置"按钮） -->
      <slot name="action" />
    </button>

    <!-- 折叠内容 -->
    <Transition name="collapse">
      <div v-if="open" class="collapse-body">
        <slot />
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
/**
 * CollapseSection - 通用折叠区（Workbench 风格）
 *
 * 用于插件列表和详情页的折叠区域展示。
 * 标题 + 可选 emoji + 可选 badge + 可选右侧操作按钮。
 */
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 区域标题 */
    title: string
    /** 可选 emoji 图标 */
    emoji?: string
    /** 可选数量徽章 */
    badge?: number
    /** 默认是否展开 */
    defaultOpen?: boolean
  }>(),
  { emoji: '', defaultOpen: true }
)

const open = ref(props.defaultOpen)
</script>

<style scoped>
.collapse-section {
  border-bottom: 1px solid var(--border);
}

.collapse-section:last-child {
  border-bottom: none;
}

.collapse-body {
  overflow: hidden;
}

.collapse-enter-active,
.collapse-leave-active {
  transition: opacity 0.15s ease, max-height 0.2s ease;
  max-height: 2000px;
}

.collapse-enter-from,
.collapse-leave-to {
  opacity: 0;
  max-height: 0;
}
</style>
