<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题，右刷新/新建 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg class="w-4 h-4 text-[var(--text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.sidebar.session') }}</h2>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="sessions" />
        <button class="wb-btn-ghost" @click="refreshSessions">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          {{ t('common.button.refresh') }}
        </button>
        <button class="wb-btn-primary" @click="showCreateDialog = true">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 4v16m8-8H4" />
          </svg>
          {{ t('desktop.session.newConfig') }}
        </button>
      </div>
    </div>

    <!-- ==================== Tab 切换：终端配置 / 运行中的会话 ==================== -->
    <div class="px-6 pt-3 flex-shrink-0">
      <div class="flex items-center gap-1 p-1 rounded-lg bg-[var(--bg-hover)]">
        <button
          v-for="tab in sessionTabs"
          :key="tab.key"
          class="h-8 flex-1 px-4 rounded-md text-[calc(12px*var(--ui-scale))] font-medium transition-colors duration-200"
          :class="
            activeTab === tab.key
              ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <!-- 内容区：按功能分 tab -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div class="max-w-5xl mx-auto">
        <!-- Loading -->
        <div v-if="isLoading" class="flex flex-col items-center justify-center py-20">
          <svg class="w-5 h-5 animate-spin text-[var(--text-secondary)] mb-3" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-secondary)]">{{ t('common.status.loading') }}</p>
        </div>

        <Transition name="tab-fade" mode="out-in">
          <!-- Tab1：终端配置 -->
          <div v-if="activeTab === 'configs'" class="space-y-6">
            <!-- Empty -->
            <div v-if="configs.length === 0" class="flex flex-col items-center justify-center py-20">
              <p class="text-sm text-[var(--text-primary)]">{{ t('desktop.session.noConfig') }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">{{ t('desktop.session.noConfigHint') }}</p>
              <button class="wb-btn-primary mt-5 h-8 px-4" @click="showCreateDialog = true">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 4v16m8-8H4" />
                </svg>
                {{ t('desktop.session.newConfig') }}
              </button>
            </div>

            <!-- Section：配置 -->
            <section v-else>
            <h3 class="wb-section-title">
              {{ t('desktop.session.sessions', { count: configs.length }) }}
            </h3>
            <div class="grid grid-cols-1 2xl:grid-cols-2 gap-4">
              <div
                v-for="config in configs"
                :key="config.id"
                class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-4 hover:shadow-sm transition-shadow"
              >
                <!-- 卡片头：名称 + 环境/自启动标签 -->
                <div class="flex items-center gap-2">
                  <h4 class="text-sm font-semibold text-[var(--text-primary)] truncate flex-1">{{ config.name }}</h4>
                  <span class="wb-mono text-[calc(10.5px*var(--ui-scale))] uppercase px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)] flex-shrink-0">
                    {{ config.environment === 'wsl2' ? 'wsl2' : 'win' }}
                  </span>
                  <span
                    v-if="cfgAutoStart(config)"
                    class="wb-mono text-[calc(10.5px*var(--ui-scale))] uppercase px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)] flex-shrink-0"
                  >auto</span>
                </div>

                <!-- 技术值：路径 + 命令，mono -->
                <div class="mt-3 space-y-1.5 min-w-0">
                  <div class="flex items-center gap-2 text-[var(--text-secondary)] min-w-0">
                    <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                    <span class="wb-mono truncate" :title="cfgDir(config)">{{ cfgDir(config) || '—' }}</span>
                  </div>
                  <div class="flex items-center gap-2 text-[var(--text-secondary)] min-w-0">
                    <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3" />
                    </svg>
                    <span class="wb-mono truncate" :title="cfgCommand(config)">{{ cfgCommand(config) || '—' }}</span>
                  </div>
                </div>

                <!-- 该配置下的会话 -->
                <div v-if="sessionsOf(config).length > 0" class="mt-3 pt-3 border-t border-[var(--border)] space-y-1">
                  <div
                    v-for="session in sessionsOf(config)"
                    :key="session.id"
                    class="flex items-center gap-2 text-xs rounded-[6px] px-1.5 py-1 -mx-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
                    :title="t('desktop.terminal.viewTerminal')"
                    @click="viewSession(session)"
                  >
                    <span :class="['w-1.5 h-1.5 rounded-full flex-shrink-0', statusDot(session.status)]"></span>
                    <span class="text-[var(--text-primary)] truncate">{{ session.name }}</span>
                    <span class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0">
                      {{ isRunningStatus(session.status) ? runTimeText(session) : formatDateTime(session.startedAt || session.createdAt || session.created_at || '') }}
                    </span>
                    <span class="flex-1"></span>
                    <button
                      class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                      :title="t('desktop.terminal.viewTerminal')"
                      @click.stop="viewSession(session)"
                    >
                      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                      </svg>
                    </button>
                    <button
                      class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                      :title="t('desktop.terminal.restartSession')"
                      @click.stop="restartSession(session)"
                    >
                      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                      </svg>
                    </button>
                    <button
                      class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                      :title="t('common.button.stop')"
                      @click.stop="confirmStopSession(session)"
                    >
                      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 6h12v12H6z" />
                      </svg>
                    </button>
                    <button
                      class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-red-600 dark:hover:text-red-400 transition-colors flex-shrink-0"
                      :title="t('common.button.delete')"
                      @click.stop="confirmDeleteSession(session)"
                    >
                      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </div>
                </div>

                <!-- 卡片底部操作 -->
                <div class="mt-3 pt-3 border-t border-[var(--border)] flex items-center gap-2">
                  <button
                    class="wb-btn-primary"
                    @click="startSession(config.id)"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
                    </svg>
                    {{ t('common.button.start') }}
                  </button>
                  <span class="flex-1"></span>
                  <span v-if="runningOf(config).length > 0" class="wb-mono text-[calc(11px*var(--ui-scale))] text-green-600 dark:text-green-400 flex items-center gap-1.5">
                    <span class="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse"></span>
                    {{ runningOf(config).length }}
                  </span>
                  <button class="wb-btn-ghost" @click="editConfig(config)">
                    {{ t('common.button.edit') }}
                  </button>
                  <button class="wb-btn-ghost hover:!text-red-600 dark:hover:!text-red-400" @click="deleteConfig(config.id)">
                    {{ t('common.button.delete') }}
                  </button>
                </div>
              </div>
            </div>
          </section>
          </div>

          <!-- Tab2：运行中的会话（跨配置汇总，操作与配置卡片内会话一致） -->
          <div v-else class="space-y-6">
            <h3 class="wb-section-title">
              {{ t('desktop.session.runningSessions', { count: runningSessions.length }) }}
            </h3>
            <div v-if="runningSessions.length === 0" class="flex flex-col items-center justify-center py-20">
              <p class="text-sm text-[var(--text-primary)]">{{ t('desktop.session.noSessions') }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">{{ t('desktop.session.noSessionsHint') }}</p>
            </div>
            <div v-else class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] divide-y divide-[var(--border)]">
              <div
                v-for="session in runningSessions"
                :key="session.id"
                class="flex items-center gap-3 px-4 h-12"
              >
                <span :class="['w-2 h-2 rounded-full flex-shrink-0', statusDot(session.status)]"></span>
                <span class="text-xs font-medium text-[var(--text-primary)] truncate cursor-pointer hover:underline" @click="viewSession(session)">{{ session.name }}</span>
                <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0">{{ statusText(session.status) }}</span>
                <span class="flex-1"></span>
                <span class="wb-mono text-[calc(11.5px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0">{{ runTimeText(session) }}</span>
                <!-- 查看终端 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('desktop.terminal.viewTerminal')"
                  @click.stop="viewSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                  </svg>
                </button>
                <!-- 重启 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('desktop.terminal.restartSession')"
                  @click.stop="restartSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                  </svg>
                </button>
                <!-- 停止 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('common.button.stop')"
                  @click.stop="confirmStopSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 6h12v12H6z" />
                  </svg>
                </button>
                <!-- 删除 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-red-600 dark:hover:text-red-400 transition-colors flex-shrink-0"
                  :title="t('common.button.delete')"
                  @click.stop="confirmDeleteSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 创建/编辑对话框 -->
    <Modal v-model="showCreateDialog" :title="editingConfig ? t('desktop.session.editConfig') : t('desktop.session.newConfig')" size="lg">
      <SessionForm
        ref="sessionFormRef"
        :config="editingConfig"
        @save="handleSaveConfig"
      />
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showCreateDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary" @click="submitForm">{{ editingConfig ? t('common.button.save') : t('common.button.create') }}</button>
        </div>
      </template>
    </Modal>

    <!-- 删除配置确认 -->
    <Modal v-model="showDeleteConfirmDialog" :title="t('desktop.session.confirmDelete')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">{{ t('desktop.session.confirmDeleteMsg') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showDeleteConfirmDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmDeleteConfig">{{ t('common.button.delete') }}</button>
        </div>
      </template>
    </Modal>

    <!-- 停止会话确认 -->
    <Modal v-model="showStopConfirmDialog" :title="t('desktop.session.confirmStop')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">{{ t('desktop.session.confirmStopMsg', { name: pendingSession?.name }) }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showStopConfirmDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" :disabled="isOperating" @click="confirmStop">{{ t('common.button.stop') }}</button>
        </div>
      </template>
    </Modal>

    <!-- 删除会话确认 -->
    <Modal v-model="showDeleteSessionConfirmDialog" :title="t('desktop.session.confirmDeleteSession')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('desktop.session.confirmDeleteRunning', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showDeleteSessionConfirmDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" :disabled="isOperating" @click="confirmDeleteSessionNow">{{ t('desktop.session.stopAndDelete') }}</button>
        </div>
      </template>
    </Modal>

    <!-- 操作中遮罩（Teleport：同 Modal 约定，避免父容器 overflow/transform 裁剪） -->
    <Teleport to="body">
      <div v-if="isOperating" class="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-6 py-5 flex items-center gap-3">
          <svg class="w-4 h-4 animate-spin text-[var(--text-secondary)]" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-primary)]">{{ operatingMessage }}</p>
        </div>
      </div>
      <!-- 终端窗口打开中遮罩：窗口就绪（就绪事件或 4s 兜底）后消失 -->
      <div v-if="isTerminalOpening" class="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-6 py-5 flex items-center gap-3">
          <svg class="w-4 h-4 animate-spin text-[var(--text-secondary)]" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-primary)]">{{ t('desktop.terminal.opening') }}</p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话配置视图 — 桌面端会话管理页面
 * Warm Workbench 风格：配置卡片 + 运行中汇总 section；全部操作为真实调用
 */
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import Modal from '@/components/Modal.vue'
import SessionForm from '@/components/SessionForm.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'
import { useToast } from '@/composables/useToast'
import { InvokeTimeoutError } from '@/utils/invoke'
import { useSessionStore, type SessionInfo, type SessionConfig } from '@/stores/session'
import { useSessionWindows } from '@/composables/useSessionWindows'
import { useSessionStatusListener } from '@/composables/useSessionStatusListener'

