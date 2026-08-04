<script setup lang="ts">
/**
 * ToolboxEntry — 工具箱入口长条卡片 (Mobile)
 *
 * 渐变圆角图标（⇄）+ 标题 + 副标题 + 右侧实时状态角标。
 * 角标随 `plugin:file-transfer:tasks-changed` 刷新：对端在线且 N 传输中时显示数量，
 * 离线显示「未连接」文案。
 *
 * 由宿主 ToolboxView 经 PluginViewHost 渲染（宿主 provide pluginContext），
 * 因此直接 inject 插件上下文；组件挂载时启动任务监听，卸载时摘除。
 */
import { inject, onMounted, onUnmounted, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import { useTasks } from '../composables/useTasks'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const tasks = useTasks(context)

/** 活跃传输数（transferring + queued + resumable + paused） */
const activeCount = computed(
  () =>
    tasks.summary.value.active +
    tasks.summary.value.queued +
    tasks.summary.value.resumable +
    tasks.summary.value.paused,
)

const online = computed(() => tasks.peerOnline.value)

onMounted(() => {
  tasks.start()
})

onUnmounted(() => {
  tasks.stop()
})
</script>

<template>
  <div class="flex items-center gap-3 min-w-0">
    <!-- 渐变圆角图标 -->
    <div class="ft-entry-icon flex-shrink-0 flex items-center justify-center rounded-xl border border-[var(--mobile-border)]">
      <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
        />
      </svg>
    </div>

    <div class="flex-1 min-w-0">
      <p class="font-medium text-[0.9375rem] text-[var(--mobile-text-primary)] truncate">
        {{ t('transfer.toolbox.title') }}
      </p>
      <p class="text-[var(--mobile-text-muted)] text-sm truncate mt-0.5">
        {{ t('transfer.toolbox.subtitle') }}
      </p>
    </div>

    <!-- 右侧实时状态角标 -->
    <span
      v-if="activeCount > 0"
      class="flex-shrink-0 inline-flex items-center h-6 px-2.5 rounded-full text-xs font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]"
    >
      {{ t('transfer.toolbox.activeCount', { count: activeCount }) }}
    </span>
    <span
      v-else-if="online"
      class="flex-shrink-0 w-2 h-2 rounded-full bg-[var(--mobile-success)] shadow-[0_0_6px_rgba(16,185,129,0.6)]"
    ></span>
    <span
      v-else
      class="flex-shrink-0 inline-flex items-center h-6 px-2.5 rounded-full text-xs font-medium bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]"
    >
      {{ t('transfer.toolbox.disconnected') }}
    </span>
  </div>
</template>
