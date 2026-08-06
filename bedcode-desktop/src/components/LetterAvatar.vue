<template>
  <div
    class="icon-tile text-white font-bold select-none"
    :class="size === 'lg' ? 'w-14 h-14 rounded-2xl text-2xl' : 'w-10 h-10 rounded-xl text-base'"
    :style="{ background: gradient }"
  >
    {{ letter }}
  </div>
</template>

<script setup lang="ts">
/**
 * LetterAvatar - 渐变字母头像（PluginIcon 的回退实现）
 *
 * 按 seed（插件 id）FNV-1a 哈希从预设渐变配色中选取，取插件名首字符。
 * 桌面端版本：渐变配色使用 --color-primary 系，与 Workbench 风格协调。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 显示名称（取首字符） */
    name: string
    /** 渐变配色哈希种子，通常为插件 id */
    seed: string
    /** 图标尺寸 */
    size?: 'md' | 'lg'
  }>(),
  { size: 'md' }
)

/** 预设渐变配色（桌面端 Workbench 风格，与 --color-primary 系协调） */
const GRADIENTS = [
  'linear-gradient(135deg, #6366f1, #4f46e5)',
  'linear-gradient(135deg, #3b82f6, #6366f1)',
  'linear-gradient(135deg, #10b981, #0d9488)',
  'linear-gradient(135deg, #f59e0b, #d97706)',
  'linear-gradient(135deg, #8b5cf6, #6d28d9)',
  'linear-gradient(135deg, #0ea5e9, #3b82f6)',
]

const letter = computed(() => (props.name.trim().charAt(0) || '?').toUpperCase())

/** FNV-1a 风格哈希：稳定且零依赖 */
const gradient = computed(() => {
  let hash = 2166136261
  for (let i = 0; i < props.seed.length; i++) {
    hash ^= props.seed.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return GRADIENTS[(hash >>> 0) % GRADIENTS.length]
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
