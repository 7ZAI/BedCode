<script setup lang="ts">
/**
 * ToolboxEntry — 工具箱入口长条卡片 (Mobile)
 *
 * 由宿主 ToolboxView 以 `group-row group-row-btn` 容器渲染，本组件只填充内容：
 *   icon-chip（渐变 ⇄）+ group-row-title + group-row-sub + 右侧状态角标。
 * 角标随 `plugin:file-transfer:tasks-changed` 刷新：对端在线且 N 传输中时显示数量，
 * 离线显示「未连接」文案。
 *
 * 由宿主 ToolboxView 经 PluginViewHost 渲染（宿主 provide pluginContext），
 * 因此直接 inject 插件上下文；组件挂载时启动任务监听，卸载时摘除。
 */
import { inject, onMounted, onUnmounted, computed } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
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

const online = computed(() => tasks.connOnline.value)

onMounted(() => {
  tasks.start()
})

onUnmounted(() => {
  tasks.stop()
})
</script>

<template>
  <!-- PluginViewHost 的 div 无 flex，需自行包裹 flex 容器对齐宿主 group-row 布局 -->
  <div class="flex items-center gap-3 min-w-0 w-full">
    <span class="icon-chip ft-entry-icon flex-shrink-0 flex items-center justify-center">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
        />
      </svg>
    </span>
    <div class="flex-1 min-w-0">
      <div class="group-row-title truncate">{{ t('transfer.toolbox.title') }}</div>
      <div class="group-row-sub mt-0.5 truncate">{{ t('transfer.toolbox.subtitle') }}</div>
    </div>

    <!-- 右侧实时状态角标 -->
    <span
      v-if="activeCount > 0"
      class="status-badge badge-cyan flex-shrink-0"
    >
      {{ t('transfer.toolbox.activeCount', { count: activeCount }) }}
    </span>
    <span
      v-else-if="online"
      class="flex-shrink-0 status-dot dot-emerald"
      :aria-label="t('transfer.toolbox.online')"
      role="img"
    ></span>
    <span
      v-else
      class="status-badge badge-zinc flex-shrink-0"
    >
      {{ t('transfer.toolbox.disconnected') }}
    </span>

    <!-- 可点击 affordance：与宿主默认入口卡片一致的 chevron -->
    <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>
  </div>
</template>

<style scoped>
/* 工具箱入口图标：扁平实色 accent（与插件 FAB 主按钮同语言，
   不用渐变 — 渐变是 AI 套路特征，且与宿主线性图标风格不统一） */
.ft-entry-icon {
  color: var(--mobile-text-on-accent);
  background: var(--mobile-accent);
  border: none;
}

/* 在线状态点脉冲：柔和呼吸提示「实时在线」（ANIMATIONS 规范：尊重减弱动效偏好） */
.dot-emerald {
  animation: ft-entry-dot-pulse 2.4s ease-in-out infinite;
}

@keyframes ft-entry-dot-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.45;
  }
}

@media (prefers-reduced-motion: reduce) {
  .dot-emerald {
    animation: none;
  }
}
</style>
