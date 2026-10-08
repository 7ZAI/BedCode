<template>
  <span
    class="inline-flex items-center justify-center flex-shrink-0 overflow-hidden border"
    :class="toneClass"
    :style="boxStyle"
    role="img"
    :aria-label="app.name"
  >
    <!-- SVG path d（应用自带图标，24×24 视框 stroke 风格） -->
    <svg
      v-if="kind === 'path'"
      :width="innerSize"
      :height="innerSize"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path :d="app.icon" />
    </svg>
    <!-- emoji 图标 -->
    <span
      v-else-if="kind === 'emoji'"
      :style="{ fontSize: `${innerSize}px`, lineHeight: '1' }"
      aria-hidden="true"
      >{{ app.icon }}</span
    >
    <!-- 无图标：首字母回退（中文名取首字比取不到拉丁字母更可读） -->
    <span
      v-else
      :style="{ fontSize: `${Math.round(size * 0.42)}px`, fontWeight: 600 }"
      aria-hidden="true"
      >{{ initial }}</span
    >
  </span>
</template>

<script setup lang="ts">
/**
 * 应用图标
 *
 * 三种形态归一：emoji / SVG path d / 首字母回退。判据集中在 utils.iconKindOf，
 * 避免各屏各写一份「这是不是 path」的判断。
 */
import { computed } from 'vue'
import type { ShellApp } from '../types'
import { iconKindOf, initialOf } from '../utils'

const props = withDefaults(
  defineProps<{
    app: ShellApp
    /** 图标盒边长（px） */
    size?: number
    /** 色调：accent（强调底）/ plain（中性底）/ ghost（幽灵框，用于「更多」占位） */
    tone?: 'accent' | 'plain' | 'ghost'
  }>(),
  { size: 44, tone: 'accent' },
)

const kind = computed(() => iconKindOf(props.app.icon))
const initial = computed(() => initialOf(props.app.name))
/** 图形占盒子的比例：emoji 顶格显小，path 留白更协调 */
const innerSize = computed(() => Math.round(props.size * (kind.value === 'emoji' ? 0.62 : 0.55)))

const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  borderRadius: props.size >= 48 ? '16px' : '12px',
}))

const toneClass = computed(() => {
  switch (props.tone) {
    case 'plain':
      return 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)] border-[var(--mobile-border)]'
    case 'ghost':
      return 'bg-transparent text-[var(--mobile-text-secondary)] border-dashed border-[var(--mobile-border-hover)]'
    default:
      // emoji 自带颜色，不再叠加前景色，否则会出现双色描边
      return kind.value === 'emoji'
        ? 'bg-[var(--mobile-bg-elevated)] border-[var(--mobile-border)]'
        : 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] border-[var(--mobile-border)]'
  }
})
</script>
