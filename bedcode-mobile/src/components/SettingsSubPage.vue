<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header：顶部内边距叠加状态栏安全区。
         这些二级页是 standalone 路由（不经壳的 ShellView，拿不到壳的容器内边距），
         不补安全区就会把标题栏顶到系统状态栏上（2026-10-14 用户反馈）。
         间距与权限总览页头取齐：页头自身留 10px。 -->
    <header
      class="flex-shrink-0 bg-[color:color-mix(in_srgb,var(--mobile-bg-secondary)_90%,transparent)] backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 flex items-center gap-3"
      :style="{ paddingTop: `calc(10px + ${topInset}px)` }"
    >
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

    <!-- Content（页面内容滚动容器：scrollbar-gutter-stable 防滚动条显隐引起整页横向抖动；
         底部内边距叠加导航栏安全区，否则最后一行会压在系统手势条下） -->
    <div
      class="scrollbar-gutter-stable flex-1 overflow-y-auto overflow-x-hidden"
      :style="{ paddingBottom: `${bottomInset}px` }"
    >
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SettingsSubPage - 设置二级页面脚手架
 *
 * 提供带返回按钮的头部与可滚动内容区，
 * 各设置分类页（外观/出站/关于）与插件全屏路由页复用此布局。
 *
 * 安全区由 App.vue provide 注入（Android WebView 不支持 env(safe-area-inset-*)，
 * 只能消费宿主注入的 JS 值）；缺省 0 = 桌面预览，不影响布局。
 */
import { computed, inject, type Ref } from 'vue'
import { useRouter } from 'vue-router'

defineProps<{
  /** 页面标题（通常为分类名） */
  title: string
}>()

const router = useRouter()

const safeArea = inject<Ref<{ top: number; bottom: number }> | undefined>('safeArea', undefined)
const topInset = computed(() => safeArea?.value.top ?? 0)
const bottomInset = computed(() => safeArea?.value.bottom ?? 0)
</script>
