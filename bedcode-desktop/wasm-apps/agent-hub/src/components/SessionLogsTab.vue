<script setup lang="ts">
/**
 * 会话日志分区（票 06 + 改版）—— 查询表格 + 二级详情
 *
 * 一级页：日志来源（折叠区：内置/自定义目录 + 扫描 + 添加目录）→ 多条件查询
 * （Agent / 关键词 / 时间范围）→ 分页表格（点击行进二级详情）。
 * 二级页：会话详情头卡 + 页签切换（任务记录 / 原始 JSONL），任务记录为只读
 * 对话样式（类 zcode/trae，无输入框）。
 *
 * 数据流：列表分页与统计明细共用 useUsage 同一查询域；打开会话经
 * read-usage-session 按需解析（单会话一次读盘双消费）。
 */
import { computed, inject, ref, watch, onBeforeUnmount } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'
import Datepicker from '@vuepic/vue-datepicker'
import type { NormalizedEventView, UsageSource } from '../types'
import type { UseUsageReturn } from '../composables/useUsage'
import {
  abbreviateProject,
  COLLAPSE_THRESHOLD_CHARS,
  COLLAPSE_THRESHOLD_TOOL_CHARS,
  formatDuration,
  formatEventTime,
  formatSessionTime,
  formatTokens,
  looksTruncated,
  RAW_LINE_COLLAPSE_CHARS,
  splitToolText,
} from '../utils/format'
import { markdownPlainPreview, renderMarkdown } from '../utils/markdown'
import { suggestSourceName, isPathRegistered } from '../utils/sources'
import AgentIcon from './AgentIcon.vue'

const props = defineProps<{ usage: UseUsageReturn }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const usage = props.usage
const state = computed(() => usage.state.value)
const home = computed(() => state.value?.home ?? '')
const authRequired = computed(() => state.value?.status === 'auth-required')
const syncing = computed(() => state.value?.status === 'syncing')
/**
 * 扫描失败提示（ADR 0030：guest 只落机器可读 code，原文不外泄）
 *
 * 失败必须**看得见**——2026-09-28 实机：扫描在途卡死（回调丢失）时按钮停在
 * 「扫描中…」，用户既看不到失败也点不动。已登记 code 逐条映射，未登记的
 * （含旧版本遗留的原文串）统一走泛化文案，不拿原文渲染。
 */
const SCAN_ERROR_TEXT: Record<string, string> = {
  'scan-timeout': 'hub.lg.sources.scanTimeout',
  'scan-output-unreadable': 'hub.lg.sources.scanUnreadable',
  'scan-interrupted': 'hub.lg.sources.scanInterrupted',
  'scan-failed': 'hub.lg.sources.scanFailed',
}
const scanError = computed(() => {
  const code = state.value?.error
  if (!code || state.value?.status !== 'error') return ''
  return t(SCAN_ERROR_TEXT[code] ?? 'hub.lg.sources.scanFailed')
})
const sessions = computed(() => usage.logSessions.value)
const sources = computed(() => usage.sources.value)
const sessionsTotal = computed(() => usage.logTotal.value)
const totalPages = computed(() => usage.logTotalPages.value)
const page = computed(() => usage.logPage.value)
const loadingSessions = computed(() => usage.logLoading.value)

/** 当前打开会话（二级详情）；页签与折叠展开态在打开会话时重置（见下方折叠区块） */
const opened = computed(() => usage.openedSession.value)
const detailTab = ref<'chat' | 'raw'>('chat')

// ==================== 来源区（默认折叠） ====================
const sourcesOpen = ref(false)
const addingSource = ref(false)
const newSourceName = ref('')
const newSourcePath = ref('')
const sourceBusy = ref(false)
const sourceError = ref('')
const removingSource = ref('')

/**
 * 「自定义新名称」下拉哨兵值（选它 → 转入手输新建来源）
 *
 * 取 `__new__`：来源名合法性（guest `is_valid_source_name`）要求小写字母
 * 开头 + 字母/数字/连字符，故带下划线的哨兵永不可能与真实来源名相撞。
 */
const CUSTOM_SOURCE = '__new__'

/** 可选为目标的来源（仅目录型 jsonl 源——sqlite 单库不接受追加目录） */
const addableSources = computed(() => sources.value.filter((s) => sourceKind(s) === 'jsonl'))

/**
 * 来源名下拉选项：已有来源名（选它 → 目录追加到该来源下）+ 「自定义」哨兵。
 * label 复用来源名本身；自定义行的文案在 i18n 里（带 ✚ 前缀表新建）。
 */
const sourceNameOptions = computed(() => [
  ...addableSources.value.map((s) => ({ value: s.name, label: s.name })),
  { value: CUSTOM_SOURCE, label: t('hub.lg.sources.nameCustom') },
])

/** 当前名称命中的已有来源（null = 自定义新来源）——决定提交走新建还是追加 */
const matchedSource = computed(
  () => addableSources.value.find((s) => s.name === newSourceName.value.trim()) ?? null,
)

/**
 * 已选目录是否已登记在任一来源下（重复）——选中即比对，就地拦下不走到提交。
 * guest 同样以 `error.pathTaken` 最终仲裁；此处只是更早的 UX 反馈。
 */
