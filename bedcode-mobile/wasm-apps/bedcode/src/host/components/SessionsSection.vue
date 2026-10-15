<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- ==================== Header（旧 SessionsView 页头：标题 + 刷新） ==================== -->
    <div class="page-header flex-shrink-0">
      <div class="page-header-row">
        <div class="min-w-0 flex-1">
          <h1 class="page-title">{{ t('hub.sessionTitle') }}</h1>
        </div>
        <div class="flex items-center gap-2">
          <button
            v-if="isAuthenticated"
            class="w-11 h-11 flex items-center justify-center rounded-lg transition-colors active:opacity-80"
            style="color: var(--mobile-accent)"
            :title="t('hub.refreshSessions')"
            @click="refreshSessions"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Content（页面内容滚动容器：scrollbar-gutter-stable 防滚动条显隐引起整页横向抖动；
         session-scroll 局部收细并压暗滚动条） -->
    <div class="scrollbar-gutter-stable session-scroll flex-1 overflow-auto px-4 pb-8 min-h-0">
      <!-- 未连接提示（图标 + 引导 + 去连接入口：会话 → 连接联动） -->
      <div
        v-if="!isAuthenticated"
        class="min-h-[45vh] flex flex-col items-center justify-center text-center"
      >
        <svg
          class="w-12 h-12 mb-4"
          style="color: var(--mobile-text-disabled)"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
        <p class="group-row-sub">{{ t('hub.notConnected') }}</p>
        <p class="text-sm mt-1" style="color: var(--mobile-text-disabled)">
          {{ t('hub.notConnectedHint') }}
        </p>
        <button
          class="mt-5 h-11 px-6 rounded-xl text-sm font-medium transition-colors active:opacity-80"
          style="background: var(--mobile-accent); color: var(--mobile-text-on-accent)"
          @click="$emit('open-connection')"
        >
          {{ t('hub.qrConnect') }}
        </button>
      </div>

      <template v-else>
        <!-- 区块头：两个区块共用同一结构与高度（可折叠标题 + 右侧动作槽），
             避免「可折叠标题行」与「裸文字标题行」在垂直节奏上对不齐 -->
        <div class="section-head pt-1">
          <button
            class="section-head__title"
            :aria-expanded="configsExpanded"
            @click="configsExpanded = !configsExpanded"
          >
            <span class="section-head__label">{{ t('hub.sessionConfigs') }}</span>
            <svg
              class="config-chevron section-head__chevron"
              :class="{ 'config-chevron--collapsed': !configsExpanded }"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
          </button>
          <button
            class="section-head__action"
            :title="t('hub.refreshConfig')"
            :disabled="isRefreshing"
            @click="refreshConfigs"
          >
            <svg
              class="w-5 h-5"
              :class="{ 'animate-spin': isRefreshing }"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
          </button>
        </div>

        <!-- 折叠内容区：grid-template-rows 0fr→1fr 内容展开过渡（内容常驻 DOM 以保留
             TransitionGroup 离场动画与列表状态） -->
        <div class="config-section-body" :class="{ 'config-section-body--collapsed': !configsExpanded }">
          <div class="overflow-hidden">
            <!-- 加载骨架 -->
            <div v-if="isLoadingConfigs && !hasLoadedConfigs" class="space-y-3 mt-3">
              <div
                v-for="i in 3"
                :key="i"
                class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 animate-pulse"
              >
                <div class="flex items-start gap-3">
                  <div class="w-12 h-12 rounded-xl" style="background: var(--mobile-chip-zinc-bg)"></div>
                  <div class="flex-1">
                    <div class="h-4 w-32 rounded mb-2" style="background: var(--mobile-chip-zinc-bg)"></div>
                    <div class="h-3 w-48 rounded" style="background: var(--mobile-chip-zinc-bg)"></div>
                  </div>
                </div>
              </div>
            </div>

            <!-- 无配置空态 -->
            <div
              v-else-if="!isLoadingConfigs && sessionConfigs.length === 0 && hasLoadedConfigs"
              class="min-h-[40vh] flex flex-col items-center justify-center text-center"
            >
              <svg
                class="w-12 h-12 mb-4"
                style="color: var(--mobile-text-disabled)"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.5"
                  d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                />
              </svg>
              <p class="group-row-sub">{{ t('hub.noConfig') }}</p>
              <p class="text-sm mt-1" style="color: var(--mobile-text-disabled)">
                {{ t('hub.noConfigHint') }}
              </p>
            </div>

            <!-- 会话配置列表（配置卡片启动会话；运行中会话见下方独立区域） -->
            <TransitionGroup v-else name="config-list" tag="div" class="space-y-3 mt-3">
              <SessionConfigCard
                v-for="config in sessionConfigs"
                :key="config.id"
                :config="config"
                :is-starting="startingConfigId === config.id"
                @start="handleStartSession"
                @navigate-to-files="handleNavigateToFiles"
              />
            </TransitionGroup>
          </div>
        </div>

        <!-- 运行中的会话：与会话配置同级别的独立区域 -->
        <div class="mt-6">
          <div class="section-head">
            <span class="section-head__label">{{ t('hub.runningSessions') }}</span>
          </div>

          <!-- 运行中会话列表 -->
          <div v-if="realSessions.length > 0" class="group-card mt-3">
            <SessionCard
              v-for="session in realSessions"
              :key="session.id"
              :session="session"
              @click="handleSessionClick(session)"
              @stop="handleStopSession(session)"
              @delete="handleDeleteSession(session)"
            />
          </div>

          <!-- 运行中会话空态 -->
          <div
            v-if="realSessions.length === 0"
            class="min-h-[20vh] flex flex-col items-center justify-center text-center"
          >
            <svg
              class="w-12 h-12 mb-4"
              style="color: var(--mobile-text-disabled)"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
              />
            </svg>
            <p class="group-row-sub">{{ t('hub.noSessions') }}</p>
            <p class="text-sm mt-1" style="color: var(--mobile-text-disabled)">
              {{ t('hub.noSessionsHint') }}
            </p>
          </div>
        </div>
      </template>
    </div>

    <!-- Loading Dialog: 终端准备中（弹窗遮罩，会话页面保持可见；旧实现就绪前即跳转，
         此处保持一致——真正可见时间取决于列表渲染耗时） -->
    <LoadingDialog :visible="isNavigating" :message="t('hub.preparingTerminal')" />

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" :title="t('hub.confirmStop')" size="sm">
      <p style="color: var(--mobile-text-secondary)">
        {{ t('hub.confirmStopMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">{{ t('hub.actionCancel') }}</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">
            {{ t('hub.stop') }}
          </Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirmation Modal -->
    <Modal v-model="showDeleteConfirm" :title="t('hub.confirmDelete')" size="sm">
      <p style="color: var(--mobile-text-secondary)">
        {{ t('hub.confirmDeleteMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirm = false">{{ t('hub.actionCancel') }}</Button>
          <Button variant="danger" :loading="isDeleting" @click="confirmDelete">
            {{ t('hub.remove') }}
          </Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话区（旧宿主 `Views/SessionsView.vue` 形态还原，2026-10-15）
 * -----------------------------------------------------------------------------
 * 还原范围：页头（标题 + 刷新）、未连接空态（含「去连接」入口 = 会话 → 连接联动）、
 * 可折叠会话配置区块（骨架 / 空态 / 配置卡）、运行中会话区块（SessionCard：
 * 状态徽章 + 运行时长 + 任务状态 + 停止/删除）、停止/删除二次确认、终端准备 LoadingDialog。
 *
 * 数据面（与旧实现同源）：会话列表 / 配置走宿主连接域投影（`activeSessions` /
 * `sessionConfigs`），启动 / 停止 / 删除走插件命令面（`terminal-session.*`）。
 *
 * 与旧实现的三处**有意**差异：
 * 1. 会话可见性判据用 `isPaired`（已认证）而非 `isConnected`（传输层）：未认证时
 *    受保护端点一律 401，列出会话只会得到空列表 + 报错噪音（旧实现在「已连未配对」
 *    时会展示空列表）
 * 2. 打开终端前预热**传输面**（`terminal-session.prewarm`，只建连、不发 subscribe 帧）：
 *    桌面端每次 subscribe 都从环窗口起点整段重放，预热期页面通道尚未登记，那笔重放
 *    注定被丢弃（长会话可达数 MB）；真正的订阅由终端页挂载后发出
 * 3. 启动会话沿用「留在会话页」的老行为（列表刷新 + toast），但**补回终端上限校验**
 *    （`maxOpenTerminals`，旧实现在宿主 composable 里，新界面改走插件命令后曾丢失）
 */
import { computed, ref, watch } from 'vue'
import { getRouter } from '@binblink/bedcode-plugin-sdk-mobile'
import { useHostPage } from '../useHostPage'
import { isSessionActive } from '../utils'
import Button from './ui/Button.vue'
import LoadingDialog from './ui/LoadingDialog.vue'
import Modal from './ui/Modal.vue'
import SessionCard from './SessionCard.vue'
import SessionConfigCard, { type SessionConfigSummary } from './SessionConfigCard.vue'

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: true })
const emit = defineEmits<{
  /** 打开终端（AppRoot 切整页沉浸终端） */
  (e: 'open-terminal', sessionId: string, sessionName: string): void
  /** 切到连接页（未连接空态的「去连接」入口） */
  (e: 'open-connection'): void
}>()

const ctrl = useHostPage()
const t = ctrl.t
const toast = ctrl.toast
const logger = ctrl.logger
const api = ctrl.api

const isAuthenticated = computed(() => api.isPaired.value)
const realSessions = computed<any[]>(() => api.activeSessions.value ?? [])
const sessionConfigs = computed<any[]>(() => api.sessionConfigs.value ?? [])
const isLoadingConfigs = computed(() => api.isLoadingConfigs.value)
const hasLoadedConfigs = computed(() => api.hasLoadedConfigs.value)

const startingConfigId = ref<string | null>(null)
const isRefreshing = ref(false)
/** 会话配置区域折叠状态（页签常驻下往返保持；不持久化到设置） */
const configsExpanded = ref(true)

// 停止确认弹窗
const showStopConfirm = ref(false)
const pendingSession = ref<any>(null)
const isStopping = ref(false)

// 删除确认弹窗
const showDeleteConfirm = ref(false)
const isDeleting = ref(false)

// 终端跳转中的 LoadingDialog（旧口径：跳转本身是即时的，弹窗只在渲染间隙可见）
const isNavigating = ref(false)

/** 终端上限（旧实现在宿主 composable 里读同一宿主设置项；非法值回退 5） */
function maxOpenTerminalsLimit(): number {
  const value = Number(ctrl.mobileSettings.value?.maxOpenTerminals)
  return Number.isFinite(value) && value > 0 ? Math.min(20, Math.max(1, Math.round(value))) : 5
}

/** 当前占用槽位的运行中会话数（running / waitingInput 计入；stopped 不占槽位） */
const runningCount = computed(
  () => realSessions.value.filter((s: any) => isSessionActive(s?.status)).length,
)

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

// ==================== 数据加载 ====================

/** 刷新会话列表 + 配置（页头刷新按钮） */
async function refreshSessions(): Promise<void> {
  if (!isAuthenticated.value) return
  try {
    await ctrl.refreshSessions()
  } catch (e) {
    logger.error(`[host] refresh sessions failed: ${errText(e)}`)
    toast.showToast(t('hub.loadFailed'), 'error')
  }
}

/** 仅刷新会话配置（区块头刷新按钮） */
async function refreshConfigs(): Promise<void> {
  isRefreshing.value = true
  try {
    await api.loadSessionConfigs()
  } catch (e) {
    logger.error(`[host] refresh session configs failed: ${errText(e)}`)
    toast.showToast(t('hub.loadFailed'), 'error')
  } finally {
    isRefreshing.value = false
  }
}

// 页签常驻挂载：用 active 复刻旧 `onActivated` 语义——切回会话页时
// 复位导航态，并在「已认证但配置尚未加载」时补拉一次
watch(
  () => props.active,
  (visible) => {
    if (!visible) return
    isNavigating.value = false
    if (isAuthenticated.value && !hasLoadedConfigs.value) void refreshSessions()
  },
)

// ==================== 启动 / 停止 / 删除 ====================

/** 从会话配置启动会话（留在会话页：新会话出现在下方「运行中的会话」区域） */
async function handleStartSession(config: SessionConfigSummary): Promise<void> {
  if (!isAuthenticated.value || startingConfigId.value) return

  // 终端上限：运行中会话已达上限时拒绝启动（旧 host composable 口径）
  const limit = maxOpenTerminalsLimit()
  if (runningCount.value >= limit) {
    toast.showToast(t('hub.maxReached', { max: limit }), 'error')
    return
  }

  startingConfigId.value = config.id
  try {
    const sessionId = await ctrl.startSession(config.id)
    if (!sessionId) {
      logger.error('[host] start session failed: no sessionId returned')
      toast.showToast(t('hub.startFailedNoId'), 'error')
      return
    }
    await ctrl.refreshSessions().catch((e) => logger.warn(`[host] refresh after start failed: ${errText(e)}`))
    toast.showToast(t('hub.startedOk', { name: config.name || config.id }), 'success')
  } catch (e) {
    logger.error(`[host] start session failed: ${errText(e)}`)
    toast.showToast(t('hub.startFailed', { error: errText(e) }), 'error')
  } finally {
    startingConfigId.value = null
  }
}

function handleStopSession(session: any): void {
  pendingSession.value = session
  showStopConfirm.value = true
}

async function confirmStop(): Promise<void> {
  if (!pendingSession.value) return
  isStopping.value = true
  try {
    await ctrl.stopSession(pendingSession.value.id)
    showStopConfirm.value = false
    pendingSession.value = null
    await ctrl.refreshSessions().catch(() => {})
  } catch (e) {
    logger.error(`[host] stop session failed: ${errText(e)}`)
    toast.showToast(t('hub.stopFailed'), 'error')
  } finally {
    isStopping.value = false
  }
}

function handleDeleteSession(session: any): void {
  pendingSession.value = session
  showDeleteConfirm.value = true
}

async function confirmDelete(): Promise<void> {
  if (!pendingSession.value) return
  isDeleting.value = true
  try {
    await ctrl.removeSession(pendingSession.value.id)
    showDeleteConfirm.value = false
    pendingSession.value = null
    await ctrl.refreshSessions().catch(() => {})
  } catch (e) {
    logger.error(`[host] remove session failed: ${errText(e)}`)
    toast.showToast(t('hub.removeFailed'), 'error')
  } finally {
    isDeleting.value = false
  }
}

// ==================== 打开终端 / 进文件浏览 ====================

/** 点击会话行：预热传输面后请求打开终端（旧实现是 subscribe 预热，见本文件头注） */
function handleSessionClick(session: any): void {
  if (isNavigating.value) return
  isNavigating.value = true
  api.activeSessionId.value = session.id

  if (isSessionActive(session?.status)) {
    void ctrl.context.commands
      .execute('terminal-session.prewarm', { sessionId: session.id })
      .catch((e) => logger.warn(`[host] terminal prewarm failed: ${errText(e)}`))
  }

  emit('open-terminal', session.id, session.name || session.id)
}

/** 工程目录 → 宿主文件浏览器（旧实现 push 宿主 `mobile-files` 路由） */
function handleNavigateToFiles(config: SessionConfigSummary): void {
  const session = realSessions.value.find(
    (s: any) => s.config_id === config.id || s.configId === config.id,
  )
  const id = session?.id || config.id
  try {
    getRouter()?.push({ name: 'mobile-files', params: { id } })
  } catch (e) {
    logger.error(`[host] navigate to files failed: ${errText(e)}`)
  }
}
</script>

<style scoped>
/* 页面标题与下方卡片左对齐：.page-header 横向内边距是 20px，而内容容器是 px-4（16px），
   标题会比卡片右移 4px。局部覆写成 16px，不动全局（避免波及其他视图的 header）。 */
.page-header {
  padding-left: 1rem;
  padding-right: 1rem;
}

/* 区块头统一语言：两个区块标题同一高度（44px 触控下限）、同一内缩。
   内缩放在 label 上而非按钮上，这样「可折叠」与「不可折叠」两种区块头的
   标题文字落在同一条竖线上。 */
.section-head {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  min-height: 2.75rem;
}

.section-head__title {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  flex: 1;
  min-width: 0;
  height: 2.75rem;
  border-radius: 0.5rem;
  /* button 默认 text-align:center 会传给后代文字，需显式左对齐 */
  text-align: left;
  transition: opacity 0.15s ease;
}

.section-head__title:active {
  opacity: 0.7;
}

.section-head__label {
  padding-inline: 0.25rem;
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--mobile-text-secondary);
}

.section-head__chevron {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
  color: var(--mobile-text-disabled);
}

.section-head__action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
  flex-shrink: 0;
  border-radius: 0.5rem;
  color: var(--mobile-text-secondary);
  transition:
    opacity 0.15s ease,
    background-color 0.15s ease;
}

.section-head__action:active {
  opacity: 0.7;
}

.section-head__action:disabled {
  opacity: 0.5;
}

/* 内容滚动容器：局部收细到 6px 并压到 --mobile-row-sub 的低透明度，
   保留 scrollbar-gutter-stable 的防抖动价值，只去掉视觉噪音 */
.session-scroll::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

.session-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.session-scroll::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--mobile-row-sub) 40%, transparent);
  border-radius: 999px;
}

/* 会话配置折叠：内容展开走 grid-template-rows 0fr→1fr 过渡
   （内容展开 UX 允许动画布局属性） */
.config-section-body {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.25s ease;
}

.config-section-body--collapsed {
  grid-template-rows: 0fr;
}

.config-chevron {
  transition: transform 0.25s ease;
}

.config-chevron--collapsed {
  transform: rotate(-90deg);
}

.config-list-enter-active {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-leave-active {
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.config-list-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.config-list-move {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

@media (prefers-reduced-motion: reduce) {
  .config-list-enter-active,
  .config-list-leave-active,
  .config-list-move,
  .config-chevron {
    transition: none !important;
  }
}
</style>
