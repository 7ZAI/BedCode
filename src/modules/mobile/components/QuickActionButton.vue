<template>
  <div class="quick-action" @click="$emit('click')">
    <div
      class="aspect-square rounded-2xl flex flex-col items-center justify-center gap-2 p-4 transition-all duration-300 active:scale-95 border border-[var(--mobile-border)] hover:border-[var(--mobile-border-hover)] hover:shadow-[0_0_15px_rgba(34,211,238,0.08)] bg-[var(--mobile-bg-card)]"
      :style="{ backgroundColor: color + '10', borderColor: color + '30' }"
    >
      <span class="text-2xl">{{ icon }}</span>
      <span class="text-sm font-medium text-center text-[var(--mobile-text-secondary)]">{{ name }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
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
