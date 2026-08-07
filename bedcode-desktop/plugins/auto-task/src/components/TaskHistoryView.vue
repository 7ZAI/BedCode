<script setup lang="ts">
/**
 * 任务历史视图 — 四 Tab 侧边栏视图
 *
 * Tab1 当前任务：创建新任务（选择运行中会话）+ 执行中的任务列表
 *              + 执行任务（各会话待执行队列）
 * Tab2 任务日志：筛选条（状态/agent/来源/时间范围）+ 分页任务列表 + 行内详情展开
 * Tab3 定时任务：新建表单（会话配置/触发时间/prompts 列表）+ 任务列表 + 删除
 * Tab4 统计：筛选条件下任务统计（状态分布 / 完成数 / 终态数 / 成功率 / 平均耗时）
 *
 * 通过 inject('pluginContext') 获取 PluginContext，
 * 调用 Rust 后端命令查询数据，监听事件实时更新
 */
import { ref, onMounted, onUnmounted, inject, computed, watch } from 'vue'
// 开源 Vue3 日期/时间选择组件（替代原生 datetime-local，样式随宿主主题定制）
import Datepicker from '@vuepic/vue-datepicker'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

const context = inject<PluginContext>('pluginContext')!
// i18n：与 AutoTaskModal 一致，经 context.i18n 自动加插件 ID 前缀
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

// ==================== Types ====================

interface TaskRecord {
  id: string
  description: string | null
  status: string
  agent: string | null
  source: string | null
  session_id: string
  claude_sid: string | null
  working_dir: string | null
  auto_approve: number
  exit_reason: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
  input_tokens: number | null
  output_tokens: number | null
}

interface QueueItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

interface ScheduledJob {
  id: string
  name: string | null
  config_id: string
  trigger_at: string
  prompts: string[]
  status: string
  session_id: string | null
  created_at: string
  executed_at: string | null
  error: string | null
}

interface SessionConfig {
  id: string
  name: string
  workingDir: string
  command: string
}

interface HistoryStats {
  total: number
  by_status: Record<string, number>
  completed: number
  terminal: number
  success_rate: number
  avg_duration_seconds: number
}

/** 运行中的会话（后端 list-running-sessions 返回，queue 为前端按需加载） */
interface RunningSession {
  session_id: string
  name: string
  config_id: string
  working_dir: string
  status: string
  task_status: string
  description: string
  started_at: string | null
  agent: string
  queue_count: number
  queue: QueueItem[]
}

/** 预设任务（无会话/未选会话时创建，一次性消耗） */
interface PresetItem {
  id: string
  prompt: string
  created_at: string
}

// ==================== State ====================

type TabKey = 'current' | 'records' | 'scheduled' | 'stats'
const activeTab = ref<TabKey>('current')

// Tab1 当前任务
const runningSessions = ref<RunningSession[]>([])
const currentLoading = ref(false)
const createSessionId = ref('')
const createPrompt = ref('')
const creatingTask = ref(false)
const createError = ref('')
// Tab1 预设任务（无会话/未选会话时创建，加入队列后自动移除）
const presets = ref<PresetItem[]>([])
const presetError = ref('')

// Tab2 任务记录
const tasks = ref<TaskRecord[]>([])
const total = ref(0)
const limit = 50
const offset = ref(0)
const loading = ref(false)
const stats = ref<HistoryStats | null>(null)
const expandedId = ref<string | null>(null)

const filterStatus = ref('')
const filterAgent = ref('')
const filterSource = ref('')
const filterSince = ref<Date | null>(null)
const filterUntil = ref<Date | null>(null)

// Tab3 定时任务
const jobs = ref<ScheduledJob[]>([])
const jobsLoading = ref(false)
const configs = ref<SessionConfig[]>([])
const showForm = ref(false)
const creatingJob = ref(false)
const formName = ref('')
const formConfigId = ref('')
const formTriggerAt = ref<Date | null>(null)
const formPrompts = ref<string[]>([''])
const errorMessage = ref('')

// 输入/下拉框统一样式（与宿主 TerminalWindowView 的控件保持一致）
const controlCls =
  'w-full h-8 px-2 rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] ' +
  'text-xs text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)] transition-colors duration-200'

// ==================== 日期选择器（@vuepic/vue-datepicker） ====================

// 深色模式跟随宿主（documentElement.dark class，见宿主 useTheme composable），
// MutationObserver 监听宿主主题切换，Datepicker 的 dark prop 随之联动
const isDark = ref(document.documentElement.classList.contains('dark'))
let themeObserver: MutationObserver | null = null

// 输入/回填格式（date-fns token），与筛选条/定时任务的显示习惯一致
const dateFormat = 'yyyy-MM-dd HH:mm'

// 跟随宿主语言（zh-CN / en），供 Datepicker 渲染对应语言的日历与星期/月份文案
const dateLocale = computed(() => context.i18n.getI18n()?.global?.locale?.value ?? 'zh-CN')

// 筛选变化防抖：Datepicker 的 update:model-value 在手动输入时逐字符触发，
// 聚合成一次重载避免高频请求（原生 datetime-local 的 change 语义在提交时触发一次）
let filterDebounce: ReturnType<typeof setTimeout> | null = null

