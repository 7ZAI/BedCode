/**
 * 使用统计展示层格式化（票据 06 + 看板改版）
 *
 * 看板与会话日志共用的纯函数：token 量缩写、时长缩写、按天标签。
 * 数字与源数据一致性由后端聚合保证，本层只做无损展示换算（四舍五入
 * 截断进位方向向上取整到 0.1 粒度，避免「显示 0 但实际非 0」的假零）。
 */
import type { StatsMetric } from '../types'

/**
 * 会话聊天视图「模型输出折叠」阈值（字符）。
 *
 * 超过才提供展开控件且默认**收起**（短消息 clamp 剪不到，按钮是噪音）。
 * 设计依据：ui-ux-pro-max ux-guidelines「Content/Truncation: Truncate with
 * ellipsis and expand option」；预览行为 3 行（取 line-clamp-2 示例语义、
 * 略放宽一行为模型输出的结构化正文留可读预览，仍带省略号 + 展开入口）。
 */
export const COLLAPSE_THRESHOLD_CHARS = 500

/**
 * 工具卡「卡身」折叠阈值（字符）。
 *
 * 工具卡默认收起（对照 zcode/qoder/trae），阈值取 guest 工具上限 1000 的四成：
 * 低于它直接铺开（短命令 / 单行结果一眼看完），高于它收起并给展开控件——
 * 参数 / 输出（命令、长栈、文件内容）点开才看得完，铺开会淹没对话流。
 * **不做字符级截断**：阈值只决定是否给展开控件，DOM 里始终是 guest 给的全文。
 */
export const COLLAPSE_THRESHOLD_TOOL_CHARS = 400

/**
 * 「原始 JSONL」单行折叠阈值（字符）。
 *
 * **为什么必须逐行折叠**（2026-10-06 实机）：JSONL 一行 = 一个完整 JSON 对象，
 * 而 agent 日志把整份系统提示词塞进单行——codex 每个 rollout 的**第一行**
 * `session_meta` 就带 `base_instructions`，实测 ~21KB/行，语料均值 3.3KB/行、
 * 最大 62KB。容器是 `white-space: pre-wrap` + `word-break: break-all`，
 * 于是首行折成上千个视觉行、内容全在折叠线以下：打开「原始 JSONL」只看到一堵
 * 没法扫的 JSON 墙，等于「没显示」。
 *
 * 超过本阈值才给展开控件且默认收起（短行 clamp 不到，按钮是噪音）；与聊天行
 * 折叠同款交互，只是粒度从「消息」降到「行」。**折叠时 DOM 里只放截断预览**，
 * 展开才铺全文——否则上千行长文本会先把渲染线程拖垮。
 */
export const RAW_LINE_COLLAPSE_CHARS = 1000

/**
 * guest 解析层的截断上限矩阵（展示层镜像，**只用于「疑似截断」提示**）
 *
 * 单一事实源在 guest `usage_parse/*.rs` 的 `truncate_text(_, cap)` 调用点：
 * - 消息正文（user / assistant）2000——四适配器一致
 * - 工具输出 / 结果 / 参数 1000（codex、opencode、claude `tool_result`、pi
 *   `toolResult`；**2026-10-03 从 400 提上来**——400 字符点开也读不完命令输出）
 * - 工具参数摘要 / attachment 名称（claude `TOOL_ARGS_CAP`）120
 * - 会话标题 120（不进事件流，本层用不到）
 * 另有事件流条数上限 `MAX_EVENTS = 5000`（已有独立的「事件流过长」横幅）。
 *
 * **只读镜像**：上限在 guest 变动时本层最多漏报一次（提示语刻意用「已达上限」
 * 的建议口吻），不会谎报——真正的信号是 guest 截断时补的那个省略号。
 */
export const GUEST_TEXT_CAPS = {
  message: 2000,
  toolOutput: 1000,
  toolArgs: 120,
  title: 120,
} as const

/**
 * 「自然长句恰好以省略号收尾」的排除阀（= guest 最小上限）
 *
 * 只看「以 `…` 收尾」会把中文里常见的「等等…」误报；把长度条件压到
 * **最小**上限（120）而不是按角色取各自上限，是因为卡内可能有嵌套上限
 * （`tool_use · 名称 · 参数摘要120` 整条远不到工具行的 400），且提示语是
 * 建议式的——宁可多提示一次，也不让真截断从提示里漏掉。
 */
