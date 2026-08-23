<template>
  <div class="att-root mobile-ui">
    <!-- 自绘页签：任务记录 / 定时任务（双面板常驻挂载，切页不丢订阅与滚动位置） -->
    <div class="att-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="att-tab"
        :class="{ 'att-tab-active': activeTab === tab.key }"
        @click="activeTab = tab.key"
      >
        {{ t(tab.label) }}
      </button>
    </div>
    <!-- 页签内容区：data-swipe-zone 声明内部横滑区，区内左右滑切换页签；
         边界状态经 data-zone-at-* 同步给宿主 MobileSwipeContainer——
         已在边界页时继续同向滑动交外层翻主页面 -->
    <div
      ref="zoneRoot"
      class="att-tab-panels"
      data-swipe-zone
      :data-zone-at-start="activeTabIndex === 0"
      :data-zone-at-end="activeTabIndex === tabs.length - 1"
      @touchstart.passive="onZoneTouchStart"
      @touchmove.passive="onZoneTouchMove"
      @touchend="onZoneTouchEnd"
      @touchcancel="onZoneTouchEnd"
    >
      <!-- 双面板常驻挂载（保订阅与滚动位置），切页走透明度+位移过渡；
           面板停靠类由 activeTabIndex 驱动：新页自动从滑动方向侧滑入 -->
      <div :class="['att-tab-panel', panelClass(0)]">
        <TaskHistoryTab :context="context" :history="history" />
      </div>
      <div :class="['att-tab-panel', panelClass(1)]">
        <ScheduledJobsTab :context="context" :scheduled="scheduled" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * AutoTaskToolboxView — 自动任务工具箱页（2 页签容器）
 *
 * 宿主 ToolboxView 经 PluginViewHost provide pluginContext 后渲染本组件。
 * 业务逻辑全部下沉到 composables（useTaskHistory / useScheduledJobs），
 * 本组件只做：页签切换 + 两页联动（事件去抖重拉 + 断线重连重拉）。
 */
import { inject, ref, computed, watch, onMounted, onUnmounted } from 'vue'
import type { PluginContext, MobileHostApi } from '@binblink/plugin-sdk-mobile'
import { getMobileApi } from '@binblink/plugin-sdk-mobile'
import { useSwipeTabs } from '@binblink/plugin-sdk-mobile/ui/swipe-tabs'
import TaskHistoryTab from './TaskHistoryTab.vue'
import ScheduledJobsTab from './ScheduledJobsTab.vue'
import { useTaskHistory } from '../composables/useTaskHistory'
import { useScheduledJobs } from '../composables/useScheduledJobs'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string): string => context.i18n.t(key)
const mobileApi = getMobileApi() as MobileHostApi

const activeTab = ref<'history' | 'scheduled'>('history')
const tabs: { key: 'history' | 'scheduled'; label: string }[] = [
  { key: 'history', label: 'toolbox.tabs.history' },
  { key: 'scheduled', label: 'toolbox.tabs.scheduled' },
]

/** 当前页签序号：横滑区边界声明（data-zone-at-*）与步进切换共用 */
const activeTabIndex = computed(() => tabs.findIndex(tab => tab.key === activeTab.value))

/** 面板停靠态：当前页居中；已过页停左侧、未到页停右侧——切页时新页从停靠侧滑入 */
function panelClass(index: number): string {
  if (index === activeTabIndex.value) return 'att-tab-panel--active'
  return index < activeTabIndex.value ? 'att-tab-panel--left' : 'att-tab-panel--right'
}

// ==================== 页签内容区横滑切换 ====================
//
// 区内水平主导滑动切换页签；已在边界页时继续同向滑动由宿主
// MobileSwipeContainer 接管翻主页面（外层按 data-swipe-zone /
// data-zone-at-* 仲裁，本组件无需感知）。
// 手势跳过：输入类控件内的触摸（定时任务表单，防文本选择拖动误触切页）、
// 可横向滚动容器（任务记录状态筛选 chips 行，横滑语义是滚动自身）。

const zoneRoot = ref<HTMLElement | null>(null)

/** 触摸目标到横滑区根之间是否存在可横向滚动容器 */
function insideHorizontallyScrollable(target: EventTarget | null): boolean {
  let el = (target as HTMLElement | null)?.parentElement ?? null
  const root = zoneRoot.value
  while (el && el !== root) {
    if (el.scrollWidth > el.clientWidth + 1) {
      const overflowX = getComputedStyle(el).overflowX
      if (overflowX === 'auto' || overflowX === 'scroll') return true
    }
    el = el.parentElement
  }
  return false
}

const {
  onTouchStart: onZoneTouchStart,
  onTouchMove: onZoneTouchMove,
  onTouchEnd: onZoneTouchEnd,
} = useSwipeTabs(
  (dir) => {
    // 步进切换；越界方向忽略（该手势已由外层容器接管翻主页面）
    const next = activeTabIndex.value + (dir === 'left' ? 1 : -1)
    if (next >= 0 && next < tabs.length) activeTab.value = tabs[next].key
  },
  {
    shouldSkip: (target) => {
      const el = target as HTMLElement | null
      return (
        !!el?.closest?.('input, textarea, [contenteditable="true"]') ||
        insideHorizontallyScrollable(target)
      )
    },
  },
)

const history = useTaskHistory(context)
const scheduled = useScheduledJobs(context)

// 三路 WS 事件去抖触达时，除历史页外联动重拉定时任务页
history.onDebouncedReload(() => {
  void scheduled.load()
})

let stopConnectionWatch: (() => void) | null = null

onMounted(() => {
  history.start()
  void scheduled.load()
  // 断线重连兜底：断开期间 WS 事件可能丢失，重连后两页整体重拉
  // （仿 index.ts 现有连接 watch 范式）
  stopConnectionWatch = watch(
    () => mobileApi.isConnected?.value,
    (connected) => {
      if (!connected) return
      void history.refresh()
      void scheduled.load()
    },
  )
})

onUnmounted(() => {
  history.stop()
  stopConnectionWatch?.()
  stopConnectionWatch = null
})
</script>
