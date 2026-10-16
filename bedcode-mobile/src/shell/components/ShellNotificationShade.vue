<template>
  <Teleport to="body">
    <!-- 遮罩：点任意处关闭（Android 通知栏的同一语义：下拉面之外的区域即「收起」） -->
    <Transition name="shell-scrim">
      <div
        v-if="open"
        class="fixed inset-0 z-[60]"
        style="background: var(--mobile-overlay)"
        @click="close"
      />
    </Transition>

    <!-- 通知栏面板：自顶部滑下（独立面，覆盖壳内一切屏幕与覆盖层） -->
    <Transition name="shell-shade">
      <section
        v-if="open"
        ref="panelEl"
        class="shell-ui fixed inset-x-0 top-0 z-[60] flex flex-col max-h-[85dvh] rounded-b-[20px] border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-card)]"
        :style="{ paddingTop: `calc(${topInset}px + 6px)` }"
        role="dialog"
        aria-modal="true"
        :aria-label="t('shell.notify.title')"
        tabindex="-1"
        @keydown.esc="close"
      >
        <!-- ==================== 头部 ==================== -->
        <div class="flex items-center gap-2 px-4 pt-2 pb-2 flex-shrink-0">
          <div class="flex-1 min-w-0">
            <h2 class="shell-type-title truncate">
              {{ t('shell.notify.title') }}
            </h2>
            <p class="shell-type-caption text-[var(--mobile-text-secondary)] truncate">
              {{ t('shell.notify.count', { count: totalCount }) }}
            </p>
          </div>
          <button
            type="button"
            class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
            :aria-label="t('shell.notify.close')"
            @click="close"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>

        <!-- ==================== 内容 ==================== -->
        <div class="min-h-0 overflow-y-auto pb-4">
          <!-- 待处理：待审批应用（由应用状态派生，批准后自然消失，无独立生命周期） -->
          <template v-if="pendingApps.length > 0">
            <ShellSection :title="t('shell.notify.pending')" :note="t('shell.notify.pendingNote')" />
            <div class="px-4 pb-1 flex flex-col gap-2">
              <div
                v-for="app in pendingApps"
                :key="app.id"
                class="flex items-center gap-3 rounded-[12px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)] px-3 py-3"
              >
                <ShellAppIcon :app="app" :size="32" />
                <div class="flex-1 min-w-0">
                  <div class="shell-type-label text-[var(--mobile-text-primary)] truncate">
                    {{ app.name }}
                  </div>
                  <div class="shell-type-caption text-[var(--mobile-text-secondary)] truncate">
                    {{ t('shell.notify.pendingBody', { count: app.permissions.length }) }}
                  </div>
                </div>
                <ShellChip
                  :label="t('shell.notify.pendingAction')"
                  tone="warn"
                  clickable
                  @click="openApproval(app.id)"
                />
              </div>
            </div>
          </template>

          <!-- 通知列表：壳动作 + 宿主引擎事件（文案由来源方给出） -->
          <ShellSection
            v-if="notifications.length > 0"
            :title="t('shell.notify.list')"
            :note="t('shell.notify.listNote', { count: notifications.length })"
          />
          <TransitionGroup
            v-if="notifications.length > 0"
            name="notify-list"
            tag="div"
            class="px-4 flex flex-col gap-2"
          >
            <div
              v-for="(item, index) in notifications"
              :key="item.id"
              class="notify-card-wrap relative overflow-hidden rounded-[12px]"
              :class="{ 'notify-card--flee': clearingAll && !item.sticky }"
              :style="clearingAll && !item.sticky ? { animationDelay: `${index * 48}ms` } : undefined"
            >
              <!-- 滑动删除指示层：卡片滑开时露出淡色底 + 红色垃圾桶——手势的删除表达，
                   不给驻留项（它们不响应滑动）。底色与图标色落在 .notify-swipe-reveal
                   （shell.css）：破坏色实心整块太抢眼，改用 token 派生的一层薄信号。
                   跟随卡片圆角被外层裁切。 -->
              <div
                v-if="!item.sticky"
                class="notify-swipe-reveal absolute inset-0 flex items-center justify-end pr-5"
                aria-hidden="true"
              >
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 6h18 M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2 M19 6l-.8 13.1a2 2 0 0 1-2 1.9H7.8a2 2 0 0 1-2-1.9L5 6 M10 11v6 M14 11v6" />
                </svg>
              </div>
              <article
              class="notify-card relative overflow-hidden rounded-[12px] border border-[var(--mobile-border-hover)] bg-[var(--mobile-bg-card)] px-3 py-2.5"
              :class="{ 'notify-card--dragging': cardDragState(item.id)?.dragging }"
              :style="cardStyle(item)"
              :role="item.appId ? 'button' : undefined"
              :tabindex="item.appId ? 0 : undefined"
              :aria-label="item.appId ? t('shell.notify.openByTap', { title: item.title }) : undefined"
              @pointerdown="onCardPointerDown($event, item)"
              @pointermove="onCardPointerMove($event, item)"
              @pointerup="onCardPointerUp($event, item)"
              @pointercancel="resetCardDrag(item.id)"
              @click="onCardClick(item)"
              @keydown.enter="item.appId && openApp(item.appId)"
              @keydown.space.prevent="item.appId && openApp(item.appId)"
            >
              <!-- 类型色条：progress=进行中（强调色左条）；信息/事件卡不画条，保持克制的纯卡片 -->
              <span
                v-if="item.kind === 'progress'"
                class="absolute left-0 top-0 bottom-0 w-[3px]"
                :style="{ background: 'var(--mobile-accent)' }"
                aria-hidden="true"
              />
              <div class="flex items-start gap-2.5">
                <!-- 图标块：圆角方块（Android 13 通知形态），progress 强调色高亮，普通态描边中性 -->
                <span
                  class="flex items-center justify-center w-8 h-8 rounded-[10px] border flex-shrink-0"
                  :style="
                    item.kind === 'progress'
                      ? { background: 'var(--mobile-accent-muted)', color: 'var(--mobile-accent)', borderColor: 'transparent' }
                      : {
                          background: 'var(--mobile-bg-elevated)',
                          color: 'var(--mobile-text-secondary)',
                          borderColor: 'var(--mobile-border)',
                        }
                  "
                  aria-hidden="true"
                >
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path :d="item.icon" />
                  </svg>
                </span>
                <!-- 扁平布局：标题行右侧嵌「来源 · 时间」，不单占一行；正文可有可无 -->
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="flex-1 min-w-0 shell-type-label text-[var(--mobile-text-primary)] truncate">
                      {{ item.title }}
                    </span>
                    <span class="flex-shrink-0 flex items-center gap-1 shell-type-caption text-[var(--mobile-text-muted)]">
                      <span class="max-w-[96px] truncate">{{ item.source }}</span>
                      <span aria-hidden="true">·</span>
                      <span class="tabular-nums">{{ relativeTime(item.at) }}</span>
                    </span>
                  </div>
                  <div v-if="item.body" class="mt-0.5 shell-type-caption text-[var(--mobile-text-secondary)] break-words">
                    {{ item.body }}
                  </div>
                  <!-- 进度类通知：进度条（无进度值的进度通知不渲染，避免空条） -->
                  <div
                    v-if="item.kind === 'progress' && typeof item.progress === 'number'"
                    class="mt-1.5 h-1 rounded-full overflow-hidden"
                    :style="{ background: 'var(--mobile-accent-muted)' }"
                    role="progressbar"
                    :aria-valuenow="item.progress"
                    aria-valuemin="0"
                    aria-valuemax="100"
                  >
                    <span
                      class="block h-full rounded-full"
                      :style="{ width: `${item.progress}%`, background: 'var(--mobile-accent)' }"
                    />
                  </div>
                </div>
                <!-- 驻留项没有单条关闭：它表示「需要处理」而不是「看过就算」。
                     X 是滑动删除的可达性替代（键盘 / 读屏 / 不支持手势时）。 -->
                <button
                  v-if="!item.sticky"
                  type="button"
                  class="flex items-center justify-center flex-shrink-0 rounded-[8px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] -mr-1 transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
                  :aria-label="t('shell.notify.dismissOne', { title: item.title })"
                  @click.stop="dismiss(item.id)"
                >
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" :style="{ color: 'var(--mobile-text-secondary)' }" aria-hidden="true">
                    <path d="M6 6l12 12M18 6L6 18" />
                  </svg>
                </button>
              </div>
              </article>
            </div>
          </TransitionGroup>

          <!-- 空态：说明「通知从哪来」，而不是只丢一句「暂无通知」 -->
          <div v-if="pendingApps.length === 0 && notifications.length === 0" class="px-6 pt-8 text-center">
            <span
              class="inline-flex items-center justify-center w-14 h-14 rounded-[18px]"
              :style="{ background: 'var(--mobile-bg-elevated)', color: 'var(--mobile-text-secondary)' }"
              aria-hidden="true"
            >
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                <path d="M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8" />
                <path d="M10.3 21a2 2 0 0 0 3.4 0" />
              </svg>
            </span>
            <p class="mt-3 shell-type-body text-[var(--mobile-text-primary)]">
              {{ t('shell.notify.empty') }}
            </p>
            <p class="mt-1 shell-type-caption text-[var(--mobile-text-secondary)]">
              {{ t('shell.notify.emptyHint') }}
            </p>
          </div>
        </div>

        <!-- 底部动作：清除全部。触发级联飞离动画（等差异步右飞），全部飞出后才真正
             清空 store——删除要有可见表达，而不是瞬间消失。级联期间按钮禁用防重入。 -->
        <div
          v-if="notifications.length > 0"
          class="flex-shrink-0 border-t border-[var(--mobile-group-divider)] px-4 py-3"
        >
          <button
            type="button"
            class="shell-btn-danger-outline w-full rounded-[10px] px-4 py-2.5 shell-type-label min-h-[var(--mobile-touch-target-min)]"
            :disabled="clearingAll"
            @click="clearAllCascade"
          >
            {{ t('shell.notify.clearAll') }}
          </button>
        </div>
      </section>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 通知栏（Android 式独立通知面）
 * -----------------------------------------------------------------------------
 * 打开入口在状态条与主屏铃铛（见 ShellStatusbar / ShellHomeScreen），本组件只负责
 * 面板本身：待处理（审批派生）+ 通知（壳动作 / 宿主引擎事件）+ 清除全部。
 *
 * 为什么不接管「顶部边缘下拉」手势：Android 上从屏幕最顶端下拉属于**系统**手势
 * （拉出系统通知栏），应用层再实现一次会与之冲突（ui-ux-pro-max `--domain ux`
 * Gesture Conflicts：不要覆盖系统手势）。因此入口做成显式点击，拖拽不是唯一路径。
 *
 * 焦点：打开时聚焦面板（键盘 Esc 可关），关闭时焦点回到打开它的元素——
 * 覆盖层不管理焦点会让键盘/读屏用户迷失在背景内容里。
 */
