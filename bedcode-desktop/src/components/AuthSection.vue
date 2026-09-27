<template>
  <div class="py-1.5">
    <!-- 分区标题 -->
    <p class="flex items-center gap-2 py-1">
      <span class="text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)]">
        {{ title }}
      </span>
      <span
        v-if="badge !== undefined"
        class="text-[calc(11px*var(--ui-scale))] font-medium px-1.5 py-0.5 rounded-md bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
      >
        {{ badge }}
      </span>
    </p>
    <!-- 空态 / 内容 -->
    <p v-if="!hasRecords" class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] py-1">
      {{ emptyText }}
    </p>
    <div v-else class="divide-y divide-[var(--border)]">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * AuthSection - 授权记录分区标题（详情页「授权记录」区块的四分区共用，spec §9.2）
 *
 * 只做「标题 + 可选计数徽标 + 空态 / 内容」的薄壳：分区内容（记录行 / 内置免询问项）
 * 由父组件以默认插槽传入，分区标题与空态文案由父组件按 i18n 传入——本组件不含任何
 * 业务判断，四个分区的结构差异全部收敛在父组件的插槽里。
 */
defineProps<{
  /** 分区标题（i18n 已翻译） */
  title: string
  /** 空态文案（i18n 已翻译） */
  emptyText: string
  /** 是否有内容（无内容时渲染空态而非空插槽） */
  hasRecords: boolean
  /** 可选计数徽标 */
  badge?: number
}>()
</script>