const sessionStore = useSessionStore()
const { t } = useI18n()
const toast = useToast()
const { openTerminalWindow, closeTerminalWindow, windows } = useSessionWindows()
const { startListening, stopListening } = useSessionStatusListener()

const configs = computed(() => sessionStore.configs)
const sessions = computed(() => sessionStore.sessions)

// 跨配置的「运行中」汇总 tab 数据源
const runningSessions = computed(() => sessions.value.filter(s => s.status !== 'stopped'))

// ==================== Tab 切换：终端配置 / 运行中的会话 ====================
type SessionTabKey = 'configs' | 'running'
const activeTab = ref<SessionTabKey>('configs')
const sessionTabs = computed<{ key: SessionTabKey; label: string }[]>(() => [
  { key: 'configs', label: t('desktop.session.tabConfigs') },
  // 运行中的会话 tab 实时显示数量
  { key: 'running', label: `${t('desktop.session.tabRunningSessions')} (${runningSessions.value.length})` },
])

const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)
const isLoading = ref(true)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)
const sessionFormRef = ref<InstanceType<typeof SessionForm> | null>(null)

// 会话操作对话框状态
const showStopConfirmDialog = ref(false)
const showDeleteSessionConfirmDialog = ref(false)
const pendingSession = ref<SessionInfo | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref(t('desktop.session.processing'))
// 终端窗口打开中的 loading 状态（新建窗口时显示，直到窗口就绪）
const isTerminalOpening = ref(false)

// 每秒刷新一次 now，用于运行时长显示
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

// 监听会话列表变化，自动关闭已停止会话的终端窗口
watch(() => sessionStore.sessions, (newSessions, oldSessions) => {
  if (!oldSessions) return

  for (const oldSession of oldSessions) {
    const newSession = newSessions.find(s => s.id === oldSession.id)

    // 会话从运行中变为停止/错误，关闭终端窗口
    if (oldSession.status === 'running' || oldSession.status === 'waitingInput') {
      if (newSession && (newSession.status === 'stopped' || newSession.status === 'error')) {
        closeTerminalWindow(oldSession.id)
      }
    }

    // 会话被删除
    if (!newSession) {
      closeTerminalWindow(oldSession.id)
    }
  }
}, { deep: true })

// 页面级键盘快捷键
useKeyboardShortcuts([
  { key: 'n', ctrl: true, handler: () => { showCreateDialog.value = true } },
  {
    key: 'Escape',
    handler: () => {
      showCreateDialog.value = false
      showDeleteConfirmDialog.value = false
      showStopConfirmDialog.value = false
      showDeleteSessionConfirmDialog.value = false
    },
    ignoreInput: true,
  },
])

onMounted(async () => {
  isLoading.value = true
  try {
    await sessionStore.loadConfigs()
    await sessionStore.loadSessions()
  } catch (e) {
    console.error('Failed to load data:', e)
  }
  isLoading.value = false

  // 启动会话状态变化监听
  await startListening()

  // 等待 DOM 更新完成
  await nextTick()

  // 输出应用启动耗时
  try {
    const elapsed = await invoke<number>('get_startup_time')
    console.log(`[BedCode] 应用启动耗时: ${elapsed}ms`)
  } catch (e) {
    // 非 Tauri 环境忽略
  }

  // 运行时长每秒刷新
  timer = setInterval(() => { now.value = Date.now() }, 1000)
})

