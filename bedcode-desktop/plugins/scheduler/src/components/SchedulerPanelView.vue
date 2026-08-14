<script setup lang="ts">
/**
 * 计划任务只读面板（spec §10）
 *
 * 任务列表（name/schedule/enabled/next_at/最近执行摘要）+ 选中任务的最近执行记录
 * （trigger/时间/exit_code）+ 输出文件路径（可复制）。只读，无 CRUD 表单
 * （CRUD 全走 CLI bedtask）。
 *
 * 数据源：list / logs 端点（经 useSchedulerApi → `_http_endpoint` 命令）；
 * 实时刷新：订阅 `scheduler:changed` 事件（Rust 侧 broadcast_changed 三通道
 * 广播），事件到达即重载列表与选中任务的执行记录；初始加载走端点。
 *
 * 状态展示：空态 / 错误态（端点异常）/ enabled=false 徽标 / 状态徽标
 * （succeeded/failed/timeout/missed/waiting/running，色系与 auto-task 面板一致）。
 */
import { ref, onMounted, onUnmounted, inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import { useSchedulerApi, type JobDef, type ExecutionRecord } from '../composables/useSchedulerApi'

const context = inject<PluginContext>('pluginContext')!
// i18n：与宿主 i18n 实例一致（插件 ID 前缀由 SDK 自动添加）
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)
const api = useSchedulerApi(context)

// ==================== 状态 ====================

const jobs = ref<JobDef[]>([])
const loading = ref(false)
const error = ref('')
const selectedId = ref<string | null>(null)
const executions = ref<ExecutionRecord[]>([])
const execLoading = ref(false)
const execError = ref('')
/** 最近复制成功的执行记录 id（按钮短暂显示「已复制」反馈） */
const copiedId = ref<string | null>(null)
/** 最近复制失败的执行记录 id（按钮短暂显示「复制失败」反馈） */
const copyFailedId = ref<string | null>(null)

let changedDisposable: { dispose(): void } | null = null
let copyTimer: ReturnType<typeof setTimeout> | null = null

// ==================== 状态徽标（色系与 auto-task 面板一致） ====================

const badgeBase =
  'inline-flex items-center h-5 px-2 rounded-full text-[calc(11px*var(--ui-scale))] font-medium flex-shrink-0'

const statusColors: Record<string, string> = {
  succeeded: 'bg-green-500/10 text-green-500',
  failed: 'bg-red-500/10 text-red-500',
  timeout: 'bg-orange-500/10 text-orange-500',
  missed: 'bg-amber-500/10 text-amber-500',
  waiting: 'bg-[var(--bg-hover)] text-[var(--text-secondary)]',
  running: 'bg-blue-500/10 text-blue-500',
}

function statusBadge(status: string): string {
  return `${badgeBase} ${statusColors[status] || statusColors.waiting}`
}

const statusLabel: Record<string, string> = {
  succeeded: t('panel.statusSucceeded'),
  failed: t('panel.statusFailed'),
  timeout: t('panel.statusTimeout'),
  missed: t('panel.statusMissed'),
  waiting: t('panel.statusWaiting'),
  running: t('panel.statusRunning'),
}

const triggerLabel: Record<string, string> = {
  cron: t('panel.triggerCron'),
  manual: t('panel.triggerManual'),
}

// ==================== 数据加载 ====================

async function loadJobs() {
  loading.value = true
  error.value = ''
  try {
    jobs.value = await api.listJobs()
  } catch (e) {
    console.error('[Scheduler] Failed to load jobs:', e)
    jobs.value = []
    error.value = t('panel.loadFailed')
  } finally {
    loading.value = false
  }
}

async function loadExecutions(jobId: string) {
  execLoading.value = true
  execError.value = ''
  try {
    executions.value = await api.fetchLogs(jobId)
  } catch (e) {
    console.error(`[Scheduler] Failed to load executions for ${jobId}:`, e)
    executions.value = []
    execError.value = t('panel.loadFailed')
  } finally {
    execLoading.value = false
  }
}

// 点击任务：选中加载执行记录；再次点击收起
function toggleSelect(job: JobDef) {
  if (selectedId.value === job.id) {
    selectedId.value = null
    executions.value = []
    return
  }
  selectedId.value = job.id
  void loadExecutions(job.id)
}

// 手动刷新：列表 +（有选中任务时）选中任务的执行记录
async function refresh() {
  await loadJobs()
  if (selectedId.value) await loadExecutions(selectedId.value)
}

