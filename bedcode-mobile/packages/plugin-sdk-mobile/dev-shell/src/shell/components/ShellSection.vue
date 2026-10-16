<template>
  <div class="flex items-center justify-between gap-2 min-w-0 px-4 pt-4 pb-2">
    <h2 class="text-[var(--font-size-sm)] font-semibold text-[var(--mobile-text-secondary)] truncate">
      {{ title }}
    </h2>
    <div class="flex items-center gap-2 flex-shrink-0 min-w-0">
      <span v-if="note" class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
        {{ note }}
      </span>
      <slot name="action" />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 分组标题行
 *
 * 标题 + 可选说明 + 右侧动作插槽。壳内所有「分组头」都走这一个组件，
 * 保证标题层级与右侧动作的对齐方式全平台一致。
 */
defineProps<{
  title: string
  /** 右侧说明（如「3 项」「由应用提供」） */
  note?: string
}>()
</script>