onUnmounted(() => {
  stopListening()
  if (timer) clearInterval(timer)
})

// ==================== 数据辅助 ====================

function sessionsOf(config: SessionConfig): SessionInfo[] {
  return sessions.value.filter(s => (s.configId || s.config_id) === config.id)
}

function runningOf(config: SessionConfig): SessionInfo[] {
  return sessionsOf(config).filter(s => s.status !== 'stopped')
}

function cfgDir(c: SessionConfig): string {
  return c.workingDir || c.working_dir || ''
}

function cfgCommand(c: SessionConfig): string {
  return c.command || ''
}

function cfgAutoStart(c: SessionConfig): boolean {
  return c.autoStart ?? c.auto_start ?? false
}

function isRunningStatus(status: string): boolean {
  return status === 'running' || status === 'waitingInput' || status === 'starting'
}

function statusDot(status: string): string {
  switch (status) {
    case 'running': return 'bg-green-500 animate-pulse'
    case 'waitingInput': return 'bg-amber-500'
    case 'error': return 'bg-red-500'
    default: return 'bg-[var(--text-tertiary)]'
  }
}

function statusText(status: string): string {
  switch (status) {
    case 'starting': return t('common.status.starting')
    case 'running': return t('common.status.running')
    case 'waitingInput': return t('common.status.asking')
    case 'error': return t('common.status.error')
    case 'stopped': return t('common.status.stopped')
    default: return t('common.status.unknown')
  }
}

function runTimeText(session: SessionInfo): string {
  const start = session.startedAt || session.createdAt || session.created_at
  if (!start) return '--'
  const diff = Math.floor((now.value - new Date(start).getTime()) / 1000)
  if (diff < 0) return '--'
  if (diff < 60) return t('common.time.secondsAgo', { n: diff })
  if (diff < 3600) return t('common.time.minutesSecondsAgo', { m: Math.floor(diff / 60), s: diff % 60 })
  return t('common.time.hoursMinutesAgo', { h: Math.floor(diff / 3600), m: Math.floor((diff % 3600) / 60) })
}

function formatDateTime(dateStr: string): string {
  if (!dateStr) return '--'
  return new Date(dateStr).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

// ==================== 操作 ====================

async function refreshSessions() {
  await sessionStore.loadSessions()
  toast.info(t('desktop.session.listRefreshed'))
}

async function startSession(configId: string) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.starting')

  try {
    // 两阶段启动：先创建会话（不启动 PTY），初始化历史缓存，再启动 PTY
    const sessionId = await sessionStore.createSession(configId)
    await sessionStore.startSession(sessionId)
    toast.success(t('desktop.session.sessionStarted'))
  } catch (e: any) {
    console.error('[SessionsConfigView] startSession error:', e)
    if (e instanceof InvokeTimeoutError) {
      toast.error(t('desktop.session.startTimeout'))
    } else {
      toast.error(t('desktop.session.startFailed', { error: e?.message || e }))
    }
  } finally {
    isOperating.value = false
  }
}