// ==================== 输出路径复制 ====================

async function copyOutput(exec: ExecutionRecord) {
  const text = exec.output_path
  if (!text) return
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
    } else {
      // 兜底：临时 textarea + execCommand（无 Clipboard API 的环境，如部分 webview）
      const ta = document.createElement('textarea')
      ta.value = text
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      ta.remove()
    }
    copiedId.value = exec.exec_id
    copyFailedId.value = null
  } catch (e) {
    console.warn('[Scheduler] Failed to copy output path:', e)
    copyFailedId.value = exec.exec_id
    copiedId.value = null
  }
  if (copyTimer) clearTimeout(copyTimer)
  copyTimer = setTimeout(() => {
    copiedId.value = null
    copyFailedId.value = null
  }, 1500)
}

// ==================== 生命周期 ====================

onMounted(async () => {
  await loadJobs()
  // 实时刷新：CLI 侧变更（add/remove/edit/enable/run）经 broadcast_changed
  // 广播 `scheduler:changed`，事件到达即重载（bus 与 emit_event 双通道，
  // 前端经 Tauri event 桥接收到 emit_event 通道）
  changedDisposable = context.events.on('scheduler:changed', () => {
    void refresh()
  })
})

onUnmounted(() => {
  if (copyTimer) clearTimeout(copyTimer)
  changedDisposable?.dispose()
  changedDisposable = null
})
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)] scheduler-panel">
    <!-- 头部 -->
    <div class="px-4 py-3 border-b border-[var(--border)] flex-shrink-0">
      <div class="flex items-center justify-between gap-2">
        <h2 class="text-sm font-semibold text-[var(--text-primary)]">{{ t('panel.title') }}</h2>
        <button
          type="button"
          class="flex-shrink-0 h-7 px-3 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
          :disabled="loading"
          @click="refresh"
        >
          {{ t('panel.refresh') }}
        </button>
      </div>
      <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1">
        {{ t('panel.readOnlyHint') }}
      </p>
    </div>

    <!-- 错误态（桌面端网关/插件异常） -->
    <div
      v-if="error"
      class="mx-4 mt-3 px-3 py-2 rounded-lg border border-red-500/40 bg-red-500/10 text-red-500 text-xs flex-shrink-0 break-words"
    >
      {{ error }}
    </div>

    <!-- 滚动内容区 -->
    <div class="flex-1 overflow-y-auto min-h-0 px-4 py-3">
      <!-- 加载中（首次） -->
      <div v-if="loading && jobs.length === 0" class="flex justify-center py-4">
        <span class="text-sm text-[var(--text-tertiary)]">{{ t('panel.loading') }}</span>
      </div>

      <!-- 空态 -->
      <div
        v-else-if="jobs.length === 0"
        class="flex flex-col items-center justify-center py-12"
      >
        <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <p class="text-sm text-[var(--text-tertiary)]">{{ t('panel.empty') }}</p>
        <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ t('panel.emptyHint') }}</p>
      </div>

      <!-- 任务列表 -->
      <div v-else class="space-y-1.5">
        <h3
          class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2 flex-shrink-0"
        >
          {{ t('panel.jobsSection') }} ({{ jobs.length }})
        </h3>

        <div
          v-for="job in jobs"
          :key="job.id"
          class="scheduler-job rounded-md border border-[var(--border)] bg-[var(--bg-card)] cursor-pointer transition-colors duration-200 hover:bg-[var(--bg-hover)]"
          @click="toggleSelect(job)"
        >
          <!-- 行首：名称 + 启停徽标 + 最近执行状态徽标 + 展开箭头 -->
          <div class="flex items-center gap-2 min-w-0 px-2.5 py-2">
            <span class="text-sm font-medium text-[var(--text-primary)] truncate flex-1 min-w-0">
              {{ job.name || job.id }}
            </span>
            <span
              class="inline-flex items-center h-5 px-2 rounded-full text-[calc(11px*var(--ui-scale))] font-medium flex-shrink-0"
              :class="
                job.enabled
                  ? 'bg-green-500/10 text-green-500'
                  : 'bg-[var(--bg-hover)] text-[var(--text-tertiary)]'
              "
            >
              {{ job.enabled ? t('panel.enabled') : t('panel.disabled') }}
            </span>
            <span v-if="job.last_status" class="flex-shrink-0" :class="statusBadge(job.last_status)">
              {{ statusLabel[job.last_status] || job.last_status }}
            </span>
            <svg
              class="w-3 h-3 text-[var(--text-tertiary)] flex-shrink-0 transition-transform duration-200"
              :class="{ 'rotate-90': selectedId === job.id }"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </div>

          <!-- cron 表达式 + 元信息 -->
          <div class="px-2.5 pb-2.5 -mt-0.5">
            <p class="text-xs text-[var(--text-secondary)] font-mono truncate">{{ job.schedule }}</p>

            <div class="flex items-center gap-3 mt-1 text-xs text-[var(--text-tertiary)] min-w-0">
              <span class="truncate min-w-0">{{ t('panel.nextAt') }}: {{ job.next_at || '-' }}</span>
              <span class="truncate min-w-0 flex-shrink-0">
                {{ t('panel.lastRun') }}: {{ job.last_finished_at || t('panel.neverRun') }}
              </span>
            </div>
          </div>

          <!-- 选中：最近执行记录（点击内部不触发卡片收起/切换） -->
          <div
            v-if="selectedId === job.id"
            class="mx-2.5 mb-2.5 pt-2 border-t border-[var(--border)]"
            @click.stop
          >
            <h4 class="text-xs font-semibold text-[var(--text-secondary)] mb-2">
              {{ t('panel.executionsSection') }}
            </h4>

            <div v-if="execLoading" class="py-3 text-center">
              <span class="text-xs text-[var(--text-tertiary)]">{{ t('panel.loading') }}</span>
            </div>
            <p v-else-if="execError" class="text-xs text-red-500 break-words">{{ execError }}</p>
            <p v-else-if="executions.length === 0" class="text-xs text-[var(--text-tertiary)]">
              {{ t('panel.noExecutions') }}
            </p>
            <div v-else class="space-y-1.5">
              <div
                v-for="ex in executions"
                :key="ex.exec_id"
                class="scheduler-exec rounded-md bg-[var(--bg-hover)] px-3 py-2"
              >
                <!-- 行首：触发方式 + 状态 + 时间 -->
                <div class="flex items-center gap-2 min-w-0">
                  <span class="inline-flex items-center h-5 px-2 rounded-full bg-[var(--bg-card)] text-[var(--text-secondary)] text-[calc(11px*var(--ui-scale))] font-medium flex-shrink-0">
                    {{ triggerLabel[ex.trigger] || ex.trigger }}
                  </span>
                  <span class="flex-shrink-0" :class="statusBadge(ex.status)">
                    {{ statusLabel[ex.status] || ex.status }}
                  </span>
                  <span class="text-xs text-[var(--text-tertiary)] truncate min-w-0 flex-1 text-right">
                    {{ ex.finished_at || ex.started_at || '-' }}
                  </span>
                </div>
                <p v-if="ex.exit_code !== null" class="text-xs text-[var(--text-tertiary)] mt-1">
                  {{ t('panel.exitCode') }}: {{ ex.exit_code }}
                </p>
                <!-- 输出文件路径（可复制） -->
                <div class="flex items-center gap-2 mt-1 min-w-0">
                  <span
                    v-if="ex.output_path"
                    class="text-xs text-[var(--text-secondary)] font-mono truncate min-w-0 flex-1"
                    :title="ex.output_path"
                  >
                    <span class="text-[var(--text-tertiary)]">{{ t('panel.outputPath') }}:</span>
                    {{ ex.output_path }}
                  </span>
                  <span v-else class="text-xs text-[var(--text-tertiary)] flex-shrink-0">
                    {{ t('panel.noOutput') }}
                  </span>
                  <button
                    v-if="ex.output_path"
                    type="button"
                    class="flex-shrink-0 h-6 px-2 rounded-[6px] border border-[var(--border)] text-[calc(11px*var(--ui-scale))] font-medium transition-colors duration-200"
                    :class="
                      copiedId === ex.exec_id
                        ? 'bg-green-500/10 border-green-500/40 text-green-500'
                        : copyFailedId === ex.exec_id
                          ? 'bg-red-500/10 border-red-500/40 text-red-500'
                          : 'bg-[var(--bg-card)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-strong)]'
                    "
                    :title="ex.output_path"
                    @click="copyOutput(ex)"
                  >
                    {{
                      copiedId === ex.exec_id
                        ? t('panel.copied')
                        : copyFailedId === ex.exec_id
                          ? t('panel.copyFailed')
                          : t('panel.copy')
                    }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <p v-if="!selectedId" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] pt-1">
          {{ t('panel.selectHint') }}
        </p>
      </div>
    </div>
  </div>
</template>