import { computed, inject, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import i18n from '@/locales'
import { useShellApps } from '../composables/useShellApps'
import {
  useShellNotifications,
  type ShellNotification,
} from '../composables/useShellNotifications'
import { useShellOverlays } from '../composables/useShellOverlays'
import { useShellNavigation } from '../composables/useShellNavigation'
import { openShellApp } from '../composables/useShellOpen'
import ShellAppIcon from './ShellAppIcon.vue'
import ShellChip from './ShellChip.vue'
import ShellSection from './ShellSection.vue'

const { t } = useI18n()
const overlays = useShellOverlays()
const nav = useShellNavigation()
const { apps } = useShellApps()
const { notifications, notificationCount, dismissNotification, clearNotifications } =
  useShellNotifications()

const open = computed(() => overlays.shadeOpen.value)
const pendingApps = computed(() => apps.value.filter((a) => a.state === 'approval'))
const totalCount = computed(() => notificationCount.value + pendingApps.value.length)

/** 相对时间随面板打开而刷新：否则「3 分钟前」会一直停在打开那一刻的读数 */
const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | null = null

function startTicker(): void {
  if (ticker !== null) return
  now.value = Date.now()
  ticker = setInterval(() => {
    now.value = Date.now()
  }, 30_000)
}

function stopTicker(): void {
  if (ticker === null) return
  clearInterval(ticker)
  ticker = null
}

/** 相对时间：分钟 / 小时 / 天三档，够用且不引入日期库 */
function relativeTime(at: number): string {
  const minutes = Math.max(0, Math.round((now.value - at) / 60_000))
  if (minutes < 1) return i18n.global.t('shell.notify.justNow')
  if (minutes < 60) return i18n.global.t('shell.notify.minutesAgo', { count: minutes })
  const hours = Math.round(minutes / 60)
  if (hours < 24) return i18n.global.t('shell.notify.hoursAgo', { count: hours })
  return i18n.global.t('shell.notify.daysAgo', { count: Math.round(hours / 24) })
}

// 安全区由 App.vue 注入（面板贴顶渲染，必须自己让开系统状态栏）；缺省 0 =
// 浏览器预览 / 单测没有系统状态栏。
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }> | undefined>(
  'safeArea',
  undefined,
)
const topInset = computed(() => safeArea?.value.top ?? 0)

