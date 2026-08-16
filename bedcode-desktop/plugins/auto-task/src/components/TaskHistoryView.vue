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
import { ref, onMounted, onUnmounted, inject, computed, watch, nextTick } from 'vue'
// 开源 Vue3 日期/时间选择组件（替代原生 datetime-local，样式随宿主主题定制）
import Datepicker from '@vuepic/vue-datepicker'
// 宿主共享下拉组件（替代原生 <select>，经 SDK 引用，样式随宿主主题 token）
import Select from '@bedcode/plugin-sdk-desktop/ui'
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
  auto_execute: boolean
  auto_answer: boolean
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
// 会话选择：'' = 预存模式（保存为预存任务）
const createSessionId = ref('')
// 初始默认选择是否已定：仅首次加载时自动选中优先会话，之后一律保留用户选择
let defaultSessionSelected = false
const createPrompt = ref('')
// 创建任务输入框元素引用：提交清空后重置高度为单行
const createPromptEl = ref<HTMLTextAreaElement | null>(null)
const creatingTask = ref(false)
const createError = ref('')
// Tab1 预设任务（无会话/未选会话时创建，加入队列后自动移除）
const presets = ref<PresetItem[]>([])
const presetError = ref('')

// Tab2 任务记录
const tasks = ref<TaskRecord[]>([])
const total = ref(0)
const limit = 10
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
// 历史区段默认折叠：执行档案不占主视图，展开后仍可查看/清理
const finishedCollapsed = ref(true)
// 渲染项：分组头（进行中 / 历史）+ 卡片，单循环内插区段头
interface RenderedJobItem {
  header: boolean
  group: 'active' | 'finished'
  job?: ScheduledJob
}
const configs = ref<SessionConfig[]>([])
const showForm = ref(false)
const creatingJob = ref(false)
const formName = ref('')
const formConfigId = ref('')
const formTriggerAt = ref<Date | null>(null)
// 任务内容：任务卡片列表（一条任务一个卡片，支持逐条添加/删除，与队列弹窗一致）
const formPrompts = ref<string[]>([])
const newPrompt = ref('')
// 添加任务输入框元素引用：添加卡片后重置高度为单行
const newPromptEl = ref<HTMLTextAreaElement | null>(null)
const errorMessage = ref('')
// missed / failed 任务重新设置（重置回 pending，可选改触发时间）
const resettingId = ref<string | null>(null)
const resetTriggerAt = ref<Date | null>(null)

// 输入/下拉框统一样式（与宿主 TerminalWindowView 的控件保持一致）
const controlCls =
  'w-full h-8 px-2 rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] ' +
  'text-xs text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)] transition-colors duration-200'

// ==================== 自动增高 textarea（任务内容输入统一使用） ====================

// 不含固定高度（controlCls 的 h-8）：高度由内容决定，最多 10 行（200px = 10 × 20px 行高）后内部滚动
const textareaCls =
  'w-full px-2 py-1.5 rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] ' +
  'text-xs text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)] transition-colors duration-200 ' +
  'resize-none overflow-y-auto leading-5 max-h-[200px]'

// 10 行上限（与 textareaCls 的 max-h-[200px] 保持一致）
const TEXTAREA_MAX_HEIGHT = 200

// 高度自适应：先置 auto 再取 scrollHeight，钳制到上限后由 overflow-y 滚动
function autosizeTextarea(el: EventTarget | null) {
  const t = el as HTMLTextAreaElement | null
  if (!t) return
  t.style.height = 'auto'
  t.style.height = `${Math.min(t.scrollHeight, TEXTAREA_MAX_HEIGHT)}px`
}

// 任务输入键位统一：Enter 提交（IME 组词中的回车不触发），Shift+Enter 换行
function submitOnEnter(handler: () => void) {
  return (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      handler()
    }
  }
}

// ==================== 日期选择器（@vuepic/vue-datepicker） ====================

