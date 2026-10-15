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
      <h1 class="shell-type-display flex-1 min-w-0 truncate">
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

    <!-- ==================== 卡片轨道 ====================
         卡片并排成一行，**不叠放**：同一时刻只有一张完整可见，其余在屏外；
         屏宽放不下一张时才在边缘露出半张（那是被裁切，不是叠压）。
         横向拖动切卡、上滑关闭、点卡片进入；预览位不伪造应用内截图——
         平台拿不到应用画面，放大应用图标即平台知道的全部事实（AGENTS §5.1 B4）。 -->
    <template v-if="cards.length > 0">
      <div
        ref="deckEl"
        class="shell-deck flex-1 min-h-0"
        :class="{ 'shell-deck--dragging': dragAxis !== null }"
        role="list"
        :aria-label="t('shell.switcher.title')"
        style="touch-action: pan-x"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="resetDrag"
      >
        <div class="shell-deck-track" :style="trackStyle()">
          <article
            v-for="(card, index) in cards"
            :key="card.app.id"
            class="shell-deck-card"
            :style="cardStyle(index)"
            :data-app-id="card.app.id"
            :aria-current="index === activeIndex ? 'true' : undefined"
            role="listitem"
          >
            <!-- 每张卡片整块可点 = 进入 / 启动（Android recents 的同一交互）。
                 内容放进一个铺满卡片的按钮，读屏与键盘拿到的是「进入 {应用名}」
                 这个可访问名，而不是一整张读不出意图的图。 -->
            <button
              type="button"
              class="flex flex-col flex-1 min-h-0 w-full text-left cursor-pointer transition-opacity duration-200 active:opacity-80"
              :aria-label="t(card.running ? 'shell.switcher.enterApp' : 'shell.switcher.launchApp', { name: card.app.name })"
              @click.stop="onCardClick(card)"
            >
              <span class="flex items-center gap-2.5 px-3.5 py-3 flex-shrink-0">
                <ShellAppIcon :app="card.app" :size="34" />
                <span class="flex-1 min-w-0">
                  <span class="block shell-type-label text-[var(--mobile-text-primary)] truncate">
                    {{ card.app.name }}
                  </span>
                  <span class="block shell-type-caption text-[var(--mobile-text-secondary)] truncate">
                    {{ card.running ? t('shell.common.running') : t('shell.switcher.dormant') }}
                  </span>
                </span>
              </span>

              <span
                class="relative flex-1 min-h-0 mx-3.5 mb-3 rounded-[14px] flex flex-col items-center justify-center gap-2 overflow-hidden"
                :style="{
                  // 运行中卡片的水洗底色：success-muted 在浅色主题下只有 0.1 透明度几近不可见，
                  // 用 color-mix 把实色按比例混进底面色，双主题都保持可辨；状态另有文字冗余（非纯色传达）
                  background: card.running
                    ? 'color-mix(in srgb, var(--mobile-success) 12%, var(--mobile-bg-primary))'
                    : 'var(--mobile-bg-primary)',
                }"
                aria-hidden="true"
              >
                <ShellAppIcon :app="card.app" :size="76" />
                <!-- 预览不伪造应用截图（平台拿不到应用画面，AGENTS §5.1 B4）；
                     补一行真实元信息让卡片不是光杆图标（design-taste-v1：空态/占位不裸奔） -->
                <span class="max-w-[80%] px-2 text-center shell-type-caption text-[var(--mobile-text-muted)] truncate">
                  {{ card.app.version ? `v${card.app.version}` : '' }}{{ card.app.permissions.length > 0 ? ` · ${t('shell.apps.permissionCount', { count: card.app.permissions.length })}` : '' }}
                </span>
              </span>
            </button>

            <!-- 关闭：只挂在当前卡片上（Android recents 同款——焦点卡才有清除键），
                 独立按钮压在整卡按钮之上（按钮不能嵌套），并标出
                 data-deck-ignore-drag —— 从它起手不算轨道手势 -->
            <button
              v-if="index === activeIndex"
              type="button"
              data-deck-ignore-drag
              class="absolute top-1.5 right-1.5 z-10 flex items-center justify-center rounded-[8px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
              :aria-label="t('shell.switcher.closeCard', { name: card.app.name })"
              @click.stop="dismiss(card.app.id)"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" :style="{ color: 'var(--mobile-text-secondary)' }" aria-hidden="true">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </article>
        </div>
      </div>

        <!-- 显式导航 + 位置指示：拖拽**不是唯一路径**（WCAG 2.2 Dragging Movements），
             键盘 / 单击用户用这一对按钮与圆点完成同一件事；页码数字对可见用户可读 -->
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
        <span class="shell-type-micro text-[var(--mobile-text-secondary)] tabular-nums" aria-hidden="true">
          {{ activeIndex + 1 }}/{{ cards.length }}
        </span>
        <!-- 位置对读屏可见：圆点是纯视觉，切换后播报「第 N 张，共 M 张」 -->
        <span class="sr-only" role="status" aria-live="polite">
          {{ t('shell.switcher.position', { index: activeIndex + 1, total: cards.length }) }}
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
      <p class="mt-4 shell-type-label text-[var(--mobile-text-primary)]">
        {{ t('shell.switcher.empty') }}
      </p>
      <p class="mt-1 shell-type-caption text-[var(--mobile-text-secondary)]">
        {{ t('shell.switcher.emptyHint') }}
      </p>
      <!-- 空态给主动作，不把用户堵在只有页头返回键的空屏里 -->
      <button
        type="button"
        class="mt-5 flex items-center justify-center rounded-[10px] px-5 min-h-[var(--mobile-touch-target-min)] shell-type-label transition-opacity duration-200 active:opacity-80"
        :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
        @click="nav.goHome()"
      >
        {{ t('shell.switcher.goHome') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 多任务（卡片轨道切换）
 * -----------------------------------------------------------------------------
 * 形态：卡片**并排成一行**，不叠放（Android recents 的同一形态）：同一时刻一张完整
 * 可见，其余在屏外；屏宽放不下一张整卡时才在边缘露出半张——那是被裁切，不是叠压。
 * 轨道按「卡宽 + 间距」平移把当前卡片送到中间，几何真源在 `styles/shell.css`
 * （--deck-card-w / --deck-gap / --deck-step），组件只给序号与手势位移。
 *
 * 三条硬约束：
 *   ① 预览位不伪造内容——平台不持有应用内部快照，放大应用图标 + 状态文字即
 *      平台掌握的全部事实（伪造截图属于 B4 业务投影越线）；
 *   ② 拖拽**不是唯一路径**（WCAG 2.2 Dragging Movements）：页头返回、底部
 *      上一个 / 下一个按钮、位置圆点、整卡点击（= 进入）与 × 都是显式路径；
 *   ③ 手势不互相抢：pointermove 首次越过轴锁阈值即锁定本次手势方向——横向
 *      切卡、纵向上滑关闭，两者不在同一次拖拽里串味；贴屏幕左右边缘起手则
 *      整段让位给系统返回手势，自定义手势不与系统手势抢同一条边。
 *
 * 卡片交互对齐 Android recents：**点哪张开哪张、上滑关哪张关哪张**；拖拽抬手后
 * 浏览器补的那次 click 必须吞掉，否则上滑关闭会顺手把应用打开。
 *
 * 卡片集合 = 运行中应用（MRU 优先）+ 最近使用但未运行的应用。关闭运行中的卡片 =
 * 停用实例并移出最近列表（不然它会以「未运行」身份原地复活，看起来像关不掉）。
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
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

// ==================== 轨道几何 ====================

/** 轴锁阈值：小于该位移视为点击，避免手抖把点击判成拖拽 */
const AXIS_LOCK = 10
/** 切卡位移阈值 */
const SWIPE_STEP = 56
/** 上滑关闭阈值（负值方向） */
const DISMISS_THRESHOLD = 72
/**
 * 屏幕左右边缘让位带（px）
 *
 * ui-ux-pro-max `ux / Gesture Conflicts`：自定义手势不得覆盖系统手势。从这条带里
 * 起手不接管轨道手势，交给系统的返回手势——否则贴着屏幕边缘划动会既切卡又返回。
 */
const EDGE_GUARD = 24

const deckEl = ref<HTMLElement | null>(null)

/**
 * 轨道平移：当前卡片居中，拖拽时跟手
 *
 * 步长（--deck-step = 卡宽 + 间距）与居中留白都在 `styles/shell.css` 里，JS 只
 * 给「序号 + 手势位移」，不自己做布局计算——换卡宽 / 改间距不必回来改这里。
 */
function trackStyle(): Record<string, string> {
  const drag = dragAxis.value === 'x' ? dragDx.value : 0
  return {
    transform: `translate3d(calc(-1 * var(--deck-step) * ${activeIndex.value} + ${drag}px), 0, 0)`,
  }
}

/** 卡片自身样式：只有上滑关闭时的跟手位移；相邻卡片按距离轻微压暗（不叠放、不缩放） */
function cardStyle(index: number): Record<string, string> {
  const depth = Math.abs(index - activeIndex.value)
  const dragging = dragAxis.value === 'y' && cards.value[index]?.app.id === dragTargetId
  const dy = dragging ? dragDy.value : 0
  return {
    opacity: dragging
      ? String(Math.max(0.4, 1 + dy / 360))
      : String(depth === 0 ? 1 : Math.max(0.65, 1 - depth * 0.12)),
    transform: dy === 0 ? '' : `translate3d(0, ${dy}px, 0)`,
  }
}

// ==================== 手势（横向切卡 / 纵向上滑关闭） ====================

const dragAxis = ref<'x' | 'y' | null>(null)
const dragDx = ref(0)
const dragDy = ref(0)
/** 本次手势是否已越过阈值（越过则抬手后的 click 不再触发卡片动作） */
let dragMoved = false
/** 本次手势作用的卡片（并排布局下任意一张都可上滑关闭） */
let dragTargetId: string | null = null
/** 手势起点 */
let dragOrigin: { x: number; y: number } | null = null
/** 本次手势的指针 id（捕获指针时要用） */
let lastPointerId: number | undefined
/** 已 setPointerCapture 的指针（抬起 / 取消时释放） */
let capturedPointerId: number | null = null

/**
 * 接管本次手势的后续事件
 *
 * 只在**越过轴锁、确认是拖拽之后**才捕获：简单点击不捕获，click 仍落在卡片上
 * （「点卡片进入」依赖它）。捕获后手指滑出轨道区（快速上滑）仍能收到
 * move/up，否则手势会停在偏移位上、卡片留在错位状态。
 */
function capturePointer(): void {
  const el = deckEl.value
  const id = lastPointerId
  // 非 Tauri 运行时 / 老 WebView 可能没有该 API：缺了就退化到「不捕获」，功能不受影响
  if (!el || id === undefined || typeof el.setPointerCapture !== 'function') return
  try {
    el.setPointerCapture(id)
    capturedPointerId = id
  } catch (e) {
    capturedPointerId = null
    // 捕获失败 → 快滑出界时手势收不到事件（卡片会停在偏移位），降级但要说出来
    logger.warn('[ShellSwitcher] setPointerCapture failed; drag may end out of deck', e)
  }
}

function releasePointer(): void {
  const el = deckEl.value
  const id = capturedPointerId
  capturedPointerId = null
  if (!el || id === null || typeof el.releasePointerCapture !== 'function') return
  try {
    el.releasePointerCapture(id)
  } catch (e) {
    // 指针已抬起 / 取消时释放会抛（该指针不再活跃），属清理路径，不影响交互
    logger.debug('[ShellSwitcher] releasePointerCapture skipped (pointer no longer active)', e)
  }
}

function onPointerDown(event: PointerEvent): void {
  // 只响应主指针；鼠标右键 / 中键不参与
  if (event.pointerType === 'mouse' && event.button !== 0) return
  // 只把「关闭」按钮排除在轨道手势之外：卡片整块是「进入」按钮，从它上面起手
  // 仍要能横滑切卡 / 上滑关闭，因此这里按标记排除而不是按 button 排除
  if ((event.target as HTMLElement | null)?.closest('[data-deck-ignore-drag]')) return
  // 贴边起手让位给系统返回手势（EDGE_GUARD）
  if (event.clientX <= EDGE_GUARD || event.clientX >= window.innerWidth - EDGE_GUARD) return
  // 命中测试取手指底下那张卡；落在卡片之间的空档（轨道两侧的居中留白很宽）时
  // 退回当前卡片——用户看到的是当前卡片，跟手对象就该是它
  const hitId =
    (event.target as HTMLElement | null)?.closest('[data-app-id]')?.getAttribute('data-app-id') ??
    cards.value[activeIndex.value]?.app.id
  if (!hitId || !cards.value.some((c) => c.app.id === hitId)) return
  dragTargetId = hitId
  dragOrigin = { x: event.clientX, y: event.clientY }
  lastPointerId = event.pointerId
  dragMoved = false
  dragAxis.value = null
  dragDx.value = 0
  dragDy.value = 0
}

function onPointerMove(event: PointerEvent): void {
  if (!dragOrigin) return
  const dx = event.clientX - dragOrigin.x
  const dy = event.clientY - dragOrigin.y
  if (dragAxis.value === null) {
    if (Math.abs(dx) < AXIS_LOCK && Math.abs(dy) < AXIS_LOCK) return
    // 首次越过阈值即锁轴：横向归切卡、纵向归关闭，同一次手势内不再改判
    dragAxis.value = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
    dragMoved = true
    // 确认是拖拽后才捕获指针：手指滑出轨道区仍能收尾
    capturePointer()
  }
  if (dragAxis.value === 'x') {
    dragDx.value = dx
  } else {
    // 只允许上滑（offset ≤ 0）：下滑是「取消」，不应该把卡片往下推
    dragDy.value = Math.min(0, dy)
  }
}

function onPointerUp(event: PointerEvent): void {
  const origin = dragOrigin
  const target = dragTargetId
  const axis = dragAxis.value
  const dx = dragDx.value
  resetDrag()
  if (!origin || !target) return
  if (axis === 'x') {
    if (dx <= -SWIPE_STEP) step(1)
    else if (dx >= SWIPE_STEP) step(-1)
    return
  }
  if (axis !== 'y') return
  // 以抬起位置为准（pointermove 可能因快速滑动而稀疏甚至缺失）
  const offset = Math.min(0, event.clientY - origin.y)
  if (offset <= -DISMISS_THRESHOLD) void dismiss(target)
}

function resetDrag(): void {
  releasePointer()
  dragOrigin = null
  dragTargetId = null
  dragAxis.value = null
  dragDx.value = 0
  dragDy.value = 0
}

/**
 * 拖拽刚结束时吞掉随后的 click
 *
 * 一次拖拽抬手后浏览器仍会补一次 click（落在抬手处那个元素上）。不吞掉的话，
 * 上滑关闭会顺手把应用打开、横滑切卡会连带进入——拖拽与点击必须分开判。
 *
 * @returns true = 这次 click 属于拖拽余波，调用方不要再执行动作
 */
function consumeDragClick(): boolean {
  if (!dragMoved) return false
  dragMoved = false
  return true
}

/** 点卡片 = 进入 / 启动该应用（Android recents：点哪张开哪张） */
function onCardClick(card: RecentCard): void {
  if (consumeDragClick()) return
  void enter(card.app.id)
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
