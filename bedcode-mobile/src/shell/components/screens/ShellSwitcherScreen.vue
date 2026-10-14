<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 页头（返回 + 标题 + 全部关闭） ==================== -->
    <header class="flex items-center gap-2 px-2 pt-2 pb-1 flex-shrink-0">
      <button
        type="button"
        class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.switcher.back')"
        @click="nav.back()"
      >
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M15 6l-6 6 6 6" />
        </svg>
      </button>
      <h1 class="flex-1 min-w-0 text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)] truncate">
        {{ t('shell.switcher.title') }}
      </h1>
      <ShellChip
        v-if="cards.length > 0"
        :label="t('shell.switcher.closeAll')"
        tone="danger"
        clickable
        @click="closeAll"
      />
    </header>

    <!-- ==================== 卡片叠放（deck） ====================
         固定宽高的卡片居中叠放（等差偏移 + 线性缩放）：前面的完整可操作，后面的
         只露一道边。横向滑动切换、上滑关闭、点后层卡片提到前面。
         预览位不伪造应用内截图——平台拿不到应用画面，放大应用图标即平台知道的
         全部事实（AGENTS §5.1 B4）。 -->
    <template v-if="cards.length > 0">
      <div
        class="shell-deck flex-1 min-h-0"
        :class="{ 'shell-deck--dragging': dragAxis !== null }"
        role="list"
        :aria-label="t('shell.switcher.title')"
        style="touch-action: none"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="resetDrag"
      >
        <article
          v-for="(card, index) in cards"
          :key="card.app.id"
          class="shell-deck-card"
          :style="cardStyle(index)"
          :data-app-id="card.app.id"
          :aria-current="index === activeIndex ? 'true' : undefined"
          :aria-hidden="index === activeIndex ? undefined : 'true'"
          role="listitem"
          @click="onCardClick(index)"
        >
          <!-- 前层卡片：完整信息与操作 -->
          <template v-if="index === activeIndex">
            <div class="flex items-center gap-2.5 px-3.5 py-3 flex-shrink-0">
              <ShellAppIcon :app="card.app" :size="34" />
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
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" :style="{ color: 'var(--mobile-text-secondary)' }" aria-hidden="true">
                  <path d="M6 6l12 12M18 6L6 18" />
                </svg>
              </button>
            </div>

            <div
              class="flex-1 min-h-0 mx-3.5 mb-3 rounded-[14px] flex items-center justify-center"
              :style="{ background: 'var(--mobile-bg-primary)' }"
              aria-hidden="true"
            >
              <ShellAppIcon :app="card.app" :size="76" />
            </div>

            <button
              type="button"
              class="mx-3.5 mb-3.5 flex-shrink-0 rounded-[10px] px-4 py-2.5 text-[var(--font-size-base)] transition-opacity duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
              :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
              @click.stop="enter(card.app.id)"
            >
              {{ card.running ? t('shell.switcher.enter') : t('shell.switcher.launch') }}
            </button>
          </template>

          <!-- 后层卡片：只承担「叠放」的视觉，不承载信息与焦点（详细内容在前层） -->
          <div v-else class="flex-1 flex items-center justify-center">
            <ShellAppIcon :app="card.app" :size="56" tone="plain" />
          </div>
        </article>
      </div>

      <!-- 显式导航 + 位置指示：拖拽**不是唯一路径**（WCAG 2.2 Dragging Movements），
           键盘 / 单击用户用这一对按钮与圆点完成同一件事 -->
      <div class="flex items-center justify-center gap-2 pb-3 flex-shrink-0">
        <button
          type="button"
          class="shell-icon-action"
          :disabled="activeIndex === 0"
          :aria-label="t('shell.switcher.prev')"
          @click="step(-1)"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M15 6l-6 6 6 6" />
          </svg>
        </button>
        <span class="flex items-center gap-1.5 px-1" aria-hidden="true">
          <span
            v-for="(card, index) in cards"
            :key="card.app.id"
            class="block rounded-full transition-colors duration-200"
            :style="{
              width: index === activeIndex ? '16px' : '6px',
              height: '6px',
              background: index === activeIndex ? 'var(--mobile-accent)' : 'var(--mobile-border-active)',
            }"
          />
        </span>
        <button
          type="button"
          class="shell-icon-action"
          :disabled="activeIndex >= cards.length - 1"
          :aria-label="t('shell.switcher.next')"
          @click="step(1)"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M9 6l6 6-6 6" />
          </svg>
        </button>
      </div>
    </template>

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
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 多任务（卡片叠放切换）
 * -----------------------------------------------------------------------------
 * 形态：固定宽高的卡片居中叠放（Android recents 的卡片语义 + iOS 叠放的秩序感）。
 * 前层卡片完整可操作，后层卡片等差错位，只露一道边提示「还有几张」。
 *
 * 三条硬约束：
 *   ① 预览位不伪造内容——平台不持有应用内部快照，放大应用图标 + 状态文字即
 *      平台掌握的全部事实（伪造截图属于 B4 业务投影越线）；
 *   ② 拖拽**不是唯一路径**（WCAG 2.2 Dragging Movements）：页头返回、底部
 *      上一个 / 下一个按钮、位置圆点、前层卡片的「进入」与 × 都是显式路径；
 *   ③ 手势不互相抢：pointermove 首次越过轴锁阈值即锁定本次手势方向——横向
 *      切卡、纵向上滑关闭，两者不在同一次拖拽里串味。
 *
 * 卡片集合 = 运行中应用（MRU 优先）+ 最近使用但未运行的应用。关闭运行中的卡片 =
 * 停用实例并移出最近列表（不然它会以「未运行」身份原地复活，看起来像关不掉）。
 */