export const TRUNCATION_MIN_CAP = Math.min(...Object.values(GUEST_TEXT_CAPS))

/**
 * 事件文本是否疑似「被 guest 上限截断」
 *
 * guest 的 `truncate_text` 只在真截断时补一个省略号收尾，因此**末字符是省略号
 * 就是必要信号**；长度条件只用来滤掉自然收尾的省略号。判定按角色分流：
 * - **工具行**只看**最后一个 ` · ` 段**：被截的永远是卡身那段，按整条长度判会
 *   连带误判（参数摘要 120 上限的行整条可能只有 150 字）。
 * - **非工具行**（user / assistant / system 正文）回退**整条长度**判：正文里出现
 *   ` · ` 是常态（“配置 · 超时 · 重试”式行文），只看末段会把真截断整条漏报。
 *
 * 展示层据此给出「完整原文见原始 JSONL」的可见路径（票 B4 的可做部分）。
 */
export function looksTruncated(text: string): boolean {
  if (!text.endsWith('…')) return false
  const parts = text.split(' · ')
  // 无分隔段：单段文本没有「头/身」结构，整条判
  if (parts.length === 1) return parts[0].length > TRUNCATION_MIN_CAP
  // 首段是工具类型前缀才走「末段」判，其余（正文里带 · 的行文）整条判
  const isToolRow = TOOL_KINDS.has(parts[0].trim())
  return (isToolRow ? parts[parts.length - 1].length : text.length) > TRUNCATION_MIN_CAP
}

/** 工具卡的展示切片：卡头（类型 · 名称）+ 卡身（参数 / 输出）+ 失败标记 */
export interface ToolCard {
  /** 卡头文本；空表示整条都属卡身 */
  head: string
  /** 卡身文本；空表示这条只有头（`tool_use · Bash`） */
  body: string
  /** 失败标记：文本尾部的 `(error)` 约定（pi 直接给；claude 的权威来源是 wire `error`） */
  isError: boolean
}

/** 四适配器写进 tool 事件的「类型前缀」字面量（见各 `usage_parse/*.rs`） */
const TOOL_KINDS = new Set(['tool', 'tool_use', 'tool_result'])

/**
 * guest 非文本块占位 token 的展示层镜像
 *
 * 真源：`wasm-apps/agent-hub/rust/src/usage_parse/common.rs` 的
 * `NON_TEXT_TOKEN_PREFIX` / `NON_TEXT_TOKEN_GENERIC` 与 `non_text_placeholder()`
 * （kind 严格校验 `[A-Za-z0-9_.-]{1,40}`，不合规退化为无 kind 形态）。
 *
 * guest 只吐机器 token 是因为 wire 上没有 i18n 通道（裸字符串）——文案必须在前端
 * 语言包里查；此处是正则镜像，两侧任一改动需同步（guest 侧有单测钉形状）。
 */
const NON_TEXT_TOKEN_RE = /\[non-text(?::([A-Za-z0-9_.-]{1,40}))?\]/g

/** i18n 取值器签名（与 `context.i18n.t` 同形；传进来以保持本文件无 i18n 依赖） */
type Translate = (key: string, params?: Record<string, unknown>) => string

/**
 * 把 guest 的非文本块占位 token 换成本地化文案
 *
 * - 带 kind → `hub.lg.nonTextBlock`（kind 作参数）
 * - 无 kind / token 形态不符 → `hub.lg.nonTextBlockUnknown`
 * - 无 token 时原样返回（快速路径：不碰正文）
 *
 * **为什么整组替换而不是「包含即替换」**：工具输出是不可信文本，若 agent 输出里恰好
 * 出现 `[non-text:x]` 字面量，只有「整组匹配」才不会被误当占位改写——宁可漏报，
 * 不可篡改真实输出。
 */
function renderNonTextBlocks(text: string, t: Translate): string {
  if (!text.includes('[non-text')) return text
  return text.replace(
    NON_TEXT_TOKEN_RE,
    (_m, kind: string | undefined) =>
      kind ? t('hub.lg.nonTextBlock', { kind }) : t('hub.lg.nonTextBlockUnknown'),
  )
}

