<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题，右刷新 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg
          class="w-4 h-4 text-[var(--text-secondary)]"
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
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('desktop.sidebar.terminalSession') }}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="sessions" />
        <button class="wb-btn-ghost" @click="refresh">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          {{ t('common.button.refresh') }}
        </button>
      </div>
    </div>

    <!-- ==================== 兜底提示：会话中心插件未接管本页 ==================== -->
    <div class="px-6 pt-3 flex-shrink-0">
      <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3">
        <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">
          {{ t('desktop.session.fallbackNotice') }}
        </p>
        <p class="mt-1 text-xs text-[var(--text-secondary)]">
          {{ t('desktop.session.fallbackHint') }}
        </p>
      </div>
    </div>

    <!-- ==================== 内容：配置（启动）+ 运行中的会话（停止/删除） ==================== -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div class="max-w-5xl mx-auto space-y-6">
        <!-- Loading -->
        <div v-if="isLoading" class="flex flex-col items-center justify-center py-20">
          <svg
            class="w-5 h-5 animate-spin text-[var(--text-secondary)] mb-3"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="2"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
            ></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-secondary)]">
            {{ t('common.status.loading') }}
          </p>
        </div>

        <template v-else>
          <!-- 配置：可启动（新建/编辑配置由会话中心插件提供） -->
          <section>
            <h3 class="wb-section-title">
              {{ t('desktop.session.sessions', { count: configs.length }) }}
            </h3>
            <div
              v-if="configs.length === 0"
              class="flex flex-col items-center justify-center py-14"
            >
              <p class="text-sm text-[var(--text-primary)]">{{ t('desktop.session.noConfig') }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">
                {{ t('desktop.session.noConfigHint') }}
              </p>
            </div>
            <div v-else class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)]">
              <div
                v-for="config in configs"
                :key="config.id"
                class="flex items-center gap-3 px-4 h-12 border-b border-[var(--border)] last:border-b-0"
              >
                <span class="text-xs font-medium text-[var(--text-primary)] truncate">
                  {{ config.name }}
                </span>
                <span
                  class="wb-mono text-[calc(10.5px*var(--ui-scale))] uppercase px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)] flex-shrink-0"
                >
                  {{ envBadge(config.environment) }}
                </span>
                <span class="wb-mono text-xs text-[var(--text-secondary)] truncate flex-1">
                  {{ config.command || '—' }}
                </span>
                <button
                  class="wb-btn-primary h-7 px-3 flex-shrink-0"
                  :disabled="isOperating"
                  @click="startConfig(config.id)"
                >
                  {{ t('common.button.start') }}
                </button>
              </div>
            </div>
          </section>

          <!-- 运行中的会话：停止 / 删除 -->
          <section>
            <h3 class="wb-section-title">
              {{ t('desktop.session.runningSessions', { count: runningSessions.length }) }}
            </h3>
            <div
              v-if="runningSessions.length === 0"
              class="flex flex-col items-center justify-center py-14"
            >
              <p class="text-sm text-[var(--text-primary)]">
                {{ t('desktop.session.noSessions') }}
              </p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">
                {{ t('desktop.session.noSessionsHint') }}
              </p>
            </div>
            <div
              v-else
              class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] divide-y divide-[var(--border)]"
            >
              <div
                v-for="session in runningSessions"
                :key="session.id"
                class="flex items-center gap-3 px-4 h-12"
              >
                <span
                  :class="['w-2 h-2 rounded-full flex-shrink-0', statusDot(session.status)]"
                ></span>
                <span class="text-xs font-medium text-[var(--text-primary)] truncate">
                  {{ session.name }}
                </span>
                <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)]">
                  {{ statusText(session.status) }}
                </span>
                <span class="flex-1"></span>
                <button
                  class="wb-btn-ghost h-7 px-3 flex-shrink-0"
                  :disabled="isOperating"
                  @click="confirmStop(session)"
                >
                  {{ t('common.button.stop') }}
                </button>
                <button
                  class="wb-btn-ghost h-7 px-3 flex-shrink-0 hover:!text-red-600 dark:hover:!text-red-400"
                  :disabled="isOperating"
                  @click="confirmDelete(session)"
                >
                  {{ t('common.button.delete') }}
                </button>
              </div>
            </div>
          </section>
        </template>
      </div>
    </div>

    <!-- 停止会话确认 -->
    <Modal v-model="showStopConfirm" :title="t('desktop.session.confirmStop')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('desktop.session.confirmStopMsg', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showStopConfirm = false">
            {{ t('common.button.cancel') }}
          </button>
          <button
            class="wb-btn-primary bg-[var(--color-danger)]"
            :disabled="isOperating"
            @click="doStop"
          >
            {{ t('common.button.stop') }}
          </button>
        </div>
      </template>
    </Modal>

    <!-- 删除会话确认 -->
    <Modal v-model="showDeleteConfirm" :title="t('desktop.session.confirmDeleteSession')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('desktop.session.confirmDeleteRunning', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showDeleteConfirm = false">
            {{ t('common.button.cancel') }}
          </button>
          <button
            class="wb-btn-primary bg-[var(--color-danger)]"
            :disabled="isOperating"
            @click="doDelete"
          >
            {{ t('desktop.session.stopAndDelete') }}
          </button>
        </div>
      </template>
    </Modal>

    <!-- 操作中遮罩 -->
    <Teleport to="body">
      <div
        v-if="isOperating"
        class="fixed inset-0 bg-black/40 flex items-center justify-center z-50"
      >
        <div
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-6 py-5 flex items-center gap-3"
        >
          <svg
            class="w-4 h-4 animate-spin text-[var(--text-secondary)]"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="2"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
            ></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-primary)]">{{ operatingMessage }}</p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionsFallbackView — 会话页宿主兜底壳（票 13）
 *
 * 会话列表 / 配置 CRUD / 终端窗口由 `com.bedcode.session` 插件贡献页接管；
 * 本组件只在插件未激活 / error / 停用时渲染（让位与深链判据见
 * `useSidebarMenu.builtinSupersededBy` 与 `router` 守卫），业务富交互（配置新建
 * 与编辑、终端预览入口）已随插件迁出，此处只保留「不白屏 + 基本启停删除」。
 *
 * 数据仍走宿主命令面（薄转发门面）：插件在否两条路径都可用——插件未激活时
 * 门面自动降级到宿主旧路径（票 09/10 的桥接降级）。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import Modal from '@/components/Modal.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import { useToast } from '@/composables/useToast'