// Date 对象 → UTC "YYYY-MM-DD HH:MM:SS"（与后端 SQLite datetime 同格式）
function dateToUtc(d: Date | null | undefined): string {
  if (!d || isNaN(d.getTime())) return ''
  return d.toISOString().replace('T', ' ').slice(0, 19)
}

function onFilterChangedDebounced() {
  if (filterDebounce) clearTimeout(filterDebounce)
  filterDebounce = setTimeout(onFilterChanged, 300)
}

// ==================== 状态展示 ====================

const statusLabel: Record<string, string> = {
  idle: t('idle'),
  in_progress: t('inProgress'),
  asking: t('asking'),
  completed: t('completed'),
  interrupted: t('interrupted'),
  pending: t('pending'),
}

const statusColor: Record<string, string> = {
  idle: 'text-[var(--text-tertiary)]',
  in_progress: 'text-blue-500',
  asking: 'text-amber-500',
  completed: 'text-green-500',
  interrupted: 'text-red-500',
  pending: 'text-[var(--text-tertiary)]',
}

const statusDot: Record<string, string> = {
  idle: 'bg-[var(--text-tertiary)]',
  in_progress: 'bg-blue-500',
  asking: 'bg-amber-500',
  completed: 'bg-green-500',
  interrupted: 'bg-red-500',
  pending: 'bg-[var(--text-tertiary)]',
}

const scheduledStatusLabel: Record<string, string> = {
  pending: t('pending'),
  creating: t('scheduledStatusCreating'),
  executed: t('scheduledStatusExecuted'),
  failed: t('scheduledStatusFailed'),
  missed: t('scheduledStatusMissed'),
}

function scheduledStatusBadge(status: string): string {
  const base = 'inline-flex items-center h-5 px-2 rounded-full text-[calc(11px*var(--ui-scale))] font-medium'
  const colors: Record<string, string> = {
    pending: 'bg-[var(--bg-hover)] text-[var(--text-secondary)]',
    creating: 'bg-blue-500/10 text-blue-500',
    executed: 'bg-green-500/10 text-green-500',
    failed: 'bg-red-500/10 text-red-500',
    missed: 'bg-amber-500/10 text-amber-500',
  }
  return `${base} ${colors[status] || colors.pending}`
}

const tabs: { key: TabKey; label: string }[] = [
  { key: 'current', label: t('tabsCurrent') },
  { key: 'records', label: t('tabsRecords') },
  { key: 'scheduled', label: t('tabsScheduled') },
  { key: 'stats', label: t('tabsStats') },
]

// ==================== 当前任务（Tab1） ====================

// 执行中的任务（in_progress / asking）
const activeTasks = computed(() =>
  runningSessions.value.filter((s) => ['in_progress', 'asking'].includes(s.task_status)),
)
// 有待执行队列的会话（执行任务区段）
const executingSessions = computed(() => runningSessions.value.filter((s) => s.queue.length > 0))

// 会话标签：名称/工作目录基名/会话短 ID 兜底，附 agent
function sessionLabel(s: RunningSession): string {
  const dir = baseName(s.working_dir)
  const base = s.name || dir || (s.session_id ? s.session_id.slice(0, 8) : '')
  return s.agent && s.agent !== 'unknown' ? `${base} · ${s.agent}` : base
}

async function loadRunningSessions() {
  currentLoading.value = true
  try {
    const result: any = await context.commands.execute('auto-task.list-running-sessions')
    const sessions: RunningSession[] = (result?.sessions ?? []).map((s: any) => ({
      ...s,
      queue: [],
    }))

    // 逐个加载待执行队列（仅当会话存在 pending 项，避免无谓请求）
    await Promise.all(
      sessions.map(async (s) => {
        if ((s.queue_count ?? 0) > 0) {
          try {
            const q: any = await context.commands.execute('auto-task.list-task-queue', {
              session_id: s.session_id,
            })
            s.queue = (q?.tasks as QueueItem[]) ?? []
          } catch (e) {
            console.error('[Auto Task] Failed to load queue for session:', s.session_id, e)
          }
        }
        return s
      }),
    )
    runningSessions.value = sessions

    // 会话列表变化后修正下拉选中项：优先保留原选择，否则取第一个有活动任务/队列的会话
    if (!sessions.some((s) => s.session_id === createSessionId.value)) {
      const preferred =
        sessions.find(
          (s) => s.queue_count > 0 || ['in_progress', 'asking'].includes(s.task_status),
        ) || sessions[0]
      createSessionId.value = preferred?.session_id ?? ''
    }
  } catch (e) {
    console.error('[Auto Task] Failed to load running sessions:', e)
  } finally {
    currentLoading.value = false
  }
}