/** 面板元素：打开时聚焦（键盘 Esc 可关） */
const panelEl = ref<HTMLElement | null>(null)

/** 打开它的元素：关闭时把焦点还回去（键盘/读屏用户的上下文恢复） */
let lastFocused: HTMLElement | null = null

watch(open, async (value) => {
  if (value) {
    lastFocused = (document.activeElement as HTMLElement | null) ?? null
    startTicker()
    await nextTick()
    panelEl.value?.focus()
  } else {
    stopTicker()
    lastFocused?.focus?.()
    lastFocused = null
  }
})

onMounted(() => {
  if (open.value) startTicker()
})

onUnmounted(() => {
  stopTicker()
  if (clearCascadeTimer !== null) {
    window.clearTimeout(clearCascadeTimer)
    clearCascadeTimer = null
  }
})

function close(): void {
  overlays.closeShade()
}

function dismiss(id: string): void {
  dismissNotification(id)
}

// ==================== 通知卡：左右滑动删除 + 点击打开 ====================
// 滑动是增强（Android 通知同款手势），X 按钮保留为无障碍/键盘替代；
// 驻留项（sticky = 需要处理）不响应滑动与关闭。点击打开只对有关联应用的
// 通知生效；滑动中抑制点击，避免「滑到一半松手」误触发打开。