function editConfig(config: SessionConfig) {
  editingConfig.value = config
  showCreateDialog.value = true
}

function deleteConfig(configId: string) {
  pendingDeleteConfigId.value = configId
  showDeleteConfirmDialog.value = true
}

async function confirmDeleteConfig() {
  if (!pendingDeleteConfigId.value) return
  await sessionStore.deleteConfig(pendingDeleteConfigId.value)
  toast.success(t('desktop.session.configDeleted'))
  showDeleteConfirmDialog.value = false
  pendingDeleteConfigId.value = null
}

async function viewSession(session: SessionInfo) {
  // 检查会话是否在运行
  if (session.status !== 'running' && session.status !== 'waitingInput') {
    toast.info(t('desktop.session.notRunning'))
    return
  }
  // 已有终端窗口：直接聚焦，无需 loading
  if (windows.value.has(session.id)) {
    openTerminalWindow(session)
    return
  }
  // 新建终端窗口：弹 loading 直到窗口就绪（就绪事件或 4s 兜底）
  isTerminalOpening.value = true
  try {
    await openTerminalWindow(session)
  } catch (e) {
    console.error('[SessionsConfigView] openTerminalWindow error:', e)
    toast.error(t('desktop.terminal.openFailed'))
  } finally {
    isTerminalOpening.value = false
  }
}

function confirmStopSession(session: SessionInfo) {
  pendingSession.value = session
  showStopConfirmDialog.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  isOperating.value = true
  operatingMessage.value = t('desktop.session.stopping')

  try {
    await sessionStore.killSession(sessionId)
    toast.info(t('desktop.session.sessionStopped'))

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error(t('desktop.session.stopFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showStopConfirmDialog.value = false
    pendingSession.value = null
  }
}

async function restartSession(session: SessionInfo) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.restarting')

  try {
    await sessionStore.restartSession(session.id)
    toast.success(t('desktop.session.sessionRestarted'))
  } catch (e) {
    toast.error(t('desktop.session.restartFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
  }
}

function confirmDeleteSession(session: SessionInfo) {
  pendingSession.value = session

  // 运行中的会话提示将先停止再删除，已停止的会话直接删除
  if (session.status !== 'stopped' && session.status !== 'error') {
    showDeleteSessionConfirmDialog.value = true
  } else {
    confirmDeleteSessionNow()
  }
}

async function confirmDeleteSessionNow() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  const isRunning = pendingSession.value.status !== 'stopped' && pendingSession.value.status !== 'error'

  isOperating.value = true
  operatingMessage.value = isRunning ? t('desktop.session.stoppingAndDeleting') : t('desktop.session.deleting')

  try {
    // 运行中的会话先停止
    if (isRunning) {
      await sessionStore.killSession(sessionId)
    }
    // 然后删除
    await sessionStore.deleteSession(sessionId)
    toast.success(t('desktop.session.sessionDeleted'))

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error(t('desktop.session.deleteFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showDeleteSessionConfirmDialog.value = false
    pendingSession.value = null
  }
}

function submitForm() {
  if (sessionFormRef.value) {
    handleSaveConfig(sessionFormRef.value.form)
  }
}

interface SessionFormData {
  name: string
  environment: string
  wslDistro: string
  workingDir: string
  command: string
  autoStart: boolean
}

async function handleSaveConfig(form: SessionFormData) {
  try {
    if (editingConfig.value) {
      await sessionStore.updateConfig(
        editingConfig.value.id,
        form.name,
        form.environment,
        form.workingDir || '',
        form.command || '',
        form.wslDistro || undefined,
        form.autoStart,
      )
      toast.success(t('desktop.session.configUpdated'))
    } else {
      await sessionStore.createConfig(
        form.name,
        form.environment,
        form.workingDir || '',
        form.command || '',
        form.wslDistro || undefined,
      )
      toast.success(t('desktop.session.configCreated'))
    }
    showCreateDialog.value = false
    editingConfig.value = null
  } catch (e: any) {
    console.error('[SessionsConfigView] handleSaveConfig error:', e)
    toast.error(t('desktop.session.saveFailed', { error: e?.message || e }))
  }
}
</script>

<style scoped>
/* Tab 切换过渡：淡入淡出 + 轻微 Y 位移，避免切换闪现 */
.tab-fade-enter-active,
.tab-fade-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.tab-fade-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.tab-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
