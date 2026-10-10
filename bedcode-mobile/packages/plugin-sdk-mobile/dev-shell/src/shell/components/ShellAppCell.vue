<template>
  <button
    type="button"
    class="flex flex-col items-center gap-1.5 py-2 rounded-[12px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
    :aria-label="app.name"
    @click="$emit('open', app.id)"
  >
    <span class="relative flex-shrink-0">
      <ShellAppIcon :app="app" :size="44" />
      <!-- 运行中指示：锚定图标右上角（随图标，不随名称宽度漂移） -->
      <span
        v-if="app.state === 'running'"
        class="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full"
        style="background: var(--mobile-success)"
        aria-hidden="true"
      />
    </span>
    <span class="w-full text-center text-[var(--font-size-xs)] text-[var(--mobile-text-primary)] truncate">
      {{ app.name }}
    </span>
  </button>
</template>

<script setup lang="ts">
/**
 * 应用宫格单元（首页「我的 WASM 应用」）
 */
import type { ShellApp } from '../types'
import ShellAppIcon from './ShellAppIcon.vue'

defineProps<{ app: ShellApp }>()
defineEmits<{ open: [appId: string] }>()
</script>
