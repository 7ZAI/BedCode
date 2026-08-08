<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="flex-shrink-0 bg-[color:color-mix(in_srgb,var(--mobile-bg-secondary)_90%,transparent)] backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center gap-3">
      <button
        class="flex-shrink-0 p-1 -ml-1 text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] active:opacity-80 transition-colors"
        @click="router.back()"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h1 class="flex-1 min-w-0 text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide truncate">{{ title }}</h1>
    </header>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SettingsSubPage - 设置二级页面脚手架
 *
 * 提供带返回按钮的头部与可滚动内容区，
 * 各设置分类页（连接/通知/外观/关于）复用此布局。
 */
import { useRouter } from 'vue-router'

defineProps<{
  /** 页面标题（通常为分类名） */
  title: string
}>()

const router = useRouter()
</script>