const pathDuplicate = computed(
  () => !!newSourcePath.value.trim() && isPathRegistered(sources.value, newSourcePath.value.trim()),
)

/** 提交禁用：未选目录 / 未定名 / 目录重复（重复时给出就地错误提示） */
const addDisabled = computed(
  () => !newSourceName.value.trim() || !newSourcePath.value.trim() || pathDuplicate.value,
)

/**
 * 名称下拉的回显/写入（与手输同一个真源 `newSourceName`）
 *
 * 名称命中已有来源 → 回显该来源名；否则回显「自定义」哨兵。写回时选中来源名
 * 直接填进 `newSourceName`（提交即追加到该来源），选哨兵则清空交给用户手输新名。
 */
const sourceNamePick = computed({
  get: () => (matchedSource.value ? matchedSource.value.name : CUSTOM_SOURCE),
  set: (v: string | number) => {
    if (String(v) === CUSTOM_SOURCE) newSourceName.value = ''
    else newSourceName.value = String(v)
  },
})

/** 来源形态（jsonl 目录 / sqlite 库）；旧状态无 kind 字段按 jsonl 处理 */
function sourceKind(s: UsageSource): 'jsonl' | 'sqlite' {
  return s.kind === 'sqlite' ? 'sqlite' : 'jsonl'
}

/**
 * 从选中目录派生来源名（与 guest `is_valid_source_name` 同口径：小写字母开头，
 * 字母/数字/连字符，≤32）：取 basename → 小写 → 去非法字符 → 剥前导非字母 → 截断。
 * 全部剥空时兜底 `logs`。实现见 `utils/sources.ts`（SFC 不可导出，供测试直引）。
 */

/** 系统文件夹选择器选日志目录（fs:pick）；选中后回填路径 + 派生名 */
async function onPickDir() {
  if (sourceBusy.value) return
  sourceBusy.value = true
  sourceError.value = ''
  const r = await usage.pickSourceDir()
  sourceBusy.value = false
  if (!r.ok) {
    sourceError.value = t('hub.lg.sources.pickFailed')
    return
  }
  if (!r.picked) return // 用户取消：不打扰
  newSourcePath.value = r.path
  if (!newSourceName.value.trim()) newSourceName.value = suggestSourceName(r.path)
}

/** 来源的全部目录（wire `paths: [{path, removable}]`）；旧 guest 单 `path` 字段
 * 兜底（内置条目按不可移除处理，与 guest 迁移口径一致）。
 */
function sourcePaths(s: UsageSource): { path: string; removable: boolean }[] {
  if (Array.isArray(s.paths) && s.paths.length > 0) return s.paths
  const legacy = (s as unknown as { path?: string }).path
  if (typeof legacy === 'string' && legacy) {
    return [{ path: legacy, removable: !s.builtin }]
  }
  return []
}

/**
 * 来源扫描计数摘要（未扫描过显示提示）
 *
 * **口径按 kind 分**（票 07）：JSONL 源的解析单位是文件（`parsed + skipped`
 * 为本轮扫过的文件数），SQLite 源没有「文件」概念、解析单位是会话——两者
 * 用同一模板会写出「54 文件 · 4 会话」这种自相矛盾的话。
 */
function scanCount(s: UsageSource): string {
  const sc = s.scan
  if (!sc) return t('hub.lg.sources.noScan')
  if (sourceKind(s) === 'sqlite') {
    return `${sc.parsed} ${t('hub.lg.sources.sessions')}`
  }
  return `${sc.parsed + sc.skipped} ${t('hub.lg.sources.files')} · ${sc.sessions} ${t('hub.lg.sources.sessions')}`
}

/**
 * 提交添加：名称命中已有来源 → 目录追加到该来源下；否则 → 新建来源
 *
 * 一份表单覆盖两种动作（用户原话「选择同一个名称如 pi 则将目录添加到 pi
 * 来源名下」）：走哪条路由只看**最终名称是否已存在**，不区分它是下拉选的
 * 还是手输打对的。目录重复在前端已拦（`pathDuplicate`），这里再兜一层，
 * 避免任何绕过路径把重复目录送到 guest。
 */
async function onAddSource() {
  if (addDisabled.value) return
  const name = newSourceName.value.trim()
  const path = newSourcePath.value.trim()
  sourceBusy.value = true
  sourceError.value = ''
  const r = matchedSource.value
    ? await usage.addSourcePath(name, path)
    : await usage.addSource(name, path)
  sourceBusy.value = false
  if (r.ok) {
    addingSource.value = false
    newSourceName.value = ''
    newSourcePath.value = ''
  } else {
    sourceError.value = r.error ?? t('hub.lg.sources.addFailed')
  }
}

function cancelAddSource() {
  addingSource.value = false
  newSourceName.value = ''
  newSourcePath.value = ''
  sourceError.value = ''
}

async function onRemoveSource(name: string) {
  removingSource.value = name
  sourceError.value = ''
  const r = await usage.removeSource(name)
  removingSource.value = ''
  if (!r.ok) sourceError.value = r.error ?? t('hub.lg.sources.removeFailed')
}

