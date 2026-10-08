<script setup lang="ts">
/**
 * EmptyState — 通用空态 / 加载态
 *
 * 图标底（currentColor 10% tint）+ 标题 + 说明 + 可选 CTA；flex:1 在内容区
 * 内垂直居中，列表态不参与撑高。tone 只切换图标底语义色（文字色固定）。
 */
defineProps<{
  /** 图标 path（heroicons outline，stroke-width 1.5） */
  icon: string
  title: string
  hint?: string
  /** 语义色：neutral（默认）/ warning / error */
  tone?: 'neutral' | 'warning' | 'error'
  actionLabel?: string
  actionIcon?: string
  /** CTA 样式：primary 实心 accent / neutral 灰底 */
  actionVariant?: 'primary' | 'neutral'
  /** 加载态：仅 spinner + 标题，不渲染图标与 CTA */
  loading?: boolean
}>()

defineEmits<{
  (e: 'action'): void
}>()
</script>

<template>
  <div class="fv2-empty">
    <!-- 加载态：细线圆环 + 文案 -->
    <template v-if="loading">
      <span class="fv2-spinner"></span>
      <p class="fv2-empty-hint">{{ title }}</p>
    </template>

    <template v-else>
      <div
        class="fv2-empty-icon"
        :class="{
          'fv2-empty-icon--warning': tone === 'warning',
          'fv2-empty-icon--error': tone === 'error',
        }"
      >
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" :d="icon" />
        </svg>
      </div>

      <p class="fv2-empty-title">{{ title }}</p>
      <p v-if="hint" class="fv2-empty-hint">{{ hint }}</p>

      <button
        v-if="actionLabel"
        class="fv2-empty-action"
        :class="actionVariant === 'primary' ? 'fv2-btn-primary' : 'fv2-btn-neutral'"
        @click="$emit('action')"
      >
        <svg
          v-if="actionIcon"
          class="w-4 h-4 flex-shrink-0"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="actionIcon" />
        </svg>
        {{ actionLabel }}
      </button>
    </template>
  </div>
</template>