// 创建新任务：选了运行中的会话 → 加入该会话队列（空闲时立即执行）；
// 未选会话/无运行会话 → 保存为预设任务（终端弹窗或选择会话后可加入队列）
async function createTask() {
  const prompt = createPrompt.value.trim()
  if (!prompt) return
  creatingTask.value = true
  createError.value = ''
  try {
    if (createSessionId.value) {
      await context.commands.execute('auto-task.add-task', {
        session_id: createSessionId.value,
        prompt,
      })
    } else {
      await context.commands.execute('auto-task.create-preset-task', { prompt })
    }
    createPrompt.value = ''
    // 立即刷新展示（事件广播会兜底刷新，这里先给用户即时反馈）
    await loadRunningSessions()
    if (!createSessionId.value) await loadPresets()
  } catch (e) {
    console.error('[Auto Task] Failed to create task:', e)
    createError.value = createSessionId.value ? t('createTaskFailed') : t('createPresetFailed')
  } finally {
    creatingTask.value = false
  }
}

// ==================== 预设任务（Tab1） ====================

async function loadPresets() {
  try {
    const result: any = await context.commands.execute('auto-task.list-preset-tasks')
    presets.value = (result?.presets as PresetItem[]) || []
  } catch (e) {
    console.error('[Auto Task] Failed to load presets:', e)
    presetError.value = t('loadFailed')
  }
}

// 把预设任务加入下拉所选会话的队列（一次性消耗，加入后预设自动移除）
async function addPresetToSession(presetId: string) {
  if (!createSessionId.value) return
  presetError.value = ''
  try {
    await context.commands.execute('auto-task.add-preset-to-queue', {
      session_id: createSessionId.value,
      preset_id: presetId,
    })
    // 事件广播（queue/preset-changed）兜底，此处直接刷新即时反馈
    await Promise.all([loadPresets(), loadRunningSessions()])
  } catch (e) {
    console.error('[Auto Task] Failed to add preset to queue:', e)
    presetError.value = t('addPresetFailed')
  }
}

async function deletePreset(presetId: string) {
  presetError.value = ''
  try {
    await context.commands.execute('auto-task.delete-preset-task', { preset_id: presetId })
    await loadPresets()
  } catch (e) {
    console.error('[Auto Task] Failed to delete preset:', e)
    presetError.value = t('deletePresetFailed')
  }
}

// ==================== 筛选选项与统计（Tab2/Tab4） ====================

const statusOptions = ['', 'idle', 'in_progress', 'asking', 'completed', 'interrupted']
const agentOptions = ['', 'claude', 'codex', 'opencode', 'pi', 'unknown']
const sourceOptions = ['', 'user', 'queue', 'scheduled']

const knownStatuses = ['idle', 'in_progress', 'asking', 'completed', 'interrupted']
const statusStatsList = computed(() =>
  knownStatuses
    .filter((s) => (stats.value?.by_status?.[s] ?? 0) > 0)
    .map((s) => ({ key: s, label: statusLabel[s] || s, count: stats.value!.by_status![s] }))
)

const pageFrom = computed(() => (total.value === 0 ? 0 : offset.value + 1))
const pageTo = computed(() => offset.value + tasks.value.length)
const hasPrev = computed(() => offset.value > 0)
const hasNext = computed(() => offset.value + tasks.value.length < total.value)

// ==================== 数据加载（Tab2 任务记录） ====================

function buildFilter() {
  return {
    status: filterStatus.value || undefined,
    agent: filterAgent.value || undefined,
    source: filterSource.value || undefined,
    since: dateToUtc(filterSince.value) || undefined,
    until: dateToUtc(filterUntil.value) || undefined,
  }
}

async function loadTasks() {
  loading.value = true
  try {
    const result = await context.commands.execute('auto-task.list-task-history', {
      ...buildFilter(),
      limit,
      offset: offset.value,
    })
    tasks.value = result?.tasks ?? []
    total.value = result?.total ?? 0
  } catch (e) {
    console.error('[Auto Task] Failed to load history:', e)
  } finally {
    loading.value = false
  }
}

async function loadStats() {
  try {
    const result = await context.commands.execute('auto-task.task-history-stats', buildFilter())
    stats.value = result ?? null
  } catch (e) {
    console.error('[Auto Task] Failed to load stats:', e)
  }
}

async function refreshRecords() {
  await Promise.all([loadTasks(), loadStats()])
}

// 筛选变化：重置到第一页并重载（统计随筛选刷新）
function onFilterChanged() {
  offset.value = 0
  refreshRecords()
}

// 切到对应 tab 时加载最新数据（事件刷新可能因未挂载而遗漏）
watch(activeTab, (tab) => {
  if (tab === 'current') loadRunningSessions()
  if (tab === 'stats') loadStats()
})

function resetFilters() {
  filterStatus.value = ''
  filterAgent.value = ''
  filterSource.value = ''
  filterSince.value = null
  filterUntil.value = null
  onFilterChanged()
}

function prevPage() {
  if (hasPrev.value) {
    offset.value = Math.max(0, offset.value - limit)
    loadTasks()
  }
}

function nextPage() {
  if (hasNext.value) {
    offset.value += limit
    loadTasks()
  }
}

// 点击行：展开/收起详情
function toggleTask(task: TaskRecord) {
  expandedId.value = expandedId.value === task.id ? null : task.id
}

// ==================== 数据加载（Tab3 定时任务） ====================

