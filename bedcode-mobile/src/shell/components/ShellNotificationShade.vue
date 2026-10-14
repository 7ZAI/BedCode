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
            <h2 class="text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)] truncate">
              {{ t('shell.notify.title') }}
            </h2>
            <p class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
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
                <ShellAppIcon :app="app" :size="34" />
                <div class="flex-1 min-w-0">
                  <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate">
                    {{ app.name }}
                  </div>
                  <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
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
          <div v-if="notifications.length > 0" class="px-4 flex flex-col gap-2">
            <article
              v-for="item in notifications"
              :key="item.id"
              class="rounded-[12px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)] px-3 py-3"
            >
              <div class="flex items-start gap-2.5">
                <span
                  class="flex items-center justify-center w-7 h-7 rounded-full flex-shrink-0"
                  :style="{ background: 'var(--mobile-accent-muted)', color: 'var(--mobile-text-secondary)' }"
                  aria-hidden="true"
                >
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path :d="item.icon" />
                  </svg>
                </span>
                <div class="flex-1 min-w-0">
                  <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] break-words">
                    {{ item.title }}
                  </div>
                  <div v-if="item.body" class="mt-0.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] break-words">
                    {{ item.body }}
                  </div>
                  <div class="mt-1 text-[var(--font-size-xs)] text-[var(--mobile-text-muted)]">
                    {{ item.source }} · {{ relativeTime(item.at) }}
                  </div>
                  <!-- 进度类通知：进度条（无进度值的进度通知不渲染，避免空条） -->
                  <div
                    v-if="item.kind === 'progress' && typeof item.progress === 'number'"
                    class="mt-2 h-1.5 rounded-full overflow-hidden"
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
                <!-- 驻留项没有单条关闭：它表示「需要处理」而不是「看过就算」 -->
                <button
                  v-if="!item.sticky"
                  type="button"
                  class="flex items-center justify-center flex-shrink-0 rounded-[8px] w-9 h-9 transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
                  :aria-label="t('shell.notify.dismissOne', { title: item.title })"
                  @click="dismiss(item.id)"
                >
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" :style="{ color: 'var(--mobile-text-secondary)' }">
                    <path d="M6 6l12 12M18 6L6 18" />
                  </svg>
                </button>
              </div>

              <!-- 关联应用的打开动作：通知是入口而不是终点 -->
              <div v-if="item.appId" class="mt-2 pl-[38px]">
                <ShellChip
                  :label="t('shell.notify.openApp')"
                  tone="accent"
                  clickable
                  @click="openApp(item.appId)"
                />
              </div>
            </article>
          </div>

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
            <p class="mt-3 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ t('shell.notify.empty') }}
            </p>
            <p class="mt-1 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
              {{ t('shell.notify.emptyHint') }}
            </p>
          </div>
        </div>

        <!-- ==================== 底部动作 ==================== -->
        <div
          v-if="notifications.length > 0"
          class="flex-shrink-0 border-t border-[var(--mobile-group-divider)] px-4 py-3"
        >
          <button
            type="button"
            class="w-full rounded-[10px] border border-[var(--mobile-border)] px-4 py-2.5 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
            @click="clearAll"
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
import { useShellNotifications } from '../composables/useShellNotifications'
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
})

function close(): void {
  overlays.closeShade()
}

function dismiss(id: string): void {
  dismissNotification(id)
}

function clearAll(): void {
  clearNotifications()
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
