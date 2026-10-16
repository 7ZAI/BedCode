<template>
  <div class="att-root mobile-ui">
    <!-- 自绘页签：预设任务 / 定时任务 / 任务记录（三面板常驻挂载，切页不丢订阅与滚动位置） -->
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
         已在边界页时继续同向滑动经 delegateSwipe 上交外层页签容器翻主页面 -->
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
      <!-- 三面板常驻挂载（保订阅与滚动位置），切页走透明度+位移过渡；
           面板停靠类由 activeTabIndex 驱动：新页自动从滑动方向侧滑入。
           面板序号与 tabs 数组下标一一对应（panelClass(index) 直接吃下标） -->
      <div :class="['att-tab-panel', panelClass(0)]">
        <PresetTasksTab :context="context" @open-connection="emit('open-connection')" />
      </div>
      <div :class="['att-tab-panel', panelClass(1)]">
        <ScheduledJobsTab :context="context" :scheduled="scheduled" />
      </div>
      <div :class="['att-tab-panel', panelClass(2)]">
        <TaskHistoryTab :context="context" :history="history" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * AutoTaskToolboxView — 任务页（3 页签容器）
 *
 * app 域运行面的「任务」页签与壳内 `tasks` 路由经 PluginViewHost provide pluginContext
 * 后渲染本组件。业务逻辑全部下沉到 composables（useTaskHistory / useScheduledJobs）
 * 与预设任务真源（SDK `getPresetTasks()`），本组件只做：页签切换 + 两页联动
 * （事件去抖重拉 + 断线重连重拉）。
 *
 * 页签顺序 = 预设任务 / 定时任务 / 任务记录（tabs 数组即视觉顺序，面板按下标一一对应；
 * 新增页签必须同时补 tabs 项与对应 panel，否则 panelClass 的下标对不上）。
 */
import { inject, ref, computed, watch, onMounted, onUnmounted } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'
import { useSwipeTabs } from '@binblink/bedcode-plugin-sdk-mobile/ui/swipe-tabs'
import PresetTasksTab from './PresetTasksTab.vue'
import ScheduledJobsTab from './ScheduledJobsTab.vue'
import TaskHistoryTab from './TaskHistoryTab.vue'
import { useTaskHistory } from '../composables/useTaskHistory'
import { useScheduledJobs } from '../composables/useScheduledJobs'
import { delegateSwipe } from '../../app/swipeArbitration'
import { insideHorizontallyScrollable } from '../swipe-zone'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string): string => context.i18n.t(key)
const mobileApi = getMobileApi()

/** 预设任务页需要把用户引导回连接页时向上发事件（页签切换归 app 域） */
const emit = defineEmits<{ 'open-connection': [] }>()

type TabKey = 'preset' | 'scheduled' | 'history'

const activeTab = ref<TabKey>('preset')
const tabs: { key: TabKey; label: string }[] = [
  { key: 'preset', label: 'tasks.tabs.preset' },
  { key: 'scheduled', label: 'tasks.tabs.scheduled' },
  { key: 'history', label: 'tasks.tabs.history' },
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
// 区内水平主导滑动切换页签；已在边界页时继续同向滑动上交外层页签容器翻主页面。
// 仲裁载体是 app 域的 delegateSwipe（原为宿主 MobileSwipeContainer，该容器已随
// 票 2026-10-09 阶段 B 退役；data-zone-at-* 属性保留为无障碍/调试可读状态）。
// 手势跳过：输入类控件内的触摸（定时任务表单，防文本选择拖动误触切页）、
// 真正可横向滚动的容器（如横向卡片 / chips 行——横滑语义是滚动自身）。
// 横向滚动判定见 ../swipe-zone.ts（注意不可用「overflow-x 计算值 + 微小溢出」
// 判定——纵向容器的 overflow-x 会被规范强算为 auto，长串溢出几 px 就误判，
// 会让整个横滑整轮失效；判据已在实测后收紧）。

const zoneRoot = ref<HTMLElement | null>(null)

const {
  onTouchStart: onZoneTouchStart,
  onTouchMove: onZoneTouchMove,
  onTouchEnd: onZoneTouchEnd,
} = useSwipeTabs(
  (dir) => {
    // 步进切换；越界方向上交外层页签容器翻主页面，外层不在场时忽略
    const next = activeTabIndex.value + (dir === 'left' ? 1 : -1)
    if (next >= 0 && next < tabs.length) {
      activeTab.value = tabs[next].key
      return
    }
    delegateSwipe(dir)
  },
  {
    shouldSkip: (target) => {
      const el = target as HTMLElement | null
      return (
        !!el?.closest?.('input, textarea, [contenteditable="true"]') ||
        insideHorizontallyScrollable(target, zoneRoot.value)
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
  // 首屏加载由认证态门控：任务端点是 JWT 保护的，认证前拉取只会吃 401
  // （订阅不依赖认证，仍照常建立，认证后 watch 补拉）
  const paired = mobileApi.isPaired?.value === true
  history.start({ initialLoad: paired })
  if (paired) void scheduled.load()
  // 断线重连 / 首次认证后兜底：断开期间的 WS 事件可能丢失，恢复后两页整体重拉
  stopConnectionWatch = watch(
    () => mobileApi.isPaired?.value,
    (pairedNow) => {
      if (!pairedNow) return
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