/** 拖动阈值（px）：超过即删除，未超回弹 */
const SWIPE_DISMISS_PX = 88

/** 正在拖动的卡片 id 与位移 x（拖动中无过渡，跟手） */
const dragId = ref<string | null>(null)
const dragX = ref(0)
/** 已释放但正在滑出动画的卡片（位移定格在屏外，动画结束 dismiss） */
const swipeOut = ref<{ id: string; x: number } | null>(null)

let dragStartX = 0
let dragStartY = 0
let dragAxis: 'x' | 'y' | null = null
let suppressTap = false

/** 拖动中状态（模板据此关过渡） */
function cardDragState(id: string): { dragging: boolean } {
  return { dragging: dragId.value === id }
}

/** 卡片位移样式：拖动跟手（由 cardDragState 关过渡），滑出动画定格屏外，其余静止 */
function cardStyle(item: ShellNotification): Record<string, string> {
  const style: Record<string, string> = {
    boxShadow: 'var(--mobile-card-shadow)',
    touchAction: 'pan-y',
  }
  if (dragId.value === item.id) {
    style.transform = `translateX(${dragX.value}px)`
  } else if (swipeOut.value?.id === item.id) {
    style.transform = `translateX(${swipeOut.value.x}px)`
  }
  return style
}

/** 起点在按钮上（X）不开始滑动；驻留项不响应 */
function onCardPointerDown(e: PointerEvent, item: ShellNotification): void {
  if (item.sticky) return
  if ((e.target as HTMLElement).closest('button')) return
  dragId.value = item.id
  dragX.value = 0
  dragStartX = e.clientX
  dragStartY = e.clientY
  dragAxis = null
  suppressTap = false
}