/** 卡身占位本地化（t 缺省时不做任何改写，纯文本工具照旧） */
function localize(body: string, t?: Translate): string {
  return t ? renderNonTextBlocks(body, t) : body
}

/**
 * 工具事件文本切成「卡头 + 卡身」（卡身内的非文本块占位按传入的 `t` 本地化）
 *
 * guest 把工具行拼成一条文本，以 ` · ` 分隔；各形态的卡头长度不同：
 * - claude `tool_use · 名称` / `tool_use · 名称 · 参数摘要`（有参数才有第三段）
 * - claude `tool_result · 输出` 或配对后 `tool_result · 名称 · 输出`
 * - codex / opencode `tool · 名称 (状态) · 输出`；codex `tool_result · 输出`
 * - pi `bash (error) · 输出`（无类型前缀，首段即卡头）
 *
 * 未知形态（首段不是已知类型、也没有分隔符）整条落卡身，不凭空造头、
 * 不切碎正文：`tool_result` 有第三段时按「带名称」解读是**启发式**——输出正文
 * 自身含 ` · ` 时会把首段词挪进卡头，但内容一段不少。
 */
export function splitToolText(text: string, t?: Translate): ToolCard {
  const segments = text.split(' · ')
  if (segments.length === 1) {
    return { head: '', body: localize(text, t), isError: hasErrorMark(text) }
  }
  const kind = segments[0].trim()
  const tail = segments.slice(1)
  let head: string
  let body: string
  if (!TOOL_KINDS.has(kind)) {
    // pi 形态：首段就是工具名（可带 (error) / (status)）
    head = segments[0]
    body = tail.join(' · ')
  } else if (kind === 'tool_result') {
    // 有第三段 = 配对后带名称的形态（`tool_result · Bash · 输出`）
    head = tail.length >= 2 ? segments.slice(0, 2).join(' · ') : kind
    body = (tail.length >= 2 ? tail.slice(1) : tail).join(' · ')
  } else {
    // `tool · 名称 (状态)` / `tool_use · 名称`：头两段归卡头，余下归卡身
    head = segments.slice(0, 2).join(' · ')
    body = tail.slice(1).join(' · ')
  }
  return { head, body: localize(body, t), isError: hasErrorMark(head) }
}

/** guest 失败标记约定：文本尾部的 `(error)`（大小写不敏感） */
function hasErrorMark(s: string): boolean {
  return /\(error\)\s*$/i.test(s.trim())
}

/** token / 大数字量缩写：0–999 原样，k / M / G 三级，1 位小数 */
export function formatTokens(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return '—'
  const v = Math.max(0, n)
  if (v < 1000) return String(Math.round(v))
  const units = ['', 'k', 'M', 'G', 'T'] as const
  let tier = 0
  let scaled = v
  while (scaled >= 1000 && tier < units.length - 1) {
    scaled /= 1000
    tier++
  }
  // 1 位小数，向上进位避免假零（如 0.04k → 0.1k 而非 0k）
  const rounded = scaled < 10 ? (Math.ceil(scaled * 10) / 10).toFixed(1) : Math.round(scaled).toString()
  return `${rounded}${units[tier]}`
}

/**
 * 坐标轴刻度标签：在 formatTokens 基础上抹掉「整数档上的多余小数」
 *
 * formatTokens 对 <10 的缩放值一律带 1 位小数（数据值需要：1.3M 是真信息），
 * 但刻度是人为选定的整齐档，写成「3.0M / 2.0M / 1.0M」反而像未完成的四舍五入。
 * 只影响 Y 轴刻度，数据值（悬停读数、表格、tooltip）仍用 formatTokens。
 *
 * **亚千刻度（2026-10-04 OCR A-02）**：formatTokens 对 <1000 直接
 * `String(Math.round(v))`——axisScale 合法产出的 0.6/0.4/0.2 会被取整抹平为
 * 1/0/0，坐标轴信息全丢。这里对 <1000 的刻度保留小数（仅剥掉整数值的
 * `.0` 尾巴，保持与整数档一致的「整洁」）。
 */
