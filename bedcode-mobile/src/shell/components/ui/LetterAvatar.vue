<template>
  <div
    class="icon-tile text-white font-bold select-none"
    :class="[
      size === 'lg' ? 'w-16 h-16 rounded-2xl text-2xl' : 'w-12 h-12 rounded-xl text-lg',
      gradientClass,
    ]"
  >
    {{ letter }}
  </div>
</template>

<script setup lang="ts">
/**
 * LetterAvatar — 渐变字母头像（无图标应用的兜底）
 *
 * 迁移自旧公共组件 `src/components/LetterAvatar.vue`（壳内自足化）。
 * 与旧实现的一处差异：六组渐变色从组件内硬编码 hex 移入 `styles/shell.css`
 * 的 `.shell-avatar-g*` 档位类——按 frontend-styles「token-bound」纪律，
 * 颜色落点只能是样式表；行为（seed 哈希选档 + 首字符）逐字不变。
 *
 * 文字用 `text-white` 是有意为之（非品牌/强调色令牌面）：渐变底固定不随
 * 主题反转，因此不存在令牌反色后对比度崩塌的问题。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    name: string
    /** 渐变配色哈希种子，通常为应用/插件 id */
    seed: string
    size?: 'md' | 'lg'
  }>(),
  { size: 'md' }
)

/** 渐变档位数（与 shell.css 的 .shell-avatar-g* 定义数一致） */
const GRADIENT_COUNT = 6

const letter = computed(() => (props.name.trim().charAt(0) || '?').toUpperCase())

/** FNV-1a 风格哈希：稳定且零依赖，同一 seed 永远落同一档 */
const gradientClass = computed(() => {
  let hash = 2166136261
  for (let i = 0; i < props.seed.length; i++) {
    hash ^= props.seed.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return `shell-avatar-g${(hash >>> 0) % GRADIENT_COUNT}`
})
</script>

<style scoped>
.icon-tile {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
</style>
