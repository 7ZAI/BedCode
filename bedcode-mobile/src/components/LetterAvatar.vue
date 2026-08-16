<template>
  <div
    class="icon-tile text-white font-bold select-none"
    :class="size === 'lg' ? 'w-16 h-16 rounded-2xl text-2xl' : 'w-12 h-12 rounded-xl text-lg'"
    :style="{ background: gradient }"
  >
    {{ letter }}
  </div>
</template>

<script setup lang="ts">
/**
 * LetterAvatar - 渐变字母头像（PluginIcon 的回退实现）
 *
 * 按 seed（插件 id）哈希从预设渐变配色中选取，取插件名首字符
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    name: string
    /** 渐变配色哈希种子，通常为插件 id */
    seed: string
    size?: 'md' | 'lg'
  }>(),
  { size: 'md' }
)

/** 预设渐变配色（与移动端深色主题协调） */
const GRADIENTS = [
  'linear-gradient(135deg, #7c3aed, #4f46e5)',
  'linear-gradient(135deg, #0ea5e9, #6366f1)',
  'linear-gradient(135deg, #10b981, #0d9488)',
  'linear-gradient(135deg, #f59e0b, #ef4444)',
  'linear-gradient(135deg, #ec4899, #8b5cf6)',
  'linear-gradient(135deg, #06b6d4, #3b82f6)',
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
