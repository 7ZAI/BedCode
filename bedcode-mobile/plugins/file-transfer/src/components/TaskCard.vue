<script setup lang="ts">
/**
 * TaskCard — 通用任务/历史/接收卡
 *
 * 纯展示：props 决定卡上呈现什么，emit 仅回传「操作语义」由调用方执行业务。
 * 领域模型（Task / ReceivingTask / HistoryEntry）由 TransfersTab 归一化映射后传入，
 * 卡内不做 i18n 之外的逻辑判断，便于将来平移到接收策略变更或暗色主题。
 *
 * 状态色仍走 spec 9.3 四色体系（父级已通过 state-class 传 class，不再硬编码）。
 * 方向 chip：上传 ↑ / 下载 ↓，方向反色时复用基色 --mobile-accent / --mobile-info。
 */
import { computed } from 'vue'
import type { TaskAction } from '../types'

type Direction = 'upload' | 'download'

const props = defineProps<{
  id: string
  direction: Direction
  name: string
  meta: string
  stateLabel: string
  stateClass: string
  /** 0–100；null = 不显示进度条（终态历史 / 收尾） */
  progress: number | null
  /** ft-progress-* 四色体系 class */
  progressClass?: string
  /** 失败/拒绝时附加 reason 文案（key 已在父级解析） */
  reason: string | null
  /** 不确定进度（运行中但 offset=0） */
  indeterminate?: boolean
  /** 操作按钮（语义由 kind 决定） */
  actions: TaskAction[]
}>()

const emit = defineEmits<{
  (e: 'action', kind: TaskAction['kind'], id: string): void
}>()

/** 方向图标：↑ 上传 / ↓ 下载（单色 stroke 1.75，与 spec 9.2 视觉一致） */
const directionIcon = computed(() =>
  props.direction === 'upload'
    ? 'M5 10l7-7m0 0l7 7m-7-7v18'
    : 'M19 14l-7 7m0 0l-7-7m7 7V3',
)

/** 方向 chip class（反色区分：upload = accent / download = info） */
const directionChipClass = computed(() =>
  props.direction === 'upload' ? 'fv2-dir-chip--accent' : 'fv2-dir-chip--info',
)

/** 进度条宽度（percent 数值） */
const progressWidth = computed(() => {
  if (props.indeterminate) return 100
  if (props.progress == null) return 0
  return Math.max(0, Math.min(100, props.progress))
})

/** 进度条 class：indeterminate 走 fv2-progress-indeterminate 动画 */
const progressBarClass = computed(() => {
  if (props.indeterminate) return 'fv2-progress-indeterminate'
  return props.progressClass ?? 'ft-progress-active'
})

function onAction(kind: TaskAction['kind']): void {
  emit('action', kind, props.id)
}
</script>

<template>
  <div class="fv2-card">
    <!-- 主行：方向 chip + 名称 + 状态 chip -->
    <div class="flex items-center gap-2 min-w-0">
      <div class="fv2-dir-chip" :class="directionChipClass">
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="directionIcon" />
        </svg>
      </div>
      <span class="fv2-card-name" :title="name">{{ name }}</span>
      <span class="fv2-card-state shrink-0" :class="stateClass">{{ stateLabel }}</span>
    </div>

    <!-- 元信息行（大小/速率/时间） -->
    <div class="fv2-card-meta">{{ meta }}</div>

    <!-- 进度条（仅传输/接收中可见） -->
    <div v-if="progress !== null || indeterminate" class="fv2-progress-track">
      <div
        class="fv2-progress-bar"
        :class="progressBarClass"
        :style="{ width: progressWidth + '%' }"
      />
    </div>

    <!-- 失败原因（独立行） -->
    <div v-if="reason" class="fv2-card-reason">{{ reason }}</div>

    <!-- 操作按钮行（44px 触控目标） -->
    <div v-if="actions.length > 0" class="fv2-card-actions">
      <button
        v-for="a in actions"
        :key="a.kind + a.label"
        type="button"
        class="fv2-card-action"
        :class="a.variant === 'tint' ? 'fv2-card-action--tint' : 'fv2-card-action--neutral'"
        @click="onAction(a.kind)"
      >
        {{ a.label }}
      </button>
    </div>
  </div>
</template>