export function formatAxisTick(v: number): string {
  if (v == null || !Number.isFinite(v)) return '—'
  if (v === 0) return '0'
  if (v !== Math.round(v)) {
    // 亚千刻度（非整数：axisScale 产出 0.6/0.4/0.2 这类）：保留小数，只剥
    // 整数值的 `.0` 尾巴（1.0 → 1）
    return String(v).replace(/\.0(?=$)/, '')
  }
  return formatTokens(v).replace(/\.0(?=[kMGT]|$)/, '')
}

/** 毫秒时长缩写：<1s 为 ms；<60s 为 s；<60m 为 min；<24h 为 h（1 位小数）；否则 d */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  const s = ms / 1000
  if (s < 60) return `${roundCeil(s)}s`
  const min = s / 60
  if (min < 60) return `${roundCeil(min)}min`
  const h = min / 60
  if (h < 24) return `${roundCeil(h)}h`
  const d = h / 24
  return `${roundCeil(d)}d`
}

/** 向上取整到 0.1 粒度并格式化（整数则不带小数点；-1e-9 抵消浮点误差） */
function roundCeil(v: number): string {
  const scaled = Math.ceil(v * 10 - 1e-9) / 10
  return Number.isInteger(scaled) ? String(scaled) : scaled.toFixed(1)
}

/** epoch ms → 本地「MM-DD HH:mm」标签（会话列表用） */
export function formatSessionTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return '—'
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** epoch ms → 本地「HH:mm:ss」标签（事件流时间列用） */
export function formatEventTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return ''
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 项目路径缩写：家目录前缀折叠为 ~ */
export function abbreviateProject(project: string | null | undefined, home: string): string {
  if (!project) return '—'
  if (home && (project === home || project.startsWith(`${home}/`))) {
    return `~${project.slice(home.length)}`
  }
  return project
}

/** $ 成本展示：null 不估算显示 —；有值保留 2 位小数 */
export function formatCost(cost: number | null | undefined): string {
  if (cost == null || !Number.isFinite(cost)) return '—'
  return `$${cost.toFixed(2)}`
}

// ==================== 看板指标换算 ====================

/**
 * 最小 token 桶形状
 *
 * 字段全可选：看板的分组行（按天 / CLI / 项目 / 模型）与节奏格
 * （只有总量 `tokens`、无分桶）都是它的子集，取值函数因此能对
 * **所有**行型统一工作而不必断言成某个具体行型。
 */
export interface TokenBuckets {
  tokens_in?: number
  tokens_out?: number
  tokens_cache_read?: number
  tokens_cache_write?: number
}

/** 看板任一行的度量字段超集（指标取值的唯一入参形状；不导出：调用方直接传具体行型） */
interface MetricRow extends TokenBuckets {
  sessions?: number
  tokens_reasoning?: number
  cost_total?: number | null
  duration_ms?: number
  /** 节奏格专用：已算好的 token 总量（该格无分桶） */
  tokens?: number
}

/** token 总量 = 输入 + 输出 + 缓存读 + 缓存写
 *
 * **不加推理**：claude 的 `thinking_tokens` 是 `output_tokens` 的子集
 * （见 guest `usage_parse::claude`），重复相加会凭空放大总量。推理另作
 * 独立指标（`tokens_reasoning`）呈现，语义上标注为「其中推理」。
 *
 * 入参用 [`MetricRow`]（而非 [`TokenBuckets`]）：调用方拿到的都是完整行，
 * 带上 `tokens_reasoning` 字段也应当能直接传入——这正是「推理被忽略」
 * 得以被单测证明的前提。
 */
export function totalTokens(r: MetricRow | null | undefined): number {
  if (!r) return 0
  return (
    (r.tokens_in || 0) +
    (r.tokens_out || 0) +
    (r.tokens_cache_read || 0) +
    (r.tokens_cache_write || 0)
  )
}

/** 按指标取值（趋势 / 排行 / 占比共用的唯一取值口径；缺失归 0） */
export function metricValue(r: MetricRow, metric: StatsMetric): number {
  switch (metric) {
    case 'tokens':
      return r.tokens ?? totalTokens(r)
    case 'tokens_in':
      return r.tokens_in ?? 0
    case 'tokens_out':
      return r.tokens_out ?? 0
    case 'tokens_cache_read':
      return r.tokens_cache_read ?? 0
    case 'tokens_cache_write':
      return r.tokens_cache_write ?? 0
    case 'tokens_reasoning':
      return r.tokens_reasoning ?? 0
    case 'sessions':
      return r.sessions ?? 0
    case 'cost_total':
      return r.cost_total ?? 0
    case 'duration_ms':
      return r.duration_ms ?? 0
  }
}

