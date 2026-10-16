<template>
  <div class="quick-action" @click="$emit('click')">
    <div
      class="aspect-square rounded-2xl flex flex-col items-center justify-center gap-2 p-4 transition-[border-color,box-shadow,transform] duration-300 active:scale-95 border border-[var(--mobile-border)] hover:border-[var(--mobile-border-hover)] hover:shadow-[var(--shell-action-glow)] bg-[var(--mobile-bg-card)]"
      :style="{ backgroundColor: `color-mix(in srgb, ${color} 6%, transparent)`, borderColor: `color-mix(in srgb, ${color} 20%, transparent)` }"
    >
      <span class="text-2xl">{{ icon }}</span>
      <span class="text-sm font-medium text-center text-[var(--mobile-text-secondary)]">{{ name }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * QuickActionButton — 快捷操作磁贴（图标 + 名称 + 主题色淡底）
 *
 * 迁移自旧公共组件 `src/components/QuickActionButton.vue`（壳内自足化）。
 * 与旧实现的一处差异：hover 光晕由硬编码 `rgba(34,211,238,0.08)` 改为
 * `--shell-action-glow`（styles/shell.css，由强调色派生），使其随主题与
 * 用户色板变化——硬编码青色在浅色主题下会与平台色板脱节。
 */
defineProps<{
  name: string
  content: string
  icon?: string
  color?: string
}>()

defineEmits<{
  click: []
}>()
</script>
