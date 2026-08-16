<template>
  <div class="att-root mobile-ui">
    <!-- 自绘页签：任务记录 / 定时任务（v-show 双挂载，切页不丢订阅与滚动位置） -->
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
    <div v-show="activeTab === 'history'" class="att-tab-panel">
      <TaskHistoryTab :context="context" :history="history" />
    </div>
    <div v-show="activeTab === 'scheduled'" class="att-tab-panel">
      <ScheduledJobsTab :context="context" :scheduled="scheduled" />
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
import { inject, ref, watch, onMounted, onUnmounted } from 'vue'
import type { PluginContext, MobileHostApi } from '@binblink/plugin-sdk-mobile'
import { getMobileApi } from '@binblink/plugin-sdk-mobile'
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
