<template>
  <nav class="bg-white dark:bg-dark-800 px-4 py-2 mobile-nav pb-safe">
    <div class="flex justify-around">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="flex flex-col items-center gap-1 px-4 py-2 rounded-lg transition-colors"
        :class="[
          currentPage === item.pageIndex
            ? 'text-primary-400'
            : 'text-gray-500 dark:text-dark-400 hover:text-gray-700 dark:text-dark-200'
        ]"
        @click="navigateTo(item)"
      >
        <component :is="item.icon" class="w-6 h-6" />
        <span class="text-xs">{{ item.label }}</span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { h, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()

// 路由名称到页面索引的映射
const pageRouteNames: Record<string, number> = {
  'mobile-devices': 0,
  'mobile-sessions': 1,
  'mobile-quick-actions': 2,
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
    label: '连接',
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
    label: '会话',
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
    label: '快捷',
    isSwipe: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M13 10V3L4 14h7v7l9-11h-7z'
        })
      ])
    }
  },
  {
    path: '/mobile',
    pageIndex: 3,
    label: '设置',
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