// 深色模式跟随宿主（documentElement.dark class，见宿主 useTheme composable），
// MutationObserver 监听宿主主题切换，Datepicker 的 dark prop 随之联动
const isDark = ref(document.documentElement.classList.contains('dark'))
let themeObserver: MutationObserver | null = null

// 输入/回填格式（date-fns token），与筛选条/定时任务的显示习惯一致
const dateFormat = 'yyyy-MM-dd HH:mm'

// 跟随宿主语言（zh-CN / en），供 Datepicker 渲染对应语言的日历与星期/月份文案
const dateLocale = computed(() => context.i18n.getI18n()?.global?.locale?.value ?? 'zh-CN')

// Datepicker 底部操作按钮文本：v9 默认英文（Select/Cancel/Now），不跟随 locale，需按当前语言传入
const dpSelectText = computed(() => t('confirm'))
const dpCancelText = computed(() => t('cancel'))
const dpNowLabel = computed(() => t('datepickerNow'))

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

// 进行中：等待触发 / 正在创建会话；历史：已执行 / 错过 / 失败（终态）
const activeJobs = computed(() => jobs.value.filter((j) => ['pending', 'creating'].includes(j.status)))
const finishedJobs = computed(() => jobs.value.filter((j) => ['executed', 'missed', 'failed'].includes(j.status)))
const executedCount = computed(() => finishedJobs.value.filter((j) => j.status === 'executed').length)

// 分组渲染：进行中在前，历史在后（折叠时不渲染卡片，仅保留区段头）
const renderedJobItems = computed<RenderedJobItem[]>(() => {
  const items: RenderedJobItem[] = []
  if (activeJobs.value.length > 0) {
    items.push({ header: true, group: 'active' })
    items.push(...activeJobs.value.map((job) => ({ header: false, group: 'active' as const, job })))
  }
  if (finishedJobs.value.length > 0) {
    items.push({ header: true, group: 'finished' })
    if (!finishedCollapsed.value) {
      items.push(...finishedJobs.value.map((job) => ({ header: false, group: 'finished' as const, job })))
    }
  }
  return items
})

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

// 会话下拉选项：预存选项永远存在且为默认（''），其后为运行中的会话（仅适配 agent）
const adaptedRunningSessions = computed(() =>
  runningSessions.value.filter((s) => s.is_supported),
)

const sessionOptions = computed(() => [
  { value: '', label: t('saveAsPresetOption') },
  ...adaptedRunningSessions.value.map((s) => ({ value: s.session_id, label: sessionLabel(s) })),
])

async function loadRunningSessions(opts: { silent?: boolean } = {}) {
  // silent：点击下拉触发的刷新不置 loading —— 置 loading 会禁用 select，
  // 原生下拉在禁用瞬间无法展开（表现为“点击只闪加载、下拉不弹出”）
  if (!opts.silent) currentLoading.value = true
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

    // 首次加载：有运行中会话时默认选中优先会话（活动任务/队列优先），无会话则预存；
    // 之后仅修正失效选择（所选会话消失 → 退回预存），不覆盖用户选择
    if (!defaultSessionSelected) {
      defaultSessionSelected = true
      const adapted = sessions.filter((s) => s.is_supported)
      const preferred =
        adapted.find(
          (s) => s.queue_count > 0 || ['in_progress', 'asking'].includes(s.task_status),
        ) || adapted[0]
      createSessionId.value = preferred?.session_id ?? ''
    } else if (
      createSessionId.value &&
      !sessions.some((s) => s.session_id === createSessionId.value)
    ) {
      createSessionId.value = ''
    }
  } catch (e) {
    console.error('[Auto Task] Failed to load running sessions:', e)
  } finally {
    if (!opts.silent) currentLoading.value = false
  }
}