function onCardPointerMove(e: PointerEvent, item: ShellNotification): void {
  if (dragId.value !== item.id) return
  const dx = e.clientX - dragStartX
  const dy = e.clientY - dragStartY
  // 首帧定轴：水平位移占优才开始拖（超过 8px）；垂直交给列表滚动
  if (dragAxis === null) {
    if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return
    dragAxis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
    if (dragAxis === 'y') {
      resetCardDrag(item.id)
      return
    }
  }
  if (dragAxis !== 'x') return
  e.preventDefault()
  dragX.value = dx
  suppressTap = true
}

function onCardPointerUp(_e: PointerEvent, item: ShellNotification): void {
  if (dragId.value !== item.id) return
  const x = dragX.value
  dragId.value = null
  if (Math.abs(x) > SWIPE_DISMISS_PX) {
    // 滑出动画（notify-card 的 transform 过渡生效）后移除；x 定格在屏外
    swipeOut.value = { id: item.id, x: x > 0 ? window.innerWidth : -window.innerWidth }
    window.setTimeout(() => {
      swipeOut.value = null
      dismiss(item.id)
    }, 220)
    return
  }
  // 回弹：dragX 归零，由卡片 transition 负责动画
  dragX.value = 0
}

function resetCardDrag(id: string): void {
  if (dragId.value === id) {
    dragId.value = null
    dragX.value = 0
  }
}

/** 整卡点击打开（有关联应用时）；纯滑动的松手不触发放开 */
function onCardClick(item: ShellNotification): void {
  if (!item.appId || suppressTap) return
  void openApp(item.appId)
}

function clearAll(): void {
  clearNotifications()
}

// ==================== 全部删除（级联飞离动画） ====================
// 点「清除全部」：非驻留卡按 index 等差延迟（48ms/张）依次加速右飞，
// 最后一张飞出后才清空 store——删除有可见表达而不是瞬间消失（prefers-reduced-motion
// 下动画被 CSS 禁用，仍以完整时长走 clear 逻辑，行为不受动画影响）。
const FLEE_STEP_MS = 48
const FLEE_DURATION_MS = 360 /* 与 CSS .notify-card--flee 动画时长一致 + 余量 */

/** 级联飞离进行中（防重入；动画结束时清空 store） */
const clearingAll = ref(false)
let clearCascadeTimer: number | null = null

function clearAllCascade(): void {
  if (clearingAll.value) return
  const fleeCount = notifications.value.filter((n) => !n.sticky).length
  if (fleeCount === 0) {
    clearAll()
    return
  }
  clearingAll.value = true
  const totalMs = (fleeCount - 1) * FLEE_STEP_MS + FLEE_DURATION_MS
  clearCascadeTimer = window.setTimeout(() => {
    clearCascadeTimer = null
    clearingAll.value = false
    clearAll()
  }, totalMs)
}

/** 去审批：关面板 + 进应用详情（那里有「批准并启动」） */
function openApproval(appId: string): void {
  close()
  nav.openDetail(appId)
}

/** 打开通知关联的应用（走统一打开口径：待审批会落到详情页） */
async function openApp(appId: string): Promise<void> {
  close()
  await openShellApp(appId)
}
</script>
