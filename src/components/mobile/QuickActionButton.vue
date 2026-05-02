<template>
  <div class="quick-action" @click="$emit('click')">
    <div
      class="aspect-square rounded-2xl flex flex-col items-center justify-center gap-2 p-4 transition-transform active:scale-95"
      :style="{ backgroundColor: color + '20' }"
    >
      <span class="text-2xl">{{ icon }}</span>
      <span class="text-sm font-medium text-center">{{ name }}</span>
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