import {
  deleteSession,
  killSession,
  listSessionConfigs,
  listSessions,
  startSession,
  type SessionConfig,
  type SessionInfo,
} from '@/composables/useDesktopCommands'

const { t } = useI18n()
const toast = useToast()

const configs = ref<SessionConfig[]>([])
const sessions = ref<SessionInfo[]>([])
const isLoading = ref(true)
const isOperating = ref(false)
const operatingMessage = ref(t('desktop.session.processing'))
const pendingSession = ref<SessionInfo | null>(null)
const showStopConfirm = ref(false)
const showDeleteConfirm = ref(false)

/** 运行中（非 stopped / error）的会话 —— 兜底壳只列可操作项 */
const runningSessions = computed(() =>
  sessions.value.filter((s) => s.status !== 'stopped' && s.status !== 'error'),
)

function envBadge(env: string | undefined | null): string {
  const v = (env ?? '').toLowerCase()
  if (v === 'wsl2') return 'wsl2'
  if (v === 'linux') return 'linux'
  return 'win'
}

function statusDot(status: string): string {
  switch (status) {
    case 'running':
      return 'bg-green-500 animate-pulse'
    case 'waitingInput':
      return 'bg-amber-500'
    case 'error':
      return 'bg-red-500'
    default:
      return 'bg-[var(--text-tertiary)]'
  }
}

function statusText(status: string): string {
  switch (status) {
    case 'starting':
      return t('common.status.starting')
    case 'running':
      return t('common.status.running')
    case 'waitingInput':
      return t('common.status.asking')
    case 'error':
      return t('common.status.error')
    case 'stopped':
      return t('common.status.stopped')
    default:
      return t('common.status.unknown')
  }
}

async function loadAll() {
  try {
    configs.value = await listSessionConfigs()
    sessions.value = await listSessions()
  } catch (e) {
    logger.error('[SessionsFallbackView] load failed:', e)
    toast.error(t('desktop.session.loadFailed'))
  }
}

async function refresh() {
  await loadAll()
  toast.info(t('desktop.session.listRefreshed'))
}

async function startConfig(configId: string) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.starting')
  try {
    await startSession(configId)
    await loadAll()
    toast.success(t('desktop.session.sessionStarted'))
  } catch (e) {
    logger.error('[SessionsFallbackView] startSession error:', e)
    toast.error(t('desktop.session.startFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
  }
}

function confirmStop(session: SessionInfo) {
  pendingSession.value = session
  showStopConfirm.value = true
}

async function doStop() {
  if (!pendingSession.value) return
  const sessionId = pendingSession.value.id
  isOperating.value = true
  operatingMessage.value = t('desktop.session.stopping')
  try {
    await killSession(sessionId)
    await loadAll()
    toast.info(t('desktop.session.sessionStopped'))
  } catch (e) {
    logger.error('[SessionsFallbackView] killSession error:', e)
    toast.error(t('desktop.session.stopFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showStopConfirm.value = false
    pendingSession.value = null
  }
}

function confirmDelete(session: SessionInfo) {
  pendingSession.value = session
  showDeleteConfirm.value = true
}

async function doDelete() {
  if (!pendingSession.value) return
  const sessionId = pendingSession.value.id
  isOperating.value = true
  operatingMessage.value = t('desktop.session.stoppingAndDeleting')
  try {
    // 运行中的会话先停止再删除（与插件侧编排同一语义）
    await killSession(sessionId)
    await deleteSession(sessionId)
    await loadAll()
    toast.success(t('desktop.session.sessionDeleted'))
  } catch (e) {
    logger.error('[SessionsFallbackView] delete session error:', e)
    toast.error(t('desktop.session.deleteFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showDeleteConfirm.value = false
    pendingSession.value = null
  }
}

onMounted(async () => {
  isLoading.value = true
  await loadAll()
  isLoading.value = false
})
</script>