// ==================== 每来源多目录：给来源追加 / 从来源移除目录 ====================
/** 正在添加目录的来源名（null = 无；按钮同时充当开关） */
const addingPathSource = ref<string | null>(null)
/** 该来源新增目录的选中路径（fs:pick 回填） */
const newPathForSource = ref('')
/** 正在移除的目录（name + path 复合键，防重入） */
const removingPath = ref('')

/** 给来源选目录（fs:pick，选择器与来源无关）；取消不打扰，失败显示友好错误 */
async function onPickDirForSource() {
  if (sourceBusy.value) return
  sourceBusy.value = true
  sourceError.value = ''
  const r = await usage.pickSourceDir()
  sourceBusy.value = false
  if (!r.ok) {
    sourceError.value = t('hub.lg.sources.pickFailed')
    return
  }
  if (!r.picked) return
  newPathForSource.value = r.path
}

/** 确认：给来源追加目录（guest 校验路径全局唯一 / 非 sqlite） */
async function onAddSourcePath(name: string) {
  if (!newPathForSource.value.trim()) return
  sourceBusy.value = true
  sourceError.value = ''
  const r = await usage.addSourcePath(name, newPathForSource.value.trim())
  sourceBusy.value = false
  if (r.ok) {
    addingPathSource.value = null
    newPathForSource.value = ''
  } else {
    sourceError.value = r.error ?? t('hub.lg.sources.addPathFailed')
  }
}

/** 确认：从来源移除目录（内置默认路径 / 最后一条路径由 guest 拒绝） */
async function onRemoveSourcePath(name: string, path: string) {
  removingPath.value = `${name}\u0000${path}`
  sourceError.value = ''
  const r = await usage.removeSourcePath(name, path)
  removingPath.value = ''
  if (!r.ok) sourceError.value = r.error ?? t('hub.lg.sources.removePathFailed')
}

function cancelAddPath() {
  addingPathSource.value = null
  newPathForSource.value = ''
  sourceError.value = ''
}

// ==================== 查询条件（Agent / 关键词 / 时间范围） ====================
// 本地输入初值从共享查询域回填：agent/keyword 原本就对齐了，日期两个框
// 此前初值恒为 null —— 设过时间范围后切到统计 tab 再回来，框里是空的而
// 列表仍被过滤，点「查询」就把日期条件静默清掉了。
const filterAgent = ref(usage.listFilter.value)
const keyword = ref(usage.searchText.value)
const fromInput = ref<Date | null>(usage.rangeFrom.value ? new Date(usage.rangeFrom.value) : null)
const toInput = ref<Date | null>(usage.rangeTo.value ? new Date(usage.rangeTo.value) : null)

/**
 * 日期选择器（@vuepic/vue-datepicker，与 auto-task 同款）：
 * 深色模式跟随宿主（documentElement.dark class），MutationObserver 监听主题切换联动 dark prop
 */
const isDark = ref(document.documentElement.classList.contains('dark'))
let themeObserver: MutationObserver | null = null

/** 输入/回填格式（date-fns token），与表格「开始时间」列显示习惯一致 */
const dateFormat = 'yyyy-MM-dd HH:mm'

/** 跟随宿主语言（zh-CN / en），渲染对应语言的日历与星期/月份文案 */
const dateLocale = computed(() => context.i18n.getI18n()?.global?.locale?.value ?? 'zh-CN')

/** Datepicker 底部操作按钮文本：v9 默认英文（Select/Cancel/Now），不跟随 locale，需按当前语言传入 */
const dpSelectText = computed(() => t('hub.lg.filter.dpSelect'))
const dpCancelText = computed(() => t('hub.lg.filter.dpCancel'))
const dpNowLabel = computed(() => t('hub.lg.filter.dpNow'))

themeObserver = new MutationObserver(() => {
  isDark.value = document.documentElement.classList.contains('dark')
})
themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
onBeforeUnmount(() => themeObserver?.disconnect())

/** Agent 下拉选项（SDK Select，与宿主表单控件同源） */
const agentOptions = computed(() => [
  { value: '', label: t('hub.lg.filter.all') },
  ...sources.value.map((s) => ({ value: s.name, label: s.name })),
])

/** 执行查询：提交条件 → 回第 1 页 */
async function applyQuery() {
  usage.listFilter.value = filterAgent.value
  usage.searchText.value = keyword.value
  usage.rangeFrom.value = fromInput.value?.getTime() ?? null
  usage.rangeTo.value = toInput.value?.getTime() ?? null
  await usage.reloadSessions()
}

/** 重置全部条件 → 回第 1 页 */
async function applyReset() {
  filterAgent.value = ''
  keyword.value = ''
  fromInput.value = null
  toInput.value = null
  await usage.resetQuery()
}

function openDetail(id: number) {
  void usage.openSession(id)
}

