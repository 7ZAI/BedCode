<template>
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
    <path d="M4.5 1.5h4.586a1 1 0 01.707.293l2.414 2.414a1 1 0 01.293.707V13.5a1 1 0 01-1 1h-7a1 1 0 01-1-1v-11a1 1 0 011-1z" :stroke="color" stroke-width="1.2" />
    <path d="M9 1.5v3.5h3.5" :stroke="color" stroke-width="1.2" />
  </svg>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ color?: string }>(), {
  color: 'var(--mobile-text-muted)',
})
</script>
