<template>
  <span
    class="glyph-box"
    :class="['tone-' + tone, { 'cursor-pointer': clickable }]"
    :style="boxStyle"
    :aria-hidden="decorative ? 'true' : undefined"
  >
    <ShellIcon :name="icon" :size="iconSize" :stroke-width="1.7" />
  </span>
</template>

<script setup lang="ts">
/**
 * ShellAppTile — 应用图标色块（tone 派生底色 + 壳图标）
 *
 * 尺寸按 CSS 变量注入（组件自身不决定业务尺寸）：调用方给 width/height（px
 * 或 CSS 值），图标大小按 0.46 比例派生——主屏 52px 网格 / 卡片头 20px /
 * 列表行 30px / 详情头 34px 共用同一组件。
 */
import { computed } from 'vue'
import ShellIcon from './ShellIcon.vue'
import type { AppTone } from '../state'

const props = withDefaults(
  defineProps<{
    icon: string
    tone?: AppTone | 'system'
    size?: number
    /** 图标是否纯装饰（aria-hidden） */
    decorative?: boolean
    clickable?: boolean
  }>(),
  { tone: 'system', size: 52, decorative: true },
)

const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
}))
const iconSize = computed(() => Math.round(props.size * 0.46))
</script>

<style scoped>
.glyph-box {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border-radius: var(--radius-app-icon);
  transition:
    transform 0.18s,
    box-shadow 0.18s;
}
</style>
