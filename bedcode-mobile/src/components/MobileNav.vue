<template>
  <nav
    class="bg-[var(--mobile-nav-bg)] backdrop-blur-xl border-t border-[var(--mobile-nav-border)]"
    :style="navStyle"
  >
    <!-- 顶部发光效果 -->
    <div class="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-[var(--mobile-accent)]/30 to-transparent"></div>

    <div class="flex justify-around relative">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="flex flex-col items-center gap-0.5 px-4 pt-1.5 pb-1 rounded-xl transition-all duration-300 relative"
        :class="[
          currentPage === item.pageIndex
            ? 'text-[var(--mobile-nav-active)]'
            : 'text-[var(--mobile-nav-inactive)] hover:text-[var(--mobile-text-secondary)]'
        ]"
        @click="navigateTo(item)"
      >
        <!-- 活跃指示器 -->
        <div
          v-if="currentPage === item.pageIndex"
          class="absolute -top-1 left-1/2 -translate-x-1/2 w-8 h-1 bg-[var(--mobile-nav-active)] rounded-full shadow-[0_0_8px_rgba(34,211,238,0.5)]"
        ></div>
        <component :is="item.icon" class="w-6 h-6 transition-transform duration-200" :class="currentPage === item.pageIndex ? 'scale-110' : ''" />
        <span class="text-xs font-medium">{{ item.label }}</span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { h, computed, inject } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { Ref } from 'vue'

// 注入安全区域
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar: number }>>('safeArea')

// 导航栏样式：底部安全区域
// Android WebView 不支持 CSS env(safe-area-inset-*)，完全依赖 JS 值
// 父组件 MobileLayout 会在 safeAreaReady 后才渲染
const navStyle = computed(() => {
  const jsBottom = safeArea?.value?.navigationBar || safeArea?.value?.bottom || 0
  return {
    paddingBottom: `${jsBottom}px`,
  }
})

const route = useRoute()
const router = useRouter()
const { t } = useI18n()

// 路由名称到页面索引的映射
const pageRouteNames: Record<string, number> = {
  'mobile-devices': 0,
  'mobile-sessions': 1,
  'mobile-toolbox': 2,
  'mobile-settings': 3,
  'mobile-home': 0 // 默认首页
}

// 当前页面索引
const currentPage = computed(() => {
  // 优先从查询参数获取页面索引
  const queryPage = route.query.page
  if (queryPage) {
    const page = parseInt(queryPage as string, 10)
    const maxPage = navItems.length - 1
    if (!isNaN(page) && page >= 0 && page <= maxPage) {
      return page
    }
  }

  // 其次从路由名称获取
  const name = route.name as string
  if (pageRouteNames[name] !== undefined) {
    return pageRouteNames[name]
  }
  return 0
})

const navItems = [
  {
    path: '/mobile',
    pageIndex: 0,
    label: computed(() => t('mobile.nav.connection')),
    isSwipe: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z'
        })
      ])
    }
  },
  {
    path: '/mobile',
    pageIndex: 1,
    label: computed(() => t('mobile.nav.sessions')),
    isSwipe: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z'
        })
      ])
    }
  },
  {
    path: '/mobile',
    pageIndex: 2,
    label: computed(() => t('mobile.nav.toolbox')),
    isSwipe: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z'
        })
      ])
    }
  },
  {
    path: '/mobile',
    pageIndex: 3,
    label: computed(() => t('mobile.nav.settings')),
    isSwipe: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z'
        }),
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z'
        })
      ])
    }
  }
]

// 导航处理
function navigateTo(item: typeof navItems[0]) {
  if (item.isSwipe) {
    // 跳转到滑动容器主页，并带上页面索引参数
    router.push({ name: 'mobile-home', query: { page: item.pageIndex.toString() } })
  } else {
    router.push(item.path)
  }
}
</script>