/** 助手事件 token 明细（↑ 输入 ↓ 输出 ⚡ 缓存读 +缓存写） */
function tokenMeta(e: NormalizedEventView): string {
  if (!e.tokens) return ''
  return [
    `↑ ${formatTokens(e.tokens.input)}`,
    `↓ ${formatTokens(e.tokens.output)}`,
    `⚡ ${formatTokens(e.tokens.cacheRead)}`,
    `+${formatTokens(e.tokens.cacheWrite)}`,
    e.tokens.reasoning > 0 ? `◈ ${formatTokens(e.tokens.reasoning)}` : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

// ==================== 聊天行模型（折叠 / 工具卡 / markdown / 截断提示） ====================
/** 已展开的事件下标（空 = 全部收起，即默认折叠态） */
// 展开态行号集合：用 Set 而非数组 —— chatRows 是整体 computed，切换任一行会让
// 全部行重算，数组的 includes()/filter() 在 MAX_EVENTS=5000 量级退化为 O(n²)
const expandedIndexes = ref<Set<number>>(new Set())

/**
 * 「原始 JSONL」展开行号集合（粒度从「消息」降到「行」，理由见
 * `RAW_LINE_COLLAPSE_CHARS`）
 *
 * 独立于 `expandedIndexes`（聊天行）：两个视图的展开态互不干扰，切页签回来
 * 不该丢；同时由上面那个 `watch(opened)` 一起重置，避免换会话后残留旧下标。
 */
const expandedRawIndexes = ref<Set<number>>(new Set())

/**
 * 原始行视图的行模型（一次算全：预览文本 + 是否可折叠）
 *
 * `raw` 是 guest 直接给的整文件行，单行可达数十 KB（codex `session_meta` 的
 * `base_instructions` 就是整份系统提示词）。不逐行折叠的话首行会把整屏占满，
 * 用户实际看到的是「打开页签 = 一堵 JSON 墙」。折叠态**只渲染截断预览**
 * （展开才铺全文），保证上千行时 DOM 也轻。
 */
const rawRows = computed(() => {
  const lines = opened.value?.raw ?? []
  return lines.map((line, i) => {
    const long = line.length > RAW_LINE_COLLAPSE_CHARS
    const expanded = long && expandedRawIndexes.value.has(i)
    return {
      i,
      long,
      expanded,
      // **只有真被折叠的超阈值行才挂折叠类**：短行展开与否都该原样铺开，
      // 误挂会被 `nowrap + ellipsis` 在视口宽度处截掉（本行并不长）
      collapsed: long && !expanded,
      text: expanded ? line : line.slice(0, RAW_LINE_COLLAPSE_CHARS),
    }
  })
})

/** 切换某一行原始行的展开态（整体替换而非原地 mutate，与 `toggleExpand` 同理） */
function toggleRawExpand(i: number) {
  const next = new Set(expandedRawIndexes.value)
  if (next.has(i)) {
    next.delete(i)
  } else {
    next.add(i)
  }
  expandedRawIndexes.value = next
}

watch(opened, () => {
  detailTab.value = 'chat'
  expandedIndexes.value = new Set()
  expandedRawIndexes.value = new Set()
})

/**
 * 聊天视图的行模型（一次算全：工具卡切片 + 折叠阈值 + markdown 正文 + 截断提示）
 *
 * 四条展示规则都落在这里，模板只做渲染：
 * - **折叠**：助手正文按 500 字、工具卡身按 400 字（工具输出上限 1000，见 format.ts）；
 *   超阈值默认收起，切换会话由上方 watch 重置。
 * - **markdown**：只渲染助手正文（用户行是「我说的话」、工具行是命令原文，都不该排版）。
 *   折叠态给剥标记的纯文本——CSS line-clamp 只对纯文本可靠（块级子元素上
 *   `-webkit-line-clamp` 会数不准行），展开态才产出结构化 HTML。
 * - **截断提示**：guest 在上限处补省略号收尾，这里据此提示「完整原文见原始 JSONL」。
 * - **空泡丢弃**：pi 每一轮都会写一条 `content: []` 的空助手消息（带空 usage 对象），
 *   实测单会话 58/83 条——渲染出来就是一串「助手 / 模型 / ↑0 ↓0 ⚡0 +0」的噪音气泡。
 *   正文空且五项 token 全为 0 的助手行整条不渲染；零 token 行本身也不再出
 *   （token 明细只在真有量时才有信息，“0 0 0 0” 只会让人以为统计坏了）。
 */
const chatRows = computed(() => {
  const events = opened.value?.events ?? []
  return events
    .map((e, i) => {
      // 传入 t：工具卡身里的非文本块占位 token（`[non-text:image]`）按当前语言本地化
      const card = e.role === 'tool' ? splitToolText(e.text, t) : null
      const collapsible =
        e.role === 'assistant'
          ? e.text.length > COLLAPSE_THRESHOLD_CHARS
          : card
            ? card.body.length > COLLAPSE_THRESHOLD_TOOL_CHARS
            : false
      const collapsed = collapsible && !expandedIndexes.value.has(i)
      const isMd = e.role === 'assistant'
      const bodyText = isMd
        ? collapsed
          ? markdownPlainPreview(e.text)
          : renderMarkdown(e.text)
        : card
          ? card.body
          : e.text
      const truncated = looksTruncated(e.text)
      const tokens = e.tokens
      const hasTokens =
        !!tokens &&
        (tokens.input + tokens.output + tokens.cacheRead + tokens.cacheWrite + tokens.reasoning) > 0
      // 行存亡只看**原始文本 + token**：折叠预览是展示派生物，仅由分隔线 / 表格分隔行
      // 等块级构造组成的长正文剥完标记会是空串，拿它判空会把整条实质消息丢掉
      const empty = isMd && !e.text.trim() && !hasTokens
      return {
        e,
        i,
        card,
        collapsible,
        collapsed,
        isMd,
        // 卡头为空（`tool_use · Bash` 已在头行）或正文为空时整段不渲染
        bodyText,
        truncated,
        hasTokens,
        hasBody: !!bodyText || collapsible || hasTokens || truncated,
        empty,
      }
    })
    .filter((row) => !row.empty)
})

function msgId(i: number): string {
  return `lg-msg-text-${i}`
}

function toggleExpand(i: number) {
  // 整体替换（而非原地 mutate）：Set 在 Vue 的响应式代理里 mutate 也能触发，
  // 但重建一次表达「本行折叠态集合」的完整快照，读代码时无歧义
  const next = new Set(expandedIndexes.value)
  if (next.has(i)) {
    next.delete(i)
  } else {
    next.add(i)
  }
  expandedIndexes.value = next
}
</script>

<template>
  <div class="ah-lg">
    <!-- ==================== 一级：来源区 + 查询 + 分页表格 ==================== -->
    <template v-if="!opened">
      <!-- 日志来源（默认折叠；内置只读 + 自定义增删 + 扫描） -->
      <div>
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-sources-toggle"
          :aria-expanded="sourcesOpen"
          @click="sourcesOpen = !sourcesOpen"
        >
          <span class="ah-lg-sources-chev" :class="{ open: sourcesOpen }">▸</span>
          {{ t('hub.lg.sources.title') }}
          <span class="ah-lg-sources-count">{{ sources.length }}</span>
        </button>
        <div v-if="sourcesOpen" class="ah-card ah-lg-sources-body">
          <div v-for="s in sources" :key="s.name" class="ah-lg-source">
            <div class="ah-lg-source-row">
              <AgentIcon :adapter="s.name" :size="16" />
              <span class="ah-lg-source-name">{{ s.name }}</span>
              <span class="ah-lg-source-type" :class="s.builtin ? 'builtin' : 'custom'">
                {{ s.builtin ? t('hub.lg.sources.builtin') : t('hub.lg.sources.custom') }}
              </span>
              <!-- 形态标记（票 07）：SQLite 库是只读单文件、不支持自定义增删 -->
              <span class="ah-lg-source-kind">{{ t(`hub.lg.sources.kind.${sourceKind(s)}`) }}</span>
              <span class="ah-lg-source-scan ah-mono">{{ scanCount(s) }}</span>
              <span class="ah-lg-source-flex"></span>
              <!-- 整来源移除：仅自定义来源（连带其全部目录） -->
              <button
                v-if="!s.builtin"
                type="button"
                class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-source-remove"
                :disabled="removingSource === s.name"
                :title="t('hub.lg.sources.removeSource')"
                @click="onRemoveSource(s.name)"
              >
                ✕
              </button>
              <!-- 给该来源追加目录：仅目录型来源（sqlite 单文件不接受增删） -->
              <button
                v-if="sourceKind(s) === 'jsonl'"
                type="button"
                class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-source-adddir"
                :aria-expanded="addingPathSource === s.name"
                @click="addingPathSource = addingPathSource === s.name ? null : s.name"
              >
                {{ addingPathSource === s.name ? t('hub.lg.sources.cancel') : t('hub.lg.sources.addDir') }}
              </button>
            </div>
            <!-- 目录清单（每来源多目录；内置默认路径只读，用户追加目录可移除） -->
            <div class="ah-lg-source-paths">
              <div v-for="p in sourcePaths(s)" :key="p.path" class="ah-lg-source-path-row">
                <span class="ah-lg-source-path-dot" aria-hidden="true"></span>
                <span class="ah-lg-source-path ah-mono" :title="p.path">
                  {{ abbreviateProject(p.path, home) }}
                </span>
                <span v-if="!p.removable" class="ah-lg-source-lock ah-mono">
                  {{ t('hub.lg.sources.builtinPath') }}
                </span>
                <button
                  v-if="p.removable"
                  type="button"
                  class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-source-path-remove"
                  :disabled="removingPath === `${s.name}\u0000${p.path}`"
                  :title="t('hub.lg.sources.removePath')"
                  @click="onRemoveSourcePath(s.name, p.path)"
                >
                  ✕
                </button>
              </div>
              <!-- 该来源的追加目录表单（fs:pick 选择器 + 回显 + 确认） -->
              <div v-if="addingPathSource === s.name" class="ah-lg-source-path-add">
                <span class="ah-lg-sources-pick">
                  <button
                    type="button"
                    class="ah-btn ah-btn-ghost ah-btn-sm"
                    :disabled="sourceBusy"
                    @click="onPickDirForSource()"
                  >
                    {{ sourceBusy ? t('hub.lg.sources.picking') : t('hub.lg.sources.pick') }}
                  </button>
                  <span
                    class="ah-lg-sources-pickpath ah-mono"
                    :title="newPathForSource"
                    :class="{ empty: !newPathForSource }"
                  >
                    {{ newPathForSource || t('hub.lg.sources.addPath') }}
                  </span>
                </span>
                <span class="ah-speed-actions-btns">
                  <button
                    type="button"
                    class="ah-btn ah-btn-primary ah-btn-sm"
                    :disabled="sourceBusy || !newPathForSource.trim()"
                    @click="onAddSourcePath(s.name)"
                  >
                    {{ t('hub.lg.sources.confirm') }}
                  </button>
                  <button
                    type="button"
                    class="ah-btn ah-btn-ghost ah-btn-sm"
                    :disabled="sourceBusy"
                    @click="cancelAddPath"
                  >
                    {{ t('hub.lg.sources.cancel') }}
                  </button>
                </span>
              </div>
            </div>
          </div>
          <div class="ah-lg-sources-actions">
            <button
              type="button"
              class="ah-btn ah-btn-primary ah-btn-sm"
              :disabled="syncing"
              @click="usage.scan()"
            >
              {{ syncing ? t('hub.st.scanning') : t('hub.lg.sources.scan') }}
            </button>
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="addingSource = true">
              {{ t('hub.lg.sources.add') }}
            </button>
            <span v-if="scanError" class="ah-cli-error" data-testid="scan-error">{{ scanError }}</span>
            <span v-if="sourceError" class="ah-cli-error">{{ sourceError }}</span>
          </div>
          <div v-if="addingSource" class="ah-lg-sources-add">
            <div class="ah-lg-sources-name">
              <Select
                v-model="sourceNamePick"
                class="ah-lg-sources-nameselect"
                size="sm"
                :options="sourceNameOptions"
                :disabled="sourceBusy"
                :placeholder="t('hub.lg.sources.namePick')"
              />
              <input
                v-model="newSourceName"
                class="ah-input"
                :placeholder="t('hub.lg.sources.addName')"
                :disabled="sourceBusy"
              />
            </div>
            <span class="ah-lg-sources-pick">
              <button
                type="button"
                class="ah-btn ah-btn-ghost ah-btn-sm"
                :disabled="sourceBusy"
                @click="onPickDir"
              >
                {{ sourceBusy ? t('hub.lg.sources.picking') : t('hub.lg.sources.pick') }}
              </button>
              <span
                class="ah-lg-sources-pickpath ah-mono"
                :title="newSourcePath"
                :class="{ empty: !newSourcePath, dup: pathDuplicate }"
                :aria-describedby="pathDuplicate ? 'lg-add-dup-error' : undefined"
                :aria-invalid="pathDuplicate ? 'true' : undefined"
              >
                {{ newSourcePath || t('hub.lg.sources.addPath') }}
              </span>
            </span>
            <p v-if="pathDuplicate" id="lg-add-dup-error" class="ah-cli-error" data-testid="add-dup">
              {{ t('hub.lg.sources.pathDuplicate') }}
            </p>
            <span class="ah-speed-actions-btns">
              <button
                type="button"
                class="ah-btn ah-btn-primary ah-btn-sm"
                :disabled="sourceBusy || addDisabled"
                @click="onAddSource"
              >
                {{ t('hub.lg.sources.confirm') }}
              </button>
              <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="sourceBusy" @click="cancelAddSource">
                {{ t('hub.lg.sources.cancel') }}
              </button>
            </span>
          </div>
        </div>
      </div>

      <!-- 多条件查询条 -->
      <div class="ah-card ah-lg-filter">
        <label class="ah-lg-filter-field">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.agent') }}</span>
          <Select v-model="filterAgent" class="min-w-[120px]" :options="agentOptions" />
        </label>
        <label class="ah-lg-filter-field ah-lg-filter-q">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.keyword') }}</span>
          <input
            v-model="keyword"
            class="ah-input"
            :placeholder="t('hub.lg.filter.keywordPh')"
            @keydown.enter="applyQuery"
          />
        </label>
        <label class="ah-lg-filter-field ah-lg-filter-date">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.from') }}</span>
          <!-- hide-input-icon：隐藏 vendor 的左侧日历图标（它会把框内文字推到 35px 处，
               与同排关键词输入框的 16px 明显不齐；「开始时间」标签 + 占位文案
               已说明这是日期框，此处图标是冗余暗示） -->
          <Datepicker
            v-model="fromInput"
            :format="dateFormat"
            :locale="dateLocale"
            :dark="isDark"
            :clearable="true"
            :enable-time-picker="true"
            :hide-input-icon="true"
            :select-text="dpSelectText"
            :cancel-text="dpCancelText"
            :now-button-label="dpNowLabel"
            :teleport="'body'"
            :placeholder="t('hub.lg.filter.fromPh')"
            data-testid="filter-from"
          />
        </label>
        <label class="ah-lg-filter-field ah-lg-filter-date">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.to') }}</span>
          <Datepicker
            v-model="toInput"
            :format="dateFormat"
            :locale="dateLocale"
            :dark="isDark"
            :clearable="true"
            :enable-time-picker="true"
            :hide-input-icon="true"
            :select-text="dpSelectText"
            :cancel-text="dpCancelText"
            :now-button-label="dpNowLabel"
            :teleport="'body'"
            :placeholder="t('hub.lg.filter.toPh')"
            data-testid="filter-to"
          />
        </label>
        <div class="ah-lg-filter-actions">
          <button type="button" class="ah-btn ah-btn-primary ah-btn-sm" :disabled="loadingSessions" @click="applyQuery">
            {{ t('hub.lg.filter.query') }}
          </button>
          <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="loadingSessions" @click="applyReset">
            {{ t('hub.lg.filter.reset') }}
          </button>
        </div>
      </div>

      <!-- 分页表格：Agent | 会话 | 项目 | 开始时间 | 时长 | Tokens -->
      <div class="ah-card ah-lg-table">
        <div class="ah-lg-table-head">
          <span>{{ t('hub.lg.col.agent') }}</span>
          <span>{{ t('hub.lg.col.session') }}</span>
          <span>{{ t('hub.lg.col.project') }}</span>
          <span class="ah-lg-col-time">{{ t('hub.lg.col.started') }}</span>
          <span class="ah-lg-col-dur">{{ t('hub.lg.col.duration') }}</span>
          <span class="ah-lg-col-tokens">{{ t('hub.lg.col.tokens') }}</span>
          <span class="ah-lg-col-go"></span>
        </div>

        <div class="ah-lg-table-rows">
          <div v-if="sessions.length === 0 && !loadingSessions" class="ah-st-empty">
            {{ authRequired ? t('hub.auth.banner') : t('hub.lg.noMatch') }}
          </div>
          <div
            v-for="s in sessions"
            :key="s.id"
            class="ah-lg-row"
            :class="{ 'ah-lg-row-active': s.active }"
            role="button"
            tabindex="0"
            :title="s.active ? t('hub.lg.row.currentTip') : t('hub.lg.row.open')"
            @click="openDetail(s.id)"
            @keydown.enter="openDetail(s.id)"
          >
          <div class="ah-lg-cell-agent">
            <AgentIcon :adapter="s.adapter" :size="16" />
            <span class="ah-lg-cell-agent-name">{{ s.adapter }}</span>
            <span v-if="s.active" class="ah-lg-badge-current">{{ t('hub.lg.row.current') }}</span>
          </div>
          <div class="ah-lg-cell-title" :title="s.title || s.cli_session_id">
            {{ s.title || s.cli_session_id }}
          </div>
          <div class="ah-lg-cell-sub" :title="s.project || ''">
            {{ abbreviateProject(s.project, home) }}
          </div>
          <div class="ah-lg-col-time ah-mono">{{ formatSessionTime(s.started_at) }}</div>
          <div class="ah-lg-col-dur ah-mono">{{ formatDuration(s.duration_ms) }}</div>
          <div class="ah-lg-col-tokens ah-mono">
            {{ formatTokens((s.tokens_in || 0) + (s.tokens_out || 0)) }}
          </div>
          <div class="ah-lg-col-go">›</div>
        </div>
        </div>

        <div class="ah-lg-pager">
          <span class="ah-lg-pager-info">
            {{ t('hub.lg.pager.total', { total: sessionsTotal }) }} ·
            {{ t('hub.lg.pager.page', { page, pages: totalPages }) }}
          </span>
          <!-- 翻页按钮只在真的有多页时出现：单页时两个按钮恒禁用，
               是既不能点也不传达信息的死控件（条目计数信息仍保留）。 -->
          <span v-if="totalPages > 1" class="ah-speed-actions-btns">
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="page <= 1 || loadingSessions"
              @click="usage.goPage(page - 1)"
            >
              ‹ {{ t('hub.lg.pager.prev') }}
            </button>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="page >= totalPages || loadingSessions"
              @click="usage.goPage(page + 1)"
            >
              {{ t('hub.lg.pager.next') }} ›
            </button>
          </span>
        </div>
      </div>
    </template>

    <!-- ==================== 二级：日志详情（页签：聊天 / 原始 JSONL） ==================== -->
    <template v-else>
      <div class="ah-lg-detail">
        <div class="ah-card ah-lg-detail-head">
          <div class="ah-lg-detail-head-row">
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-detail-back" @click="usage.closeSession()">
              ← {{ t('hub.lg.detail.back') }}
            </button>
            <AgentIcon :adapter="opened.session.adapter" :size="16" />
            <span class="ah-lg-detail-title">{{ opened.session.title || opened.session.cli_session_id }}</span>
            <span class="ah-cli-tag ok">{{ opened.session.adapter }}</span>
            <span class="ah-lg-detail-tabs" role="tablist">
              <button
                type="button"
                class="ah-lg-tab"
                :class="{ active: detailTab === 'chat' }"
                role="tab"
                :aria-selected="detailTab === 'chat'"
                @click="detailTab = 'chat'"
              >
                {{ t('hub.lg.detail.tabChat') }}
              </button>
              <button
                type="button"
                class="ah-lg-tab"
                :class="{ active: detailTab === 'raw' }"
                role="tab"
                :aria-selected="detailTab === 'raw'"
                @click="detailTab = 'raw'"
              >
                {{ t('hub.lg.detail.tabRaw') }}
              </button>
            </span>
          </div>
          <div class="ah-lg-head-src ah-mono" :title="opened.session.source_path || ''">
            {{ opened.session.source_path || t('hub.lg.noSource') }}
          </div>
          <div class="ah-lg-head-meta ah-mono">
            <span v-if="opened.session.model">{{ opened.session.model }}</span>
            <span>↑ {{ formatTokens(opened.session.tokens_in) }} ↓ {{ formatTokens(opened.session.tokens_out) }}</span>
            <span>⚡ {{ formatTokens(opened.session.tokens_cache_read) }} +{{ formatTokens(opened.session.tokens_cache_write) }}</span>
            <span v-if="opened.session.tokens_reasoning > 0">◈ {{ formatTokens(opened.session.tokens_reasoning) }}</span>
            <span>{{ abbreviateProject(opened.session.project, home) }}</span>
          </div>
          <div v-if="opened.eventsTruncated" class="ah-lg-warn">{{ t('hub.lg.eventsTruncated') }}</div>
          <div v-if="opened.rawTruncated" class="ah-lg-warn">{{ t('hub.lg.rawTruncated') }}</div>
        </div>

        <!-- 任务记录（只读对话样式，无输入框） -->
        <div v-if="detailTab === 'chat'" class="ah-card ah-lg-events">
          <div v-if="opened.events.length === 0" class="ah-st-empty">{{ t('hub.lg.noEvents') }}</div>
          <div v-else class="ah-chat">
            <div class="ah-chat-scroll">
              <div
                v-for="row in chatRows"
                :key="row.i"
                class="ah-msg"
                :class="`role-${row.e.role}`"
              >
                <div class="ah-msg-head">
                  <span class="ah-msg-dot" aria-hidden="true"></span>
                  <span class="ah-msg-role">{{ t(`hub.lg.role.${row.e.role}`) }}</span>
                  <!-- 工具卡头：类型 · 名称（无头形态时整条都落进卡身） -->
                  <span v-if="row.card && row.card.head" class="ah-msg-tool-head ah-mono">
                    {{ row.card.head }}
                  </span>
                  <span v-if="row.e.model" class="ah-msg-model ah-mono">{{ row.e.model }}</span>
                  <span class="ah-msg-time ah-mono">{{ formatEventTime(row.e.ts) }}</span>
                </div>
                <div
                  v-if="row.hasBody"
                  class="ah-msg-body"
                  :class="{ 'is-error': row.card?.isError || row.e.error }"
                >
                  <!-- 助手正文：markdown 结构化渲染（折叠态是剥标记的纯文本预览） -->
                  <div
                    v-if="row.isMd && row.bodyText"
                    :id="msgId(row.i)"
                    class="ah-msg-text ah-md"
                    :class="{ 'is-collapsed': row.collapsed }"
                    data-testid="lg-msg-md"
                    v-html="row.bodyText"
                  ></div>
                  <!-- 工具卡身 / 用户 / 系统：纯文本（命令原文不该排版） -->
                  <div
                    v-else-if="row.bodyText"
                    :id="msgId(row.i)"
                    class="ah-msg-text"
                    :class="{ 'is-collapsed': row.collapsed }"
                  >{{ row.bodyText }}</div>
                  <!-- 模型输出 / 工具卡身折叠：超阈值默认收起，点击展开/收起 -->
                  <button
                    v-if="row.collapsible"
                    type="button"
                    class="ah-msg-expand"
                    :aria-expanded="!row.collapsed"
                    :aria-controls="msgId(row.i)"
                    @click="toggleExpand(row.i)"
                  >
                    {{ row.collapsed
                      ? row.card
                        ? t('hub.lg.detail.expandDetail')
                        : t('hub.lg.detail.expand')
                      : t('hub.lg.detail.collapse') }}
                  </button>
                  <!-- guest 上限截断：给一个看得见的完整原文路径（票 B4 可做部分） -->
                  <div v-if="row.truncated" class="ah-msg-trunc" data-testid="lg-msg-truncated">
                    <span>{{ t('hub.lg.detail.truncated') }}</span>
                    <button type="button" class="ah-msg-trunc-jump" @click="detailTab = 'raw'">
                      {{ t('hub.lg.detail.viewRaw') }}
                    </button>
                  </div>
                  <div v-if="row.hasTokens" class="ah-msg-meta ah-mono">
                    <span>{{ tokenMeta(row.e) }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 原始 JSONL 行视图 -->
        <div v-else class="ah-card ah-lg-raw-card">
          <div v-if="opened.raw.length === 0" class="ah-st-empty">{{ t('hub.lg.noRaw') }}</div>
          <div v-else class="ah-lg-raw ah-mono">
            <div
              v-for="row in rawRows"
              :key="row.i"
              class="ah-lg-raw-line"
              :class="{ 'is-collapsed': row.collapsed }"
            >
              <div class="ah-lg-raw-text">{{ row.text }}</div>
              <!-- 原始行折叠：超阈值行默认收起，点开看全文（同聊天行折叠） -->
              <button
                v-if="row.long"
                type="button"
                class="ah-lg-raw-expand"
                :aria-expanded="row.expanded"
                @click="toggleRawExpand(row.i)"
              >
                {{ row.expanded ? t('hub.lg.detail.collapse') : t('hub.lg.detail.expand') }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>