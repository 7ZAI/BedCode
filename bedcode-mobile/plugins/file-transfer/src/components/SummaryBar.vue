<script setup lang="ts">
/**
 * SummaryBar — 底部常驻行动条（三态互斥）
 *
 * 主内容区常驻传输列表后，底栏随之演化为「唯一入口收敛」的三态：
 *   select  — 浏览 tab 有勾选：取消选择 / 批量下载（主 CTA）
 *   active  — 有传输中的任务：总体进度条 + 速度，整体可点回到传输 tab
 *   upload  — 默认：上传文件（主 CTA）+ 队列残留提示
 * 三态按优先级互斥，切换只动 opacity（.fv2-fade），底栏高度恒定不跳动。
 */
import type { FooterMode } from '../types'

const props = defineProps<{
  mode: FooterMode
  uploadLabel: string
  queueHint?: string
  /** select 态：批量下载文案（含数量与总大小） */
  downloadLabel: string
  clearLabel: string
  /** active 态 */
  activeLabel: string
  speedLabel: string
  overallLabel: string
  overallPercent: number
  viewQueueLabel: string
  /** 对端未连接时禁用上传 */
  offline: boolean
}>()

defineEmits<{
  (e: 'upload'): void
  (e: 'download'): void
  (e: 'clear-selection'): void
  (e: 'open-queue'): void
}>()
</script>

<template>
  <div class="fv2-footer">
    <Transition name="fv2-fade" mode="out-in">
      <!-- 多选操作：浏览 tab 勾选后出现 -->
      <div v-if="mode === 'select'" key="select" class="flex items-center gap-3">
        <button class="fv2-btn-neutral flex-shrink-0" @click="$emit('clear-selection')">
          {{ clearLabel }}
        </button>
        <button class="fv2-btn-primary flex-1" @click="$emit('download')">
          <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          <span class="truncate">{{ downloadLabel }}</span>
        </button>
      </div>

      <!-- 活跃传输：总体进度，整体可点回到传输 tab -->
      <button v-else-if="mode === 'active'" key="active" class="fv2-mini" @click="$emit('open-queue')">
        <svg class="w-5 h-5 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
        </svg>
        <div class="flex-1 min-w-0">
          <div class="flex items-center justify-between gap-2">
            <span class="fv2-mini-title">{{ activeLabel }}</span>
            <span class="fv2-mini-speed flex-shrink-0">{{ speedLabel }}</span>
          </div>
          <div class="fv2-progress mt-1">
            <div class="fv2-progress-fill ft-progress-active" :style="{ width: overallPercent + '%' }"></div>
          </div>
        </div>
        <span class="flex-shrink-0" style="font-size: var(--font-size-xs); color: var(--mobile-text-muted)">
          {{ viewQueueLabel }}
        </span>
        <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>

      <!-- 默认：上传 CTA + 队列残留提示 -->
      <div v-else key="upload" class="flex items-center gap-3">
        <button class="fv2-btn-primary flex-1" :disabled="offline" @click="$emit('upload')">
          <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
          </svg>
          <span class="truncate">{{ uploadLabel }}</span>
        </button>
        <button
          v-if="queueHint"
          class="fv2-btn-neutral flex-shrink-0"
          @click="$emit('open-queue')"
        >
          <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7h16M4 12h16M4 17h10" />
          </svg>
          {{ queueHint }}
        </button>
      </div>
    </Transition>
  </div>
</template>