import { computed, ref, watch } from 'vue'
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
  // 最近使用但未运行：卡片保留在叠放里（Android recents 的同一语义），点它即启动
  const dormant = recentIds.value
    .map((id) => apps.value.find((a) => a.id === id))
    .filter((app): app is ShellApp => app !== undefined && !runningIds.has(app.id))
  return [
    ...running.map((app) => ({ app, running: true })),
    ...dormant.map((app) => ({ app, running: false })),
  ]
})

// ==================== 前层选择 ====================

/** 前层卡片 id；null = 取列表第一张（卡片集合变化时的兜底） */
const activeId = ref<string | null>(null)

const activeIndex = computed(() => {
  const index = cards.value.findIndex((c) => c.app.id === activeId.value)
  return index >= 0 ? index : 0
})

/** 卡片集合变化（启动 / 关闭 / 刷新）后修正前层：原前层消失就落到相邻张 */
watch(cards, (list) => {
  if (list.length === 0) {
    activeId.value = null
    return
  }
  if (!list.some((c) => c.app.id === activeId.value)) {
    activeId.value = list[Math.min(activeIndex.value, list.length - 1)]?.app.id ?? null
  }
})

function activate(index: number): void {
  const target = cards.value[index]
  if (target) activeId.value = target.app.id
}

/** 上一个 / 下一个（越界即无操作，按钮同时以 disabled 表达） */
function step(delta: number): void {
  activate(activeIndex.value + delta)
}

// ==================== 叠放几何 ====================

/** 相邻卡片错位量（px，等差）：偏移 = 序号差 × 该值 */
const OFFSET_X = 20
/** 越靠后越小，线性递减（等差而非等比，避免后排被压成一条线） */
const SCALE_STEP = 0.04
/** 最多呈现几张：再往后只留占位（visibility: hidden），不参与绘制与命中 */
const MAX_DEPTH = 3

/** 轴锁阈值：小于该位移视为点击，避免手抖把点击判成拖拽 */
const AXIS_LOCK = 10
/** 切卡位移阈值 */
const SWIPE_STEP = 56
/** 上滑关闭阈值（负值方向） */
const DISMISS_THRESHOLD = 72

const dragAxis = ref<'x' | 'y' | null>(null)
const dragDx = ref(0)
const dragDy = ref(0)
let dragStart: { x: number; y: number; id: string } | null = null
/** 本次手势是否已越过阈值（越过则抬手时的 click 不再触发「点卡片」） */
let dragMoved = false

