<template>
  <nav
    class="backdrop-blur-xl"
    style="background: var(--mobile-nav-bg); border-top: 1px solid var(--mobile-group-border)"
    :style="[navStyle, { borderTop: '1px solid var(--mobile-group-border)' }]"
  >
    <div class="flex justify-around relative">
      <button
        v-for="item in navItems"
        :key="item.pageIndex"
        class="flex flex-col items-center gap-0.5 px-4 pt-1.5 pb-1 rounded-xl transition-colors relative"
        :class="[
          currentPage === item.pageIndex
            ? 'text-[var(--mobile-nav-active)]'
            : ''
        ]"
        :style="currentPage !== item.pageIndex ? { color: 'var(--mobile-nav-inactive)' } : {}"
        @click="navigateTo(item)"
      >
        <!-- 激活态顶部指示条：与图标严格等宽同轴（left/right 锚定 + margin auto，不依赖 transform 精度） -->
        <span
          v-if="currentPage === item.pageIndex"
          class="absolute top-0 left-0 right-0 mx-auto w-[22px] h-[2px] rounded-full"
          style="background: var(--mobile-nav-active)"
        ></span>
        <span class="relative flex-shrink-0">
          <component :is="item.icon" class="w-[22px] h-[22px]" />
          <!-- 插件 tab 绿点：锚定图标右上角（随图标，不随 label 宽度漂移） -->
          <span
            v-if="item.isPlugin"
            class="absolute -top-0.5 -right-1 w-1.5 h-1.5 rounded-full"
            style="background: var(--mobile-chip-emerald)"
          ></span>
        </span>
        <span class="text-xs" :class="currentPage === item.pageIndex ? 'font-semibold' : 'font-medium'">{{ item.label }}</span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { h, computed, inject, type ComputedRef } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { Ref } from 'vue'
import { getPluginRegistry } from '@/plugin/registry'

// 注入安全区域
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar: number }>>('safeArea')

// 插件注册表
const pluginRegistry = getPluginRegistry()

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

// 导航项（内置 + 插件导航 Tab，统一按 order 排序决定页面索引）
// 内置插槽约定：连接=0、会话=100、工具箱=200、设置=300，插件用中间值插入
// 如 order=150 即位于「会话」右侧；页面索引 = 排序后位置（内置与插件共享序列）
const navItems = computed<NavItem[]>(() => {
  const builtin = [
    {
      path: '/mobile',
      name: 'mobile-devices',
      order: 0,
      label: computed(() => t('mobile.nav.connection')),
      isSwipe: true,
      icon: {
        render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M18 8V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h8'
          }),
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M10 19v-3.96 3.15'
          }),
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M7 19h5'
          }),
          h('rect', { width: 6, height: 10, x: 16, y: 12, rx: 2 })
        ])
      }
    },
    {
      path: '/mobile',
      name: 'mobile-sessions',
      order: 100,
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
      name: 'mobile-toolbox',
      order: 200,
      label: computed(() => t('mobile.nav.toolbox')),
      isSwipe: true,
      icon: {
        render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z'
          })
        ])
      }
    },
    {
      path: '/mobile',
      name: 'mobile-settings',
      order: 300,
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

  // 追加插件导航 Tab（order 参与全局排序，可与内置插槽交错）
  const pluginTabs = pluginRegistry.navTabs.value.map((tab) => ({
    path: '/mobile',
    name: `plugin-nav-${tab.pluginId}-${tab.id}`,
    order: tab.order,
    label: computed(() => tab.title),
    isSwipe: true,
    isPlugin: true,
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: tab.icon,
        })
      ])
    }
  }))

  return [...builtin, ...pluginTabs]
    .sort((a, b) => a.order - b.order)
    .map((item, idx) => ({ ...item, pageIndex: idx }))
})

/** 路由名称到页面索引的映射（随导航排序动态生成） */
const pageRouteNames = computed<Record<string, number>>(() => {
  const map: Record<string, number> = { 'mobile-home': 0 }
  for (const [idx, item] of navItems.value.entries()) {
    if (item.name) map[item.name] = idx
  }
  return map
})

// 当前页面索引
const currentPage = computed(() => {
  // 优先从查询参数获取页面索引
  const queryPage = route.query.page
  if (queryPage) {
    const page = parseInt(queryPage as string, 10)
    const maxPage = navItems.value.length - 1
    if (!isNaN(page) && page >= 0 && page <= maxPage) {
      return page
    }
  }

  // 其次从路由名称获取
  const name = route.name as string
  if (pageRouteNames.value[name] !== undefined) {
    return pageRouteNames.value[name]
  }
  return 0
})

/** 导航项类型 */
interface NavItem {
  path: string
  pageIndex: number
  /** 内置页路由名（插件 tab 为 plugin-nav-{pluginId}-{id}） */
  name?: string
  /** 全局排序值：内置插槽 0/100/200/300，插件用中间值插入 */
  order: number
  label: ComputedRef<string>
  isSwipe: boolean
  /** 插件 tab 标记（绿点指示） */
  isPlugin?: boolean
  icon: { render: () => ReturnType<typeof h> }
}

// 导航处理
function navigateTo(item: NavItem) {
  if (item.isSwipe) {
    // 跳转到滑动容器主页，并带上页面索引参数
    router.push({ name: 'mobile-home', query: { page: item.pageIndex.toString() } })
  } else {
    router.push(item.path)
  }
}
</script>
