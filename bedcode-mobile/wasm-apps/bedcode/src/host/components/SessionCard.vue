<template>
  <div
    class="group-row cursor-pointer transition-colors"
    :class="[session.status === 'stopped' ? 'opacity-60' : '']"
    @click="$emit('click')"
  >
    <span class="icon-chip" :class="statusChipClass">
      <component :is="statusIcon" class="w-5 h-5" />
    </span>

    <!-- 中间列恒定两行，每行 nowrap：标题行=会话名，状态行=任务状态+运行时长。 -->
    <div class="flex-1 min-w-0">
      <div class="group-row-title truncate">{{ session.name }}</div>
      <div class="group-row-sub mt-0.5 flex items-center gap-1.5 min-w-0">
        <span v-if="taskStatusLabel" class="task-state" :class="taskStatusClass">{{ taskStatusLabel }}</span>
        <span class="font-mono truncate min-w-0" style="color: var(--mobile-row-sub)">{{ elapsed }}</span>
      </div>
    </div>

    <span class="status-badge" :class="statusBadgeClass">
      <span v-if="session.status === 'running'" class="status-dot dot-emerald"></span>
      {{ statusLabel }}
    </span>

    <button
      v-if="session.status !== 'stopped'"
      class="ml-1 w-11 h-11 rounded-lg flex items-center justify-center active:opacity-80 transition-colors flex-shrink-0"
      style="background: color-mix(in srgb, var(--mobile-chip-red) 16%, transparent); color: var(--mobile-chip-red); border: 1px solid color-mix(in srgb, var(--mobile-chip-red) 35%, transparent)"
      :title="t('hub.stopSession')"
      @click.stop="$emit('stop')"
    >
      <svg class="w-2.5 h-2.5" fill="currentColor" viewBox="0 0 24 24">
        <rect x="6" y="6" width="12" height="12" rx="2" />
      </svg>
    </button>
    <button
      v-else
      class="ml-1 w-11 h-11 rounded-lg flex items-center justify-center active:opacity-80 transition-colors flex-shrink-0"
      style="background: var(--mobile-chip-zinc-bg); color: var(--mobile-chip-zinc)"
      :title="t('hub.deleteSession')"
      @click.stop="$emit('delete')"
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
        />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 运行中会话行（自旧宿主 `src/components/SessionCard.vue` 迁入插件）
 *
 * 布局契约（旧实现踩过的坑，别改回去）：
 * - 中间列恒定两行：标题行独占=会话名；状态行=任务状态标签 + 运行时长（nowrap +
 *   truncate）。旧版把「类型/时长/任务状态」挤一行，141px 预算被 1px 溢出触发 CJK
 *   逐字折行，行高从 79px 涨到 102px
 * - 字号 2026-10-16 整体提档后（标题 lg / 状态行 base / 状态徽章 sm，见
 *   `mobile.css` 的 .group-row-title / .group-row-sub / .status-badge），右侧
 *   状态徽章与停止按钮都变宽，中列可用宽度比当年那个 141px 预算更窄——上面两条
 *   `truncate` + `nowrap` 是当前唯一的防线，不要给中列加第三个元素
 * - 右侧状态 badge 常驻（颜色只做强化，文案本身承载语义）
 * - 非 stopped 会话给停止按钮；stopped 给删除按钮
 * - 点击整行 = 打开终端（父组件负责预热与跳转）
 */
import { computed, h } from 'vue'
import { useHostPage } from '../useHostPage'
import { useRunTime } from '../useRunTime'

const t = useHostPage().t

const props = defineProps<{
  // 会话形状由宿主连接域投影决定（字段见宿主 RemoteSession）
  session: any
}>()

defineEmits<{
  click: []
  stop: []
  delete: []
}>()

const isRunning = computed(
  () => props.session.status === 'running' || props.session.status === 'waitingInput',
)

const { runTime: elapsed } = useRunTime(
  () => props.session.startedAt || props.session.createdAt || props.session.started_at || props.session.created_at,
  isRunning,
)

const statusChipClass = computed(() => {
  switch (props.session.status) {
    case 'running':
      return 'chip-emerald'
    case 'waitingInput':
      return 'chip-amber'
    default:
      return 'chip-zinc'
  }
})

const statusIcon = computed(() => {
  switch (props.session.status) {
    case 'running':
      return () =>
        h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z',
          }),
        ])
    case 'waitingInput':
      return () =>
        h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
          }),
        ])
    default:
      return () =>
        h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', {
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'stroke-width': '2',
            d: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
          }),
        ])
  }
})

const statusBadgeClass = computed(() => {
  switch (props.session.status) {
    case 'running':
      return 'badge-emerald'
    case 'waitingInput':
      return 'badge-amber'
    default:
      return 'badge-zinc'
  }
})

const statusLabel = computed(() => {
  switch (props.session.status) {
    case 'running':
      return t('hub.statusRunning')
    case 'waitingInput':
      return t('hub.statusWaiting')
    default:
      return t('hub.statusStopped')
  }
})

/** 任务状态语义色：状态各有归属（「已完成」与「执行中」不同色）；色只作强化 */
const taskStatusClass = computed(() => {
  switch (props.session.taskStatus) {
    case 'in_progress':
      return 'task-state--in-progress'
    case 'asking':
      return 'task-state--asking'
    case 'retrying':
      return 'task-state--retrying'
    case 'completed':
      return 'task-state--completed'
    case 'interrupted':
      return 'task-state--interrupted'
    default:
      return 'task-state--idle'
  }
})

const taskStatusLabel = computed(() => {
  const status = props.session.taskStatus
  if (!status) return null
  switch (status) {
    case 'idle':
      return t('hub.taskIdle')
    case 'in_progress':
      return t('hub.taskInProgress')
    case 'asking':
      return t('hub.taskAsking')
    case 'retrying':
      return t('hub.taskRetrying')
    case 'completed':
      return t('hub.taskCompleted')
    case 'interrupted':
      return t('hub.taskInterrupted')
    default:
      return status
  }
})
</script>
