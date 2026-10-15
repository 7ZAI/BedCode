<template>
  <component
    :is="clickable ? 'button' : 'span'"
    :type="clickable ? 'button' : undefined"
    class="inline-flex items-center gap-1.5 rounded-[999px] border px-2.5 shell-type-micro transition-colors duration-200"
    :class="[
      toneClass,
      // 可点击时撑满 44px 触控下限：胶囊的视觉高度由 py 控制，但点击区必须够。
      // 早前固定 min-h-32px，被用在「停止实例 / 批准 / 待审批处理」这类需要用户
      // 动手的按钮上，低于平台 44px 门槛。
      clickable
        ? 'min-h-[var(--mobile-touch-target-min)] hover:bg-[var(--mobile-accent-muted)] active:opacity-80'
        : 'py-1',
    ]"
    @click="clickable && $emit('click', $event)"
  >
    <slot name="icon" />
    <span class="truncate">{{ label }}</span>
  </component>
</template>

<script setup lang="ts">
/**
 * 状态胶囊
 *
 * 可点击与不可点击共用一套视觉（点击态额外补 hover/active 反馈，
 * 保证触屏上有按压反馈——hover-only 在移动端等于没有反馈）。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    label: string
    /** neutral 中性 / accent 强调 / success / warn / danger */
    tone?: 'neutral' | 'accent' | 'success' | 'warn' | 'danger'
    clickable?: boolean
  }>(),
  { tone: 'neutral', clickable: false },
)

defineEmits<{ click: [event: MouseEvent] }>()

const toneClass = computed(() => {
  switch (props.tone) {
    case 'accent':
      return 'bg-[var(--mobile-accent-muted)] border-[var(--mobile-border-active)] text-[var(--mobile-accent)]'
    case 'success':
      return 'bg-[var(--mobile-success-muted)] border-transparent text-[var(--mobile-success)]'
    case 'warn':
      return 'bg-[var(--mobile-warning-muted)] border-transparent text-[var(--mobile-warning)]'
    case 'danger':
      return 'bg-[var(--mobile-error-muted)] border-transparent text-[var(--mobile-error)]'
    default:
      return 'bg-[var(--mobile-bg-elevated)] border-[var(--mobile-border)] text-[var(--mobile-text-secondary)]'
  }
})
</script>