/**
 * 缓存命中率 = 缓存读 / (输入 + 缓存读 + 缓存写)
 *
 * 分母是「进模型的输入侧总量」：命中越多，同样的任务要重付的输入越少。
 * 分母为 0（还没产生过输入）时无命中率可言，返回 null 而不是 0%——
 * 「0%」会被读成「缓存完全没起作用」。
 */
export function cacheHitRate(r: TokenBuckets | null | undefined): number | null {
  if (!r) return null
  const denom = (r.tokens_in || 0) + (r.tokens_cache_read || 0) + (r.tokens_cache_write || 0)
  if (denom <= 0) return null
  return (r.tokens_cache_read || 0) / denom
}

/** 百分比展示（0–1 小数 → 整数 %，null → —） */
export function formatPercent(ratio: number | null | undefined, digits = 0): string {
  if (ratio == null || !Number.isFinite(ratio)) return '—'
  return `${(ratio * 100).toFixed(digits)}%`
}

/** 坐标轴刻度档位（「好看的」数字梯队） */
const NICE_MANTISSAS = [1, 2, 2.5, 5, 10] as const

/** 坐标轴标度：上界与各档刻度，供折线/面积图共用一套缩放 */
export interface AxisScale {
  /** 上界（= step × count）：最高点与上界之间保留至少一档的呼吸 */
  top: number
  /** 档距（1 / 2 / 2.5 / 5 / 10 × 10^k） */
  step: number
  /** 自上而下的刻度（含 top 与 0），长度恒为 count + 1 */
  ticks: number[]
}

/**
 * 坐标轴标度：档距吸附到「好看的」梯队，上界 = 档距 × 档数
 *
 * 为什么不直接用 `max` 再等分：等分会造出 2.5M → [2.5M, 1.7M, 833k, 0] 这类
 * 读不出来的刻度（833k 既不是 1/2/2.5/5 的任一档，也没法在脑子里心算）。
 * 先把**档距**吸附到整齐梯队、再乘档数，标签就恒是可读的整数档；代价是上界
 * 可能略高于 max（数据不顶边，正是期望的呼吸）。数据全 0 / 非法时返回 top=1、
 * step=1/count（避免除零与空路径）。
 */
export function axisScale(max: number, count = 3): AxisScale {
  if (!Number.isFinite(max) || max <= 0 || count <= 0) {
    // 兜底：top=1、step=1/n（n≥1，count≤0 时用 1 防除零）。**刻度必须自上而下
    // 降序**（与正常分支的 `[top,…,0]` 同构）——旧实现 `i === count ? 1 : 0`
    // 产出升序 [0,0,0,1]，全零/非法数据时 Y 轴上下颠倒（2026-10-04 OCR A-01）；
    // count≤0 时 `1/count` 是 Infinity，刻度全为 1 重叠。
    const n = Math.max(count, 1)
    const step = 1 / n
    return { top: 1, step, ticks: Array.from({ length: n + 1 }, (_, i) => (i === 0 ? 1 : 0)) }
  }
  // 先用「好看的上界」压一档：避免 max 恰在档距边界时多出一整档空白
  const exp = Math.floor(Math.log10(max))
  const base = 10 ** exp
  let niceTop = 10 * base
  for (const m of NICE_MANTISSAS) {
    const cand = m * base
    if (cand >= max - 1e-9) {
      niceTop = cand
      break
    }
  }
  const rough = niceTop / count
  const stepExp = Math.floor(Math.log10(rough))
  const stepBase = 10 ** stepExp
  let step = 10 * stepBase
  for (const m of NICE_MANTISSAS) {
    const cand = m * stepBase
    if (cand >= rough - 1e-9) {
      step = cand
      break
    }
  }
  const top = step * count
  return { top, step, ticks: Array.from({ length: count + 1 }, (_, i) => step * (count - i)) }
}