function cardStyle(index: number): Record<string, string> {
  const depth = index - activeIndex.value
  const absDepth = Math.abs(depth)
  const dragging = dragAxis.value !== null
  const offsetX = depth * OFFSET_X + (dragging && dragAxis.value === 'x' ? dragDx.value : 0)
  const offsetY =
    absDepth * 7 + (dragging && dragAxis.value === 'y' && depth === 0 ? dragDy.value : 0)
  const hidden = absDepth > MAX_DEPTH
  return {
    zIndex: String(40 - absDepth),
    opacity: hidden ? '0' : String(Math.max(0.3, 1 - absDepth * 0.14)),
    transform: `translate3d(calc(-50% + ${offsetX}px), calc(-50% + ${offsetY}px), 0) scale(${
      1 - absDepth * SCALE_STEP
    })`,
    visibility: hidden ? 'hidden' : 'visible',
    pointerEvents: hidden ? 'none' : 'auto',
  }
}

// ==================== 手势（横向切卡 / 纵向上滑关闭） ====================

function onPointerDown(event: PointerEvent): void {
  // 只响应主指针；鼠标右键 / 中键不参与
  if (event.pointerType === 'mouse' && event.button !== 0) return
  // 卡片上的按钮自己处理点击：从按钮上起手不做叠放手势，避免「点关闭」被判成滑动
  if ((event.target as HTMLElement | null)?.closest('button')) return
  const card = cards.value[activeIndex.value]
  if (!card) return
  dragStart = { x: event.clientX, y: event.clientY, id: card.app.id }
  dragMoved = false
  dragAxis.value = null
  dragDx.value = 0
  dragDy.value = 0
}

function onPointerMove(event: PointerEvent): void {
  if (!dragStart) return
  const dx = event.clientX - dragStart.x
  const dy = event.clientY - dragStart.y
  if (dragAxis.value === null) {
    if (Math.abs(dx) < AXIS_LOCK && Math.abs(dy) < AXIS_LOCK) return
    // 首次越过阈值即锁轴：横向归切卡、纵向归关闭，同一次手势内不再改判
    dragAxis.value = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
    dragMoved = true
  }
  if (dragAxis.value === 'x') {
    dragDx.value = dx
  } else {
    // 只允许上滑（offset ≤ 0）：下滑是「取消」，不应该把卡片往下推
    dragDy.value = Math.min(0, dy)
  }
}

function onPointerUp(event: PointerEvent): void {
  const start = dragStart
  const axis = dragAxis.value
  const dx = dragDx.value
  resetDrag()
  if (!start) return
  if (axis === 'x') {
    if (dx <= -SWIPE_STEP) activate(activeIndex.value + 1)
    else if (dx >= SWIPE_STEP) activate(activeIndex.value - 1)
    return
  }
  if (axis !== 'y') return
  // 以抬起位置为准（pointermove 可能因快速滑动而稀疏甚至缺失）
  const offset = Math.min(0, event.clientY - start.y)
  if (offset <= -DISMISS_THRESHOLD) void dismiss(start.id)
}

function resetDrag(): void {
  dragStart = null
  dragAxis.value = null
  dragDx.value = 0
  dragDy.value = 0
}

/** 点后层卡片 = 提到前层（前层卡片上的按钮各自 @click.stop，不会走到这里） */
function onCardClick(index: number): void {
  if (dragMoved) {
    dragMoved = false
    return
  }
  if (index !== activeIndex.value) activate(index)
}

// ==================== 动作 ====================

/** 关闭一张卡片：运行中先停用，然后（无论是否在跑）移出最近列表 */
async function dismiss(appId: string): Promise<void> {
  const index = cards.value.findIndex((c) => c.app.id === appId)
  if (index === -1) return
  // 前层移走后落到相邻张（后一张优先，没有则前一张）
  if (activeId.value === appId || index === activeIndex.value) {
    activeId.value =
      cards.value[index + 1]?.app.id ?? cards.value[index - 1]?.app.id ?? null
  }
  const app = cards.value[index]
  if (app?.running) await stop(appId)
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

/** 进入应用：先回主屏重置栈，再走统一入口（未运行的会先启动，待审批落详情页） */
async function enter(appId: string): Promise<void> {
  nav.goHome()
  await open(appId)
}
</script>