// 点击下拉：静默刷新会话列表（后台更新选项，不打断原生下拉展开）
function onSessionSelectFocus() {
  loadRunningSessions({ silent: true })
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
      // 兜底：检查所选会话的 agent 是否适配（过滤列表理论上已排除，防止竞态/旧选择残留）
      const session = runningSessions.value.find((s) => s.session_id === createSessionId.value)
      if (session && !session.is_supported) {
        createError.value = t('agentNotAdapted')
        return
      }
      await context.commands.execute('auto-task.add-task', {
        session_id: createSessionId.value,
        prompt,
      })
    } else {
      await context.commands.execute('auto-task.create-preset-task', { prompt })
    }
    createPrompt.value = ''
    // 清空后把输入框高度重置回单行
    await nextTick()
    autosizeTextarea(createPromptEl.value)
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

// ==================== 预设任务编辑（行内编辑：输入框 + 保存/取消） ====================

const editingPresetId = ref<string | null>(null)
const editingPresetText = ref('')

// 进入编辑态：预填当前内容，回车保存 / Esc 取消
function startEditPreset(p: PresetItem) {
  editingPresetId.value = p.id
  editingPresetText.value = p.prompt
}

async function saveEditPreset() {
  const prompt = editingPresetText.value.trim()
  if (!editingPresetId.value || !prompt) {
    editingPresetId.value = null
    return
  }
  presetError.value = ''
  try {
    await context.commands.execute('auto-task.update-preset-task', {
      preset_id: editingPresetId.value,
      prompt,
    })
    editingPresetId.value = null
    // 事件广播（preset-changed）兜底，此处直接刷新即时反馈
    await loadPresets()
  } catch (e) {
    console.error('[Auto Task] Failed to update preset:', e)
    presetError.value = t('updateFailed')
  }
}

function cancelEditPreset() {
  editingPresetId.value = null
  editingPresetText.value = ''
}

// ==================== 筛选选项与统计（Tab2/Tab4） ====================

const statusOptions = ['', 'idle', 'in_progress', 'asking', 'completed', 'interrupted']
const agentOptions = ['', 'claude', 'codex', 'opencode', 'pi', 'unknown']
const sourceOptions = ['', 'user', 'queue', 'scheduled']

// 来源固定三种（手动输入/自动任务/定时任务）：预存被消费后归为自动任务，
// 历史遗留的 preset 行同样按自动任务显示，与筛选条件保持一致
const sourceLabel: Record<string, string> = {
  user: t('sourceUser'),
  queue: t('sourceQueue'),
  preset: t('sourceQueue'),
  scheduled: t('sourceScheduled'),
}

// 筛选下拉选项（value 保持内部值，展示中文/可读标签）
const filterStatusOptions = statusOptions
  .slice(1)
  .map((s) => ({ value: s, label: statusLabel[s] || s }))
const filterAgentOptions = agentOptions.slice(1).map((a) => ({ value: a, label: a }))
const filterSourceOptions = sourceOptions
  .slice(1)
  .map((s) => ({ value: s, label: sourceLabel[s] || s }))

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

// ==================== 会话开关（队列卡片上的启动 / 自动应答） ====================

