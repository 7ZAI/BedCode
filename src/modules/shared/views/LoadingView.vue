<template>
  <div class="h-screen flex items-center justify-center bg-gray-50 dark:bg-dark-900">
    <div class="flex flex-col items-center gap-4">
      <!-- Logo or spinner -->
      <div class="w-12 h-12 rounded-full bg-primary-400 animate-pulse" />
      <span class="text-gray-600 dark:text-dark-300 text-sm">加载中...</span>
    </div>
  </div>
</template>

<script setup lang="ts">
// 根路由过渡页面
// 路由守卫会立即重定向到正确的页面，此页面仅作为fallback
</script>