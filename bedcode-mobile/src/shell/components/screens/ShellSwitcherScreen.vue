<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 头部 ==================== -->
    <div class="flex items-center gap-2 px-4 pt-3 pb-2 flex-shrink-0">
      <span class="flex-1 min-w-0 text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)] truncate">
        {{ t('shell.switcher.title') }}
      </span>
      <ShellChip
        v-if="cards.length > 0"
        :label="t('shell.switcher.closeAll')"
        tone="danger"
        clickable
        @click="closeAll"
      />
    </div>

    <!-- ==================== 卡片轨道（Android 卡片式切换） ==================== -->
    <div
      v-if="cards.length > 0"
      class="flex-1 min-h-0 flex items-stretch gap-3 overflow-x-auto px-4 pb-3"
      style="scroll-snap-type: x mandatory"
      role="list"
    >
      <article
        v-for="card in cards"
        :key="card.app.id"
        class="flex-shrink-0 w-[78%] max-w-[320px] h-full flex flex-col rounded-[18px] border border-[var(--mobile-border)] overflow-hidden"
        :style="{
          background: 'var(--mobile-bg-card)',
          boxShadow: 'var(--mobile-card-shadow)',
          scrollSnapAlign: 'center',
          transform: dragTransform(card.app.id),
          opacity: dragOpacity(card.app.id),
          touchAction: 'pan-x',
        }"
        role="listitem"
        @pointerdown="onPointerDown($event, card.app.id)"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="resetDrag"
      >
        <!-- 卡片头：图标 + 名称 + 状态 + 关闭 -->
        <div class="flex items-center gap-2.5 px-3 py-2.5 flex-shrink-0">
          <ShellAppIcon :app="card.app" :size="30" />
          <span class="flex-1 min-w-0">
            <span class="block text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate">
              {{ card.app.name }}
            </span>
            <span class="block text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
              {{ card.running ? t('shell.common.running') : t('shell.switcher.dormant') }}
            </span>
          </span>
          <button
            type="button"
            class="flex items-center justify-center flex-shrink-0 rounded-[8px] w-9 h-9 transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
            :aria-label="t('shell.switcher.closeCard', { name: card.app.name })"
            @click.stop="dismiss(card.app.id)"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>

        <!-- 预览区：平台不持有应用内部快照，这里给「平台知道的事实」——
             运行态、权限数、应用 id，而不是伪造一张截图（AGENTS §5.1 B4） -->
        <div
          class="flex-1 min-h-0 mx-3 mb-3 rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] flex flex-col items-center justify-center gap-1.5 px-4 text-center"
          :style="{ background: 'var(--mobile-bg-primary)' }"
          aria-hidden="true"
        >
          <span
            class="flex items-center justify-center w-12 h-12 rounded-[16px]"
            :style="{ background: 'var(--mobile-bg-elevated)', color: 'var(--mobile-text-secondary)' }"
          >
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="4" width="18" height="16" rx="3" />
              <path d="M3 9h18" />
            </svg>
          </span>
          <span class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
            {{ t('shell.switcher.previewHint') }}
          </span>
          <span class="text-[var(--font-size-xs)] text-[var(--mobile-text-muted)] truncate max-w-full">
            {{ t('shell.switcher.previewMeta', { count: card.app.permissions.length, id: card.app.id }) }}
          </span>
        </div>

        <!-- 卡片脚：进入应用 -->
        <button
          type="button"
          class="mx-3 mb-3 flex-shrink-0 rounded-[10px] px-4 py-2.5 text-[var(--font-size-base)] transition-opacity duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
          :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
          @click="enter(card.app.id)"
        >
          {{ card.running ? t('shell.switcher.enter') : t('shell.switcher.launch') }}
        </button>
      </article>
    </div>

    <!-- ==================== 空态 ==================== -->
    <div v-else class="flex-1 flex flex-col items-center justify-center px-6 text-center">
      <span
        class="flex items-center justify-center w-14 h-14 rounded-[18px]"
        :style="{ background: 'var(--mobile-bg-elevated)', color: 'var(--mobile-text-secondary)' }"
        aria-hidden="true"
      >
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="3" />
          <path d="M3 9h18" />
        </svg>
      </span>
      <p class="mt-4 text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">
        {{ t('shell.switcher.empty') }}
      </p>
      <p class="mt-1 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.switcher.emptyHint') }}
      </p>
      <button
        type="button"
        class="mt-5 rounded-[10px] px-5 py-2.5 text-[var(--font-size-base)] transition-opacity duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
        :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
        @click="nav.goHome()"
      >
        {{ t('shell.switcher.backHome') }}
      </button>
    </div>

    <!-- 手势提示：拖拽有替代路径（卡片上的 × 与「全部关闭」），这里只说能力不说教 -->
    <p
      v-if="cards.length > 0"
      class="flex-shrink-0 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] pb-2"
    >
      {{ t('shell.switcher.foot') }}
    </p>
  </div>
</template>

<script setup lang="ts">
/**
 * 多任务（Android 卡片式切换）
 * -----------------------------------------------------------------------------
 * 形态取自 Android recents：横向卡片轨道（原生 scroll-snap 横向滚动，无需自写手势），
 * 卡片上滑关闭、点卡片进入、右上角 × 关闭、「全部关闭」清场。
 *
 * 两条硬约束：
 *   ① 预览区不伪造内容——平台不持有应用内部快照，卡片只呈现平台知道的事实
 *      （运行态 / 权限数 / 应用 id），伪造截图属于 B4 业务投影越线；
 *   ② 上滑关闭**不是唯一路径**（WCAG 2.2 Dragging Movements）：每张卡片都有 ×，
 *      头部有「全部关闭」。卡片设 `touch-action: pan-x`，纵向拖动归本组件、
 *      横向拖动归原生滚动，两者不互相抢（ui-ux-pro-max `--domain ux` Gesture Conflicts）。
 *
 * 卡片集合 = 运行中应用（MRU 优先）+ 最近使用但未运行的应用。关闭运行中的卡片 =
 * 停用实例并移出最近列表（不然它会以「未运行」身份原地复活，看起来像关不掉）。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellOpen } from '../../composables/useShellOpen'
import { useShellRecent } from '../../composables/useShellRecent'
import type { ShellApp } from '../../types'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const { apps, stop } = useShellApps()
const { recentIds, removeRecent, clearRecent } = useShellRecent()
const { open } = useShellOpen()

interface RecentCard {
  app: ShellApp
  running: boolean
}

/** 最近使用排序：recentIds 有记录的按记录顺序，其余运行中应用按名称兜底 */
function mruRank(app: ShellApp): number {
  const index = recentIds.value.indexOf(app.id)
  return index === -1 ? Number.MAX_SAFE_INTEGER : index
}

const cards = computed<RecentCard[]>(() => {
  const running = apps.value
    .filter((a) => a.state === 'running')
    .sort((a, b) => {
      const diff = mruRank(a) - mruRank(b)
      return diff !== 0 ? diff : a.name.localeCompare(b.name, 'zh-Hans-CN')
    })
  const runningIds = new Set(running.map((a) => a.id))
  // 最近使用但未运行：卡片保留在轨道里（Android recents 的同一语义），点它即启动
  const dormant = recentIds.value
    .map((id) => apps.value.find((a) => a.id === id))
    .filter((app): app is ShellApp => app !== undefined && !runningIds.has(app.id))
  return [
    ...running.map((app) => ({ app, running: true })),
    ...dormant.map((app) => ({ app, running: false })),
  ]
})

// ==================== 上滑关闭（拖拽） ====================

/** 触发关闭的上滑位移（px，负值方向） */
const DISMISS_THRESHOLD = 72

const dragging = ref<{ id: string; startY: number; offset: number } | null>(null)

function dragTransform(appId: string): string | undefined {
  const drag = dragging.value
  if (!drag || drag.id !== appId) return undefined
  return `translateY(${drag.offset}px)`
}

function dragOpacity(appId: string): number | undefined {
  const drag = dragging.value
  if (!drag || drag.id !== appId) return undefined
  return Math.max(0.35, 1 + drag.offset / 400)
}

function onPointerDown(event: PointerEvent, appId: string): void {
  // 只响应主指针；鼠标右键 / 中键不参与
  if (event.pointerType === 'mouse' && event.button !== 0) return
  dragging.value = { id: appId, startY: event.clientY, offset: 0 }
}

function onPointerMove(event: PointerEvent): void {
  const drag = dragging.value
  if (!drag) return
  // 只允许上滑（offset ≤ 0）：下滑是「取消」，不应该把卡片推出轨道
  drag.offset = Math.min(0, event.clientY - drag.startY)
}

function onPointerUp(event: PointerEvent): void {
  const drag = dragging.value
  dragging.value = null
  if (!drag) return
  // 以抬起位置为准（pointermove 可能因快速滑动而稀疏甚至缺失）：上滑才计入，
  // 下滑视为取消，不能把卡片往下推
  const offset = Math.min(0, event.clientY - drag.startY)
  if (offset <= -DISMISS_THRESHOLD) void dismiss(drag.id)
}

function resetDrag(): void {
  dragging.value = null
}

// ==================== 动作 ====================

/** 关闭一张卡片：运行中先停用，然后（无论是否在跑）移出最近列表 */
async function dismiss(appId: string): Promise<void> {
  const app = apps.value.find((a) => a.id === appId)
  if (app?.state === 'running') await stop(appId)
  removeRecent(appId)
}

/** 全部关闭：停止所有运行中实例并清空最近列表（Android「清除全部」的同一语义） */
async function closeAll(): Promise<void> {
  const running = apps.value.filter((a) => a.state === 'running')
  for (const app of running) {
    await stop(app.id)
  }
  clearRecent()
}

/** 进入应用：未运行的会先启动（openShellApp 统一口径，待审批会落到详情页） */
async function enter(appId: string): Promise<void> {
  await open(appId)
}
</script>