// 切换会话自动执行/自动应答开关
//
// 乐观更新本地状态即时反馈，失败回滚；后端 set-auto-mode 在自动执行
// 由关转开且会话空闲时立即调度队列（try_dispatch_next），由此实现在
// 当前任务页直接触发队列执行。
async function toggleSessionFlag(
  s: RunningSession,
  key: 'auto_execute' | 'auto_answer',
) {
  const prev = s[key]
  s[key] = !prev
  try {
    const result: any = await context.commands.execute('auto-task.set-auto-mode', {
      session_id: s.session_id,
      auto_execute: s.auto_execute,
      auto_answer: s.auto_answer,
    })
    // 以后端返回的合并结果为准（未传字段保持原值，幂等回写避免闪烁）
    s.auto_execute = result?.auto_execute ?? s.auto_execute
    s.auto_answer = result?.auto_answer ?? s.auto_answer
  } catch (e) {
    console.error('[Auto Task] Failed to set session mode:', e)
    s[key] = prev
    errorMessage.value = t('sessionFlagFailed')
  }
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
    const allConfigs: SessionConfig[] = result?.configs ?? []
    // 过滤掉未适配 auto-task 的 agent 的会话配置
    configs.value = allConfigs.filter((c: SessionConfig) => c.is_supported)
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

// 会话配置下拉选项（Tab3 定时任务表单）
const configOptions = computed(() =>
  configs.value.map((c) => ({ value: c.id, label: configLabel(c) })),
)

const utcPreview = computed(() => (formTriggerAt.value ? dateToUtc(formTriggerAt.value) : '-'))

// 重新设置面板的 UTC 预览（与新建表单同款提示）
const resetUtcPreview = computed(() => (resetTriggerAt.value ? dateToUtc(resetTriggerAt.value) : '-'))

// 添加一条任务卡片（回车或点击按钮；空白忽略）
async function addPrompt() {
  const p = newPrompt.value.trim()
  if (!p) return
  formPrompts.value.push(p)
  newPrompt.value = ''
  // 清空后把输入框高度重置回单行
  await nextTick()
  autosizeTextarea(newPromptEl.value)
}

// 删除指定任务卡片
function removePrompt(index: number) {
  formPrompts.value.splice(index, 1)
}

async function submitJob() {
  // 任务内容为卡片列表，创建后按卡片顺序依次执行
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
      formPrompts.value = []
      newPrompt.value = ''
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

// 一键清空执行档案（仅 executed；missed/failed 需用户单独决定重置或删除）
async function clearFinished() {
  const executed = finishedJobs.value.filter((j) => j.status === 'executed')
  if (executed.length === 0) return
  errorMessage.value = ''
  try {
    for (const job of executed) {
      await context.commands.execute('auto-task.delete-scheduled-job', { job_id: job.id })
    }
    await loadJobs()
  } catch (e) {
    console.error('[Auto Task] Failed to clear executed jobs:', e)
    errorMessage.value = t('scheduledDeleteFailed')
  }
}

// ==================== 重新设置（missed / failed → pending） ====================

// 打开重置面板：默认预填当前时间（不改则重置后下一调度周期立即触发执行）
function startReset(job: ScheduledJob) {
  resettingId.value = job.id
  resetTriggerAt.value = new Date()
  errorMessage.value = ''
}

function cancelReset() {
  resettingId.value = null
  resetTriggerAt.value = null
}

async function resetJob(jobId: string) {
  if (!resetTriggerAt.value) {
    errorMessage.value = t('scheduledFormInvalid')
    return
  }
  errorMessage.value = ''
  try {
    await context.commands.execute('auto-task.reset-scheduled-job', {
      job_id: jobId,
      trigger_at: dateToUtc(resetTriggerAt.value),
    })
    // 成功：关闭面板并刷新，任务回到 pending 重新参与调度
    cancelReset()
    await loadJobs()
  } catch (e) {
    console.error('[Auto Task] Failed to reset scheduled job:', e)
    errorMessage.value = t('scheduledResetFailed')
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
  // 跟随宿主当前语言（zh-CN / en），避免硬编码 zh-CN；
  // 数据层时间精确到秒（datetime('now')），展示同步到秒
  const locale = context.i18n.getI18n()?.global?.locale?.value ?? 'zh-CN'
  return d.toLocaleString(locale, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
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
let modeDisposable: { dispose(): void } | null = null

onMounted(async () => {
  await Promise.all([refreshRecords(), loadJobs(), loadConfigs(), loadRunningSessions(), loadPresets()])

  // 监听宿主深色模式切换（documentElement.dark class 变化）
  themeObserver = new MutationObserver(() => {
    isDark.value = document.documentElement.classList.contains('dark')
  })
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })

  // status/queue 变更刷新当前任务 + 任务记录；scheduled 变更刷新定时任务；preset 变更刷新预设区；
  // mode 变更同步会话开关状态（定时任务/弹窗等路径修改后当前页不失步）
  statusDisposable = context.events.on('task:status-changed', onLiveChanged)
  queueDisposable = context.events.on('task:queue-changed', onLiveChanged)
  scheduledDisposable = context.events.on('task:scheduled-changed', () => loadJobs())
  presetDisposable = context.events.on('task:preset-changed', () => loadPresets())
  modeDisposable = context.events.on('session:mode-changed', onModeChanged)
})

// 会话模式变更：同步对应会话的开关状态
function onModeChanged(data: any) {
  const sid = data?.session_id
  if (!sid) return
  const s = runningSessions.value.find((x) => x.session_id === sid)
  if (!s) return
  if (typeof data.auto_execute === 'boolean') s.auto_execute = data.auto_execute
  if (typeof data.auto_answer === 'boolean') s.auto_answer = data.auto_answer
}

onUnmounted(() => {
  if (filterDebounce) clearTimeout(filterDebounce)
  themeObserver?.disconnect()
  statusDisposable?.dispose()
  queueDisposable?.dispose()
  scheduledDisposable?.dispose()
  presetDisposable?.dispose()
  modeDisposable?.dispose()
})

// 任务状态/队列实时变更：任务记录（含统计）与当前任务 Tab 一起刷新
function onLiveChanged() {
  refreshRecords()
  loadRunningSessions()
}
</script>

<template>
  <div class="h-full overflow-hidden flex flex-col bg-[var(--bg-page)]">
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
            <!-- 预存选项永远存在且为默认：即使有运行中的会话，也可不加入队列直接预存 -->
            <Select
              v-model="createSessionId"
              :options="sessionOptions"
              size="sm"
              @open="onSessionSelectFocus"
            />
          </div>
          <div class="flex items-end gap-1.5">
            <textarea
              v-model="createPrompt"
              rows="1"
              :class="textareaCls"
              :ref="(el) => (createPromptEl = el as HTMLTextAreaElement | null)"
              :placeholder="t('createTaskPromptPlaceholder')"
              @input="autosizeTextarea($event.target)"
              @keydown="submitOnEnter(createTask)($event)"
            />
            <button
              class="flex-shrink-0 h-8 px-3 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
              :disabled="creatingTask || !createPrompt.trim()"
              @click="createTask"
            >
              {{ createSessionId ? t('createTaskSubmit') : t('saveAsPreset') }}
            </button>
          </div>
          <!-- 未选会话（预存模式）时的去向提示 -->
          <p v-if="!createSessionId" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
            {{ t('createTaskPresetHint') }}
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
              <!-- 编辑模式：自动增高输入框 + 保存/取消（回车保存，Shift+回车换行，Esc 取消） -->
              <template v-if="editingPresetId === p.id">
                <textarea
                  v-model="editingPresetText"
                  rows="1"
                  :class="textareaCls"
                  :ref="(el) => autosizeTextarea(el)"
                  :placeholder="p.prompt"
                  @input="autosizeTextarea($event.target)"
                  @keydown="submitOnEnter(saveEditPreset)($event)"
                  @keydown.esc="cancelEditPreset"
                />
                <button
                  class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--color-primary)] hover:bg-[var(--color-primary)]/10 transition-colors duration-200"
                  :title="t('save')"
                  @click="saveEditPreset"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M5 13l4 4L19 7"
                    />
                  </svg>
                </button>
                <button
                  class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors duration-200"
                  :title="t('cancel')"
                  @click="cancelEditPreset"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </template>
              <!-- 常规模式：内容 + 加入队列 / 编辑 / 删除 -->
              <template v-else>
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
                  class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors duration-200"
                  :title="t('edit')"
                  @click="startEditPreset(p)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"
                    />
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                  </svg>
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
              </template>
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
              <!-- 队列卡片头部：启动（自动执行）/ 自动应答开关；开启自动执行后队列立即调度 -->
              <div class="flex items-center gap-4 px-3 py-1.5 rounded-md bg-[var(--bg-card)] border border-[var(--border)] mb-1.5">
                <button
                  class="inline-flex items-center gap-1.5"
                  :title="t('autoExecuteHint')"
                  @click="toggleSessionFlag(s, 'auto_execute')"
                >
                  <span
                    class="relative w-8 h-4 rounded-full transition-colors duration-200"
                    :class="s.auto_execute ? 'bg-[var(--color-primary)]' : 'bg-[var(--border-strong)]'"
                  >
                    <span
                      class="absolute top-[2px] w-3 h-3 rounded-full transition-all duration-200"
                      :class="s.auto_execute ? 'left-[18px] bg-[var(--color-primary-contrast)]' : 'left-[2px] bg-[var(--text-tertiary)]'"
                    ></span>
                  </span>
                  <span
                    class="text-xs transition-colors duration-200"
                    :class="s.auto_execute ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'"
                  >
                    {{ t('autoExecute') }}
                  </span>
                </button>
                <button
                  class="inline-flex items-center gap-1.5"
                  :title="t('autoAnswerHint')"
                  @click="toggleSessionFlag(s, 'auto_answer')"
                >
                  <span
                    class="relative w-8 h-4 rounded-full transition-colors duration-200"
                    :class="s.auto_answer ? 'bg-[var(--color-primary)]' : 'bg-[var(--border-strong)]'"
                  >
                    <span
                      class="absolute top-[2px] w-3 h-3 rounded-full transition-all duration-200"
                      :class="s.auto_answer ? 'left-[18px] bg-[var(--color-primary-contrast)]' : 'left-[2px] bg-[var(--text-tertiary)]'"
                    ></span>
                  </span>
                  <span
                    class="text-xs transition-colors duration-200"
                    :class="s.auto_answer ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'"
                  >
                    {{ t('autoAnswer') }}
                  </span>
                </button>
              </div>
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
    <div v-else-if="activeTab === 'records'" class="flex-1 flex flex-col min-h-0">
      <!-- 筛选条 -->
      <div class="px-4 pt-3 flex-shrink-0 space-y-2">
        <div class="grid grid-cols-3 gap-1.5 items-start">
          <Select
            v-model="filterStatus"
            :options="filterStatusOptions"
            size="sm"
            :placeholder="t('filterStatus')"
            @update:model-value="onFilterChanged"
          />
          <Select
            v-model="filterAgent"
            :options="filterAgentOptions"
            size="sm"
            :placeholder="t('filterAgent')"
            @update:model-value="onFilterChanged"
          />
          <Select
            v-model="filterSource"
            :options="filterSourceOptions"
            size="sm"
            :placeholder="t('filterSource')"
            @update:model-value="onFilterChanged"
          />
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
              :select-text="dpSelectText"
              :cancel-text="dpCancelText"
              :now-button-label="dpNowLabel"
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
              :select-text="dpSelectText"
              :cancel-text="dpCancelText"
              :now-button-label="dpNowLabel"
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

      <!-- 任务列表：固定高度区域（filterReset 与分页之间），任务超出时内部滚动；
           分页固定在面板底部不随列表滚动。flex-1 + max-h 保证常规窗口下列表高度固定为 440px，
           窗口过小时自动收缩以保持分页可见（相对容器计算，随 ui-scale 自适应） -->
      <div class="flex-1 min-h-0 max-h-[440px] overflow-y-auto px-4 py-3 space-y-0.5">
        <!-- 列表加载中 -->
        <div v-if="loading" class="flex justify-center py-4">
          <span class="text-sm text-[var(--text-tertiary)]">{{ t('loading') }}</span>
        </div>

        <!-- 任务列表（自然高度卡片，超出滚动；分页固定在底部不随列表滚动） -->
        <div v-if="tasks.length > 0" class="space-y-0.5">
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
                  <span class="text-[var(--text-primary)] truncate min-w-0">{{ sourceLabel[task.source] || task.source || '-' }}</span>
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
          <Select
            v-model="formConfigId"
            :options="configOptions"
            size="sm"
            :placeholder="t('scheduledConfigPlaceholder')"
          />
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
            :select-text="dpSelectText"
            :cancel-text="dpCancelText"
            :now-button-label="dpNowLabel"
            :teleport="'body'"
            :placeholder="t('scheduledTriggerAt')"
          />
          <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1">
            {{ t('scheduledUtcHint', { time: utcPreview }) }}
          </p>
        </div>
        <div>
          <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledPrompts') }}</label>
          <!-- 任务卡片：一条任务一个卡片，支持逐条删除（与队列弹窗交互一致） -->
          <div v-if="formPrompts.length > 0" class="space-y-1 mb-2">
            <div
              v-for="(p, idx) in formPrompts"
              :key="idx"
              class="flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-hover)]"
            >
              <span class="text-xs text-[var(--text-tertiary)] w-5 text-right flex-shrink-0">#{{ idx + 1 }}</span>
              <span class="flex-1 min-w-0 text-sm text-[var(--text-primary)] break-words">{{ p }}</span>
              <button
                class="flex-shrink-0 w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:text-red-500 hover:bg-red-500/10 transition-colors duration-200"
                :title="t('delete')"
                @click="removePrompt(idx)"
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
          <!-- 添加任务：输入后回车或点击按钮生成一张卡片（自动增高，最多 10 行） -->
          <div class="flex items-end gap-1.5">
            <textarea
              v-model="newPrompt"
              rows="1"
              :class="textareaCls"
              :ref="(el) => (newPromptEl = el as HTMLTextAreaElement | null)"
              :placeholder="t('scheduledPromptPlaceholder')"
              @input="autosizeTextarea($event.target)"
              @keydown="submitOnEnter(addPrompt)($event)"
            />
            <button
              class="flex-shrink-0 h-8 px-3 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
              :disabled="!newPrompt.trim()"
              @click="addPrompt"
            >
              {{ t('add') }}
            </button>
          </div>
          <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1">{{ t('scheduledPromptsHint') }}</p>
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

      <!-- 任务列表：进行中在前，历史在后（单循环 + 区段头） -->
      <div v-else class="space-y-2">
        <template v-for="item in renderedJobItems" :key="item.header ? `header-${item.group}` : item.job.id">
          <!-- 区段头：进行中（纯文本）；历史（可折叠 + 一键清空执行档案） -->
          <div v-if="item.header" class="flex items-center justify-between px-1 pt-1">
            <button
              v-if="item.group === 'finished'"
              class="inline-flex items-center gap-1 h-6 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors duration-200"
              @click="finishedCollapsed = !finishedCollapsed"
            >
              <svg
                class="w-3.5 h-3.5 transition-transform duration-200"
                :class="{ 'rotate-90': !finishedCollapsed }"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
              {{ t('scheduledSectionFinished') }} ({{ finishedJobs.length }})
            </button>
            <span v-else class="inline-flex items-center h-6 text-xs font-medium text-[var(--text-secondary)]">
              {{ t('scheduledSectionActive') }} ({{ activeJobs.length }})
            </span>
            <button
              v-if="item.group === 'finished' && executedCount > 0"
              class="inline-flex items-center gap-1 h-6 px-2 rounded-[6px] text-xs text-[var(--text-tertiary)] hover:text-red-500 hover:bg-red-500/10 transition-colors duration-200"
              :title="t('scheduledClearFinished')"
              @click="clearFinished"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                />
              </svg>
              {{ t('scheduledClearFinished') }}
            </button>
          </div>

          <!-- 任务卡片 -->
          <div v-else class="rounded-lg border border-[var(--border)] bg-[var(--bg-card)] p-3">
            <div class="flex items-start gap-2">
              <div class="flex-1 min-w-0">
                <p class="text-sm text-[var(--text-primary)] font-medium truncate">{{ item.job.name || '-' }}</p>
                <p class="text-xs text-[var(--text-secondary)] mt-0.5">{{ t('scheduledTriggerAt') }}: {{ formatTime(item.job.trigger_at) }}</p>
                <p class="text-xs text-[var(--text-secondary)] mt-0.5">{{ t('scheduledConfig') }}: {{ item.job.config_id }}</p>
              </div>
              <span class="flex-shrink-0" :class="scheduledStatusBadge(item.job.status)">
                {{ scheduledStatusLabel[item.job.status] || item.job.status }}
              </span>
            </div>
            <div v-if="item.job.prompts.length" class="mt-2 space-y-0.5">
              <p v-for="(p, idx) in item.job.prompts" :key="idx" class="text-xs text-[var(--text-secondary)] truncate">
                {{ idx + 1 }}. {{ p }}
              </p>
            </div>
            <p v-if="item.job.error" class="text-xs text-red-500 mt-1.5 break-words">{{ t('scheduledError') }}: {{ item.job.error }}</p>

            <!-- 操作区：pending 可删除；missed/failed 可删除或重新设置（重置回 pending 重新调度）；executed 可删除（清档） -->
            <div v-if="['pending', 'missed', 'failed', 'executed'].includes(item.job.status)" class="flex justify-end gap-1 mt-2">
              <button
                v-if="item.job.status !== 'pending' && item.job.status !== 'executed'"
                class="inline-flex items-center gap-1 h-6 px-2 rounded-[6px] text-xs text-[var(--color-primary)] hover:bg-[var(--color-primary)]/10 transition-colors duration-200"
                :title="t('scheduledReset')"
                @click="startReset(item.job)"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M4 4v5h5M20 20v-5h-5M4.1 9a8 8 0 0115.4-1M19.9 15a8 8 0 01-15.4 1"
                  />
                </svg>
                {{ t('scheduledReset') }}
              </button>
              <button
                class="inline-flex items-center gap-1 h-6 px-2 rounded-[6px] text-xs text-red-500 hover:bg-red-500/10 transition-colors duration-200"
                :title="t('delete')"
                @click="deleteJob(item.job.id)"
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

            <!-- 重新设置面板：选择新触发时间（默认当前时间），确认后回到 pending 重新调度 -->
            <div v-if="resettingId === item.job.id" class="mt-2 rounded-lg border border-[var(--border)] bg-[var(--bg-hover)] p-2.5 space-y-2">
              <div>
                <label class="block text-xs text-[var(--text-secondary)] mb-1">{{ t('scheduledTriggerAt') }}</label>
                <Datepicker
                  v-model="resetTriggerAt"
                  :format="dateFormat"
                  :locale="dateLocale"
                  :dark="isDark"
                  :clearable="false"
                  :enable-time-picker="true"
                  :select-text="dpSelectText"
                  :cancel-text="dpCancelText"
                  :now-button-label="dpNowLabel"
                  :teleport="'body'"
                  :placeholder="t('scheduledTriggerAt')"
                />
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1">
                  {{ t('scheduledResetHint') }}
                  {{ t('scheduledUtcHint', { time: resetUtcPreview }) }}
                </p>
              </div>
              <div class="flex gap-1.5">
                <button
                  class="flex-1 h-7 rounded-[6px] bg-[var(--color-primary)] text-[var(--color-primary-contrast)] text-xs font-medium transition-opacity duration-200 hover:opacity-90"
                  @click="resetJob(item.job.id)"
                >
                  {{ t('confirm') }}
                </button>
                <button
                  class="flex-1 h-7 rounded-[6px] border border-[var(--border)] text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors duration-200"
                  @click="cancelReset"
                >
                  {{ t('cancel') }}
                </button>
              </div>
            </div>
          </div>
        </template>
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
/*
 * Tab 切换过渡：out-in 模式。
 * 旧面板快速淡出上移，新面板随后从下方滑入插入（slide-up + fade + 轻微缩放），
 * 缓出曲线带出顺畅感，避免内容直接闪现的生硬切换。
 */
.tab-fade-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.tab-fade-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
.tab-fade-enter-active {
  transition:
    opacity 0.26s cubic-bezier(0.22, 0.61, 0.36, 1),
    transform 0.26s cubic-bezier(0.22, 0.61, 0.36, 1);
  will-change: opacity, transform;
}
.tab-fade-enter-from {
  opacity: 0;
  transform: translateY(14px) scale(0.985);
}
</style>