async function loadJobs() {
  jobsLoading.value = true
  try {
    const result = await context.commands.execute('auto-task.list-scheduled-jobs')
    jobs.value = (result?.jobs ?? []).map((j: any) => ({
      ...j,
      prompts: parsePrompts(j.prompts),
    }))
  } catch (e) {
    console.error('[Auto Task] Failed to load scheduled jobs:', e)
  } finally {
    jobsLoading.value = false
  }
}

async function loadConfigs() {
  try {
    const result = await context.commands.execute('auto-task.list-session-configs')
    configs.value = result?.configs ?? []
  } catch (e) {
    console.error('[Auto Task] Failed to load session configs:', e)
  }
}

function parsePrompts(raw: string | null): string[] {
  if (!raw) return []
  try {
    const arr = JSON.parse(raw)
    return Array.isArray(arr) ? arr.filter((p): p is string => typeof p === 'string') : []
  } catch {
    return []
  }
}

// workingDir 基名：兼容 Windows 反斜杠路径
function baseName(p: string): string {
  if (!p) return ''
  const parts = p.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || p
}

function configLabel(c: SessionConfig): string {
  const base = baseName(c.workingDir)
  return c.name ? `${c.name} (${base})` : base
}

function addPrompt() {
  formPrompts.value.push('')
}

function removePrompt(idx: number) {
  // 至少保留一个输入框
  if (formPrompts.value.length > 1) {
    formPrompts.value.splice(idx, 1)
  }
}

const utcPreview = computed(() => (formTriggerAt.value ? dateToUtc(formTriggerAt.value) : '-'))

async function submitJob() {
  const prompts = formPrompts.value.map((p) => p.trim()).filter(Boolean)
  if (!formConfigId.value || !formTriggerAt.value || prompts.length === 0) {
    errorMessage.value = t('scheduledFormInvalid')
    return
  }
  creatingJob.value = true
  errorMessage.value = ''
  try {
    const result = await context.commands.execute('auto-task.create-scheduled-job', {
      name: formName.value.trim() || undefined,
      config_id: formConfigId.value,
      trigger_at: dateToUtc(formTriggerAt.value),
      prompts,
    })
    if (result?.job_id) {
      // 成功：收起表单并清空，列表刷新即为反馈
      formName.value = ''
      formConfigId.value = ''
      formTriggerAt.value = null
      formPrompts.value = ['']
      showForm.value = false
      await loadJobs()
    } else {
      errorMessage.value = t('scheduledCreateFailed')
    }
  } catch (e) {
    console.error('[Auto Task] Failed to create scheduled job:', e)
    errorMessage.value = t('scheduledCreateFailed')
  } finally {
    creatingJob.value = false
  }
}

async function deleteJob(jobId: string) {
  errorMessage.value = ''
  try {
    await context.commands.execute('auto-task.delete-scheduled-job', { job_id: jobId })
    await loadJobs()
  } catch (e) {
    console.error('[Auto Task] Failed to delete scheduled job:', e)
    errorMessage.value = t('scheduledDeleteFailed')
  }
}

// ==================== 时间工具 ====================

// 后端时间均为 UTC "YYYY-MM-DD HH:MM:SS"，解析时补 Z 转本地时区显示
function toDate(isoStr: string): Date | null {
  if (!isoStr) return null
  const s = isoStr.includes('T') ? isoStr : isoStr.replace(' ', 'T')
  const hasZone = /[Zz]|[+-]\d{2}:?\d{2}$/.test(s)
  const d = new Date(hasZone ? s : `${s}Z`)
  return isNaN(d.getTime()) ? null : d
}

function formatTime(isoStr: string | null): string {
  if (!isoStr) return '-'
  const d = toDate(isoStr)
  if (!d) return isoStr
  // 跟随宿主当前语言（zh-CN / en），避免硬编码 zh-CN
  const locale = context.i18n.getI18n()?.global?.locale?.value ?? 'zh-CN'
  return d.toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

// 秒 → 可读时长（如 "5分钟 30秒" / "3h 20min"）
function formatDuration(seconds: number | undefined): string {
  if (!seconds || seconds <= 0) return '-'
  const total = Math.round(seconds)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return `${h}${t('durationHours')} ${m}${t('durationMinutes')}`
  if (m > 0) return `${m}${t('durationMinutes')} ${s}${t('durationSeconds')}`
  return `${s}${t('durationSeconds')}`
}

function formatPercent(rate: number | undefined): string {
  if (rate === undefined) return '-'
  return `${(rate * 100).toFixed(1)}%`
}

// ==================== 事件与生命周期 ====================

let statusDisposable: { dispose(): void } | null = null
let queueDisposable: { dispose(): void } | null = null
let scheduledDisposable: { dispose(): void } | null = null
let presetDisposable: { dispose(): void } | null = null

onMounted(async () => {
  await Promise.all([refreshRecords(), loadJobs(), loadConfigs(), loadRunningSessions(), loadPresets()])

  // 监听宿主深色模式切换（documentElement.dark class 变化）
  themeObserver = new MutationObserver(() => {
    isDark.value = document.documentElement.classList.contains('dark')
  })
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })

  // status/queue 变更刷新当前任务 + 任务记录；scheduled 变更刷新定时任务；preset 变更刷新预设区
  statusDisposable = context.events.on('task:status-changed', onLiveChanged)
  queueDisposable = context.events.on('task:queue-changed', onLiveChanged)
  scheduledDisposable = context.events.on('task:scheduled-changed', () => loadJobs())
  presetDisposable = context.events.on('task:preset-changed', () => loadPresets())
})

onUnmounted(() => {
  if (filterDebounce) clearTimeout(filterDebounce)
  themeObserver?.disconnect()
  statusDisposable?.dispose()
  queueDisposable?.dispose()
  scheduledDisposable?.dispose()
  presetDisposable?.dispose()
})

// 任务状态/队列实时变更：任务记录（含统计）与当前任务 Tab 一起刷新
function onLiveChanged() {
  refreshRecords()
  loadRunningSessions()
}
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- Header + Tab 切换 -->
    <div class="px-4 py-3 border-b border-[var(--border)] flex-shrink-0">
      <h2 class="text-sm font-semibold text-[var(--text-primary)] mb-2">{{ t('historyTitle') }}</h2>
      <div class="flex items-center gap-1 p-1 rounded-lg bg-[var(--bg-hover)]">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="flex-1 h-8 rounded-md text-xs font-medium transition-colors duration-200"
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

    <!-- Tab1 当前任务 -->
    <Transition name="tab-fade" mode="out-in">
    <div v-if="activeTab === 'current'" class="flex-1 flex flex-col min-h-0">
      <!-- 滚动内容：创建任务 / 当前任务 / 执行任务 -->
      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-4 min-h-0">
        <!-- 创建新任务 -->
        <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] p-3 space-y-2">
          <h3 class="text-xs font-semibold text-[var(--text-primary)]">{{ t('createTaskTitle') }}</h3>
          <div>
            <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('createTaskSession') }}</label>
            <select v-model="createSessionId" :class="controlCls" :disabled="currentLoading">
              <option value="" disabled>{{ t('createTaskSessionPlaceholder') }}</option>
              <option v-for="s in runningSessions" :key="s.session_id" :value="s.session_id">
                {{ sessionLabel(s) }}
              </option>
            </select>
          </div>
          <div class="flex items-center gap-1.5">
            <input
              v-model="createPrompt"
              type="text"
              :class="controlCls"
              :placeholder="t('createTaskPromptPlaceholder')"
              @keydown.enter="createTask"
            />
            <button
              class="flex-shrink-0 h-8 px-3 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
              :disabled="creatingTask || !createPrompt.trim()"
              @click="createTask"
            >
              {{ createSessionId ? t('createTaskSubmit') : t('saveAsPreset') }}
            </button>
          </div>
          <!-- 未选会话时的去向提示：任务将保存为预设 -->
          <p v-if="!createSessionId" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
            {{ t('createTaskNoSessionHint') }}
          </p>
          <p v-if="createError" class="text-xs text-red-500 break-words">{{ createError }}</p>
        </div>

        <!-- 预设任务（常显：只要存在预设就展示，不随会话选择隐藏） -->
        <div v-if="presets.length > 0" class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] p-3 space-y-2">
          <h3 class="text-xs font-semibold text-[var(--text-primary)]">
            {{ t('presetTitle') }} ({{ presets.length }})
          </h3>
          <div class="space-y-1">
            <div
              v-for="p in presets"
              :key="p.id"
              class="flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-hover)]"
            >
              <span class="text-sm text-[var(--text-primary)] truncate flex-1 min-w-0">{{ p.prompt }}</span>
              <button
                class="inline-flex items-center gap-1 flex-shrink-0 h-7 px-2.5 rounded-[6px] text-xs font-medium transition-opacity duration-200 disabled:opacity-40 disabled:cursor-not-allowed"
                :class="
                  createSessionId
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)] hover:opacity-90'
                    : 'bg-[var(--border)] text-[var(--text-tertiary)]'
                "
                :disabled="!createSessionId"
                :title="createSessionId ? t('addToQueue') : t('presetAddHint')"
                @click="addPresetToSession(p.id)"
              >
                {{ t('addToQueue') }}
              </button>
              <button
                class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:text-red-500 hover:bg-red-500/10 transition-colors duration-200"
                :title="t('delete')"
                @click="deletePreset(p.id)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M3 6h18m-2 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"
                  />
                </svg>
              </button>
            </div>
          </div>
          <p v-if="!createSessionId" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
            {{ t('presetAddHint') }}
          </p>
          <p v-if="presetError" class="text-xs text-red-500 break-words">{{ presetError }}</p>
        </div>

        <!-- 当前任务（执行中的任务） -->
        <div v-if="activeTasks.length > 0">
          <h3 class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2">
            {{ t('currentTaskTitle') }}
          </h3>
          <div class="space-y-1.5">
            <div
              v-for="s in activeTasks"
              :key="s.session_id"
              class="rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 p-3"
            >
              <div class="flex items-center gap-2 mb-1">
                <div class="w-2 h-2 rounded-full animate-pulse flex-shrink-0" :class="statusDot[s.task_status] || 'bg-blue-500'"></div>
                <span class="text-xs font-medium flex-shrink-0" :class="statusColor[s.task_status] || 'text-blue-500'">
                  {{ statusLabel[s.task_status] || s.task_status }}
                </span>
                <span class="text-xs text-[var(--text-tertiary)] truncate flex-1 min-w-0">{{ sessionLabel(s) }}</span>
                <span v-if="s.queue_count > 0" class="text-xs text-[var(--text-secondary)] flex-shrink-0">
                  {{ t('queueCount', { count: s.queue_count }) }}
                </span>
              </div>
              <p class="text-sm text-[var(--text-primary)] break-words">{{ s.description || '-' }}</p>
              <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ formatTime(s.started_at) }}</p>
            </div>
          </div>
        </div>

        <!-- 执行任务（各会话待执行队列） -->
        <div v-if="executingSessions.length > 0">
          <h3 class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2">
            {{ t('executingTaskTitle') }}
          </h3>
          <div class="space-y-2.5">
            <div v-for="s in executingSessions" :key="s.session_id">
              <p class="text-xs text-[var(--text-tertiary)] mb-1">
                {{ sessionLabel(s) }} · {{ t('queueCount', { count: s.queue.length }) }}
              </p>
              <div class="space-y-1">
                <div
                  v-for="item in s.queue"
                  :key="item.id"
                  class="flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-hover)] text-sm"
                >
                  <span class="text-xs text-[var(--text-tertiary)] w-5 text-right flex-shrink-0">#{{ item.position }}</span>
                  <span class="text-[var(--text-primary)] truncate flex-1">{{ item.prompt }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 加载中 -->
        <div v-if="currentLoading" class="flex justify-center py-4">
          <span class="text-sm text-[var(--text-tertiary)]">{{ t('loading') }}</span>
        </div>

        <!-- 空状态：无运行会话且无预设任务时展示（有预设时由预设区替代） -->
        <div
          v-if="!currentLoading && runningSessions.length === 0 && presets.length === 0"
          class="flex flex-col items-center justify-center py-12"
        >
          <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M13 10V3L4 14h7v7l9-11h-7z"
            />
          </svg>
          <p class="text-sm text-[var(--text-tertiary)]">{{ t('noRunningSessions') }}</p>
          <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ t('noRunningSessionsHint') }}</p>
        </div>
      </div>
    </div>

    <!-- Tab2 任务日志 -->
    <div v-else-if="activeTab === 'records'" class="flex-1 flex flex-col min-h-0 max-h-[70vh]">
      <!-- 筛选条 -->
      <div class="px-4 pt-3 flex-shrink-0 space-y-2">
        <div class="grid grid-cols-3 gap-1.5">
          <select v-model="filterStatus" :class="controlCls" @change="onFilterChanged">
            <option value="">{{ t('filterStatus') }}</option>
            <option v-for="s in statusOptions.slice(1)" :key="s" :value="s">{{ statusLabel[s] || s }}</option>
          </select>
          <select v-model="filterAgent" :class="controlCls" @change="onFilterChanged">
            <option value="">{{ t('filterAgent') }}</option>
            <option v-for="a in agentOptions.slice(1)" :key="a" :value="a">{{ a }}</option>
          </select>
          <select v-model="filterSource" :class="controlCls" @change="onFilterChanged">
            <option value="">{{ t('filterSource') }}</option>
            <option v-for="s in sourceOptions.slice(1)" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
        <div class="grid grid-cols-2 gap-1.5">
          <div>
            <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('filterSince') }}</label>
            <Datepicker
              v-model="filterSince"
              :format="dateFormat"
              :locale="dateLocale"
              :dark="isDark"
              :clearable="true"
              :enable-time-picker="true"
              :teleport="'body'"
              :placeholder="t('filterSince')"
              @update:model-value="onFilterChangedDebounced"
            />
          </div>
          <div>
            <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('filterUntil') }}</label>
            <Datepicker
              v-model="filterUntil"
              :format="dateFormat"
              :locale="dateLocale"
              :dark="isDark"
              :clearable="true"
              :enable-time-picker="true"
              :teleport="'body'"
              :placeholder="t('filterUntil')"
              @update:model-value="onFilterChangedDebounced"
            />
          </div>
        </div>
        <div class="flex justify-end">
          <button
            class="text-xs text-[var(--color-primary)] hover:underline transition-colors duration-200"
            @click="resetFilters"
          >
            {{ t('filterReset') }}
          </button>
        </div>
      </div>

      <!-- 滚动内容：分页任务列表 -->
      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-4 min-h-0">
        <!-- 列表加载中 -->
        <div v-if="loading" class="flex justify-center py-4">
          <span class="text-sm text-[var(--text-tertiary)]">{{ t('loading') }}</span>
        </div>

        <!-- 任务列表 -->
        <div v-if="tasks.length > 0">
          <div class="space-y-0.5">
            <div
              v-for="task in tasks"
              :key="task.id"
              class="rounded-md border border-[var(--border)] bg-[var(--bg-card)] cursor-pointer transition-colors duration-200 hover:bg-[var(--bg-hover)]"
              @click="toggleTask(task)"
            >
              <div class="flex items-center gap-2 px-2.5 py-1.5">
                <div class="w-1.5 h-1.5 rounded-full flex-shrink-0" :class="statusDot[task.status] || 'bg-[var(--text-tertiary)]'"></div>
                <p class="flex-1 min-w-0 text-xs text-[var(--text-primary)] truncate">{{ task.description || task.session_id }}</p>
                <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] flex-shrink-0">
                  {{ formatTime(task.started_at || task.created_at) }}
                </span>
                <span class="text-[calc(11px*var(--ui-scale))] flex-shrink-0" :class="statusColor[task.status] || 'text-[var(--text-secondary)]'">
                  {{ statusLabel[task.status] || task.status }}
                </span>
                <svg
                  class="w-3 h-3 text-[var(--text-tertiary)] flex-shrink-0 transition-transform duration-200"
                  :class="{ 'rotate-90': expandedId === task.id }"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </div>

              <!-- 行内详情展开 -->
              <div v-if="expandedId === task.id" class="px-3 pb-2.5 pt-1.5 border-t border-[var(--border)]">
                <div class="grid grid-cols-[auto_1fr] gap-x-2 gap-y-1 text-xs">
                  <span class="text-[var(--text-tertiary)]">{{ t('detailAgent') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ task.agent || '-' }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailSource') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ task.source || '-' }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailCreated') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ formatTime(task.created_at) }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailStarted') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ formatTime(task.started_at) }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailCompleted') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ formatTime(task.completed_at) }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailWorkingDir') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ task.working_dir || '-' }}</span>
                  <span class="text-[var(--text-tertiary)]">{{ t('detailExitReason') }}</span>
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ task.exit_reason || '-' }}</span>
                </div>
                <div class="text-xs mt-1">
                  <span class="text-[var(--text-tertiary)]">{{ t('detailDescription') }}: </span>
                  <span class="text-[var(--text-primary)] whitespace-pre-wrap break-words">{{ task.description || '-' }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 空状态 -->
        <div v-if="!loading && tasks.length === 0" class="flex flex-col items-center justify-center py-12">
          <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
            />
          </svg>
          <p class="text-sm text-[var(--text-tertiary)]">{{ t('emptyHistory') }}</p>
          <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ t('emptyHistoryHint') }}</p>
        </div>
      </div>

      <!-- 分页 -->
      <div v-if="total > 0" class="flex items-center justify-between px-4 py-2 border-t border-[var(--border)] flex-shrink-0">
        <span class="text-xs text-[var(--text-secondary)]">
          {{ t('paginationRange', { from: pageFrom, to: pageTo, total }) }}
        </span>
        <div class="flex items-center gap-1.5">
          <button
            class="h-7 px-2.5 rounded-[6px] text-xs font-medium bg-[var(--bg-hover)] text-[var(--text-primary)] hover:bg-[var(--border)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors duration-200"
            :disabled="!hasPrev"
            @click="prevPage"
          >
            {{ t('paginationPrev') }}
          </button>
          <button
            class="h-7 px-2.5 rounded-[6px] text-xs font-medium bg-[var(--bg-hover)] text-[var(--text-primary)] hover:bg-[var(--border)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors duration-200"
            :disabled="!hasNext"
            @click="nextPage"
          >
            {{ t('paginationNext') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Tab3 定时任务 -->
    <div v-else-if="activeTab === 'scheduled'" class="flex-1 overflow-y-auto px-4 py-3 space-y-3">
      <!-- 新建/收起 -->
      <button
        class="w-full h-8 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90"
        @click="showForm = !showForm"
      >
        {{ showForm ? t('cancel') : t('scheduledNew') }}
      </button>

      <!-- 新建表单 -->
      <div v-if="showForm" class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] p-3 space-y-2.5">
        <div>
          <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledName') }}</label>
          <input v-model="formName" type="text" :class="controlCls" />
        </div>
        <div>
          <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledConfig') }}</label>
          <select v-model="formConfigId" :class="controlCls">
            <option value="" disabled>{{ t('scheduledConfigPlaceholder') }}</option>
            <option v-for="c in configs" :key="c.id" :value="c.id">{{ configLabel(c) }}</option>
          </select>
        </div>
        <div>
          <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledTriggerAt') }}</label>
          <Datepicker
            v-model="formTriggerAt"
            :format="dateFormat"
            :locale="dateLocale"
            :dark="isDark"
            :clearable="true"
            :enable-time-picker="true"
            :teleport="'body'"
            :placeholder="t('scheduledTriggerAt')"
          />
          <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1">
            {{ t('scheduledUtcHint', { time: utcPreview }) }}
          </p>
        </div>
        <div>
          <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledPrompts') }}</label>
          <div class="space-y-1.5">
            <div v-for="(p, idx) in formPrompts" :key="idx" class="flex items-center gap-1.5">
              <input v-model="formPrompts[idx]" type="text" :class="controlCls" :placeholder="t('scheduledPromptPlaceholder')" />
              <button
                v-if="formPrompts.length > 1"
                class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors duration-200"
                :title="t('scheduledRemovePrompt')"
                @click="removePrompt(idx)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>
          <button class="mt-1.5 text-xs text-[var(--color-primary)] hover:underline transition-colors duration-200" @click="addPrompt">
            {{ t('scheduledAddPrompt') }}
          </button>
        </div>
        <button
          class="w-full h-8 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
          :disabled="creatingJob"
          @click="submitJob"
        >
          {{ t('scheduledCreate') }}
        </button>
      </div>

      <!-- 错误提示 -->
      <div
        v-if="errorMessage"
        class="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-500 break-words"
      >
        {{ errorMessage }}
      </div>

      <!-- 空状态 -->
      <div v-if="!jobsLoading && jobs.length === 0" class="flex flex-col items-center justify-center py-12">
        <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <p class="text-sm text-[var(--text-tertiary)]">{{ t('scheduledEmpty') }}</p>
        <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ t('scheduledEmptyHint') }}</p>
      </div>

      <!-- 任务列表（按触发时间升序） -->
      <div v-else class="space-y-2">
        <div v-for="job in jobs" :key="job.id" class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] p-3">
          <div class="flex items-start gap-2">
            <div class="flex-1 min-w-0">
              <p class="text-sm text-[var(--text-primary)] font-medium truncate">{{ job.name || '-' }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-0.5">{{ t('scheduledTriggerAt') }}: {{ formatTime(job.trigger_at) }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-0.5">{{ t('scheduledConfig') }}: {{ job.config_id }}</p>
            </div>
            <span class="flex-shrink-0" :class="scheduledStatusBadge(job.status)">
              {{ scheduledStatusLabel[job.status] || job.status }}
            </span>
          </div>
          <div v-if="job.prompts.length" class="mt-2 space-y-0.5">
            <p v-for="(p, idx) in job.prompts" :key="idx" class="text-xs text-[var(--text-secondary)] truncate">
              {{ idx + 1 }}. {{ p }}
            </p>
          </div>
          <p v-if="job.error" class="text-xs text-red-500 mt-1.5 break-words">{{ t('scheduledError') }}: {{ job.error }}</p>
          <div v-if="job.status === 'pending'" class="flex justify-end mt-2">
            <button
              class="inline-flex items-center gap-1 h-6 px-2 rounded-[6px] text-xs text-red-500 hover:bg-red-500/10 transition-colors duration-200"
              :title="t('delete')"
              @click="deleteJob(job.id)"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                />
              </svg>
              {{ t('delete') }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Tab4 统计 -->
    <div v-else class="flex-1 overflow-y-auto px-4 py-3">
      <!-- 加载中 -->
      <div v-if="!stats" class="flex justify-center py-8">
        <span class="text-sm text-[var(--text-tertiary)]">{{ t('loading') }}</span>
      </div>

      <!-- 统计卡片 -->
      <div v-else class="space-y-3">
        <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3">
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-[var(--text-secondary)]">{{ t('statsTitle') }}</span>
            <span class="text-sm font-semibold text-[var(--text-primary)]">{{ t('statsTotal') }}: {{ stats.total }}</span>
          </div>

          <!-- 状态分布 -->
          <div v-if="statusStatsList.length" class="flex flex-wrap gap-1.5 mt-2">
            <span
              v-for="s in statusStatsList"
              :key="s.key"
              class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[calc(11px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-secondary)]"
            >
              <span class="w-1.5 h-1.5 rounded-full" :class="statusDot[s.key]"></span>
              {{ s.label }} {{ s.count }}
            </span>
          </div>
          <div v-else class="text-xs text-[var(--text-tertiary)] mt-2">-</div>

          <!-- 核心指标 -->
          <div class="grid grid-cols-2 gap-2 mt-3">
            <div class="rounded-md bg-[var(--bg-hover)] px-3 py-2">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ t('statsCompleted') }}</div>
              <div class="text-base font-semibold text-[var(--text-primary)] mt-0.5">{{ stats.completed }}</div>
            </div>
            <div class="rounded-md bg-[var(--bg-hover)] px-3 py-2">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ t('statsTerminal') }}</div>
              <div class="text-base font-semibold text-[var(--text-primary)] mt-0.5">{{ stats.terminal }}</div>
            </div>
            <div class="rounded-md bg-[var(--bg-hover)] px-3 py-2">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ t('statsSuccessRate') }}</div>
              <div class="text-base font-semibold text-[var(--text-primary)] mt-0.5">{{ formatPercent(stats.success_rate) }}</div>
            </div>
            <div class="rounded-md bg-[var(--bg-hover)] px-3 py-2">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ t('statsAvgDuration') }}</div>
              <div class="text-base font-semibold text-[var(--text-primary)] mt-0.5">{{ formatDuration(stats.avg_duration_seconds) }}</div>
            </div>
          </div>
        </div>

        <!-- 无数据提示 -->
        <div v-if="stats.total === 0" class="flex flex-col items-center justify-center py-10">
          <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M11 3.055A9.001 9.001 0 1020.945 13H11V3.055zM20.488 9H15V3.512A9.025 9.025 0 0120.488 9z"
            />
          </svg>
          <p class="text-sm text-[var(--text-tertiary)]">{{ t('emptyHistory') }}</p>
          <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ t('emptyHistoryHint') }}</p>
        </div>
      </div>
    </div>
    </Transition>
  </div>
</template>

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
