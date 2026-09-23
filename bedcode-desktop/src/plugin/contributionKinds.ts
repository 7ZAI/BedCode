/**
 * Contribution Kinds Registry
 *
 * 扩展点种类描述符注册表 + 权限元数据 + 展示辅助函数。
 * 新增扩展点种类只需在此表加一条目 + 对应 i18n key，不动渲染分支。
 *
 * 与 PluginInfo.contributes 字段一一对应（不含 icon 由 PluginIcon 组件处理）
 */

import type { PluginInfo, PluginState } from '@/plugin/types'
import i18n from '@/locales'

// ==================== 扩展点 Chip 描述符 ====================

/** 扩展点 chip 展示数据 */
export interface ContributionChip {
  /** 唯一标识 */
  key: string
  /** 图标 emoji（由表携带，不本地化） */
  emoji: string
  /** i18n label key */
  labelKey: string
  /** i18n 插值参数 */
  params?: Record<string, unknown>
}

/** 扩展点种类描述符注册表 */
interface ContributionKind {
  emoji: string
  labelKey: string
  /** 从 PluginInfo 提取 items，返回数量或空表示该种类不存在 */
  count: (p: PluginInfo) => number
  /** i18n 参数构造器（如 commands 需要 count） */
  params?: (p: PluginInfo) => Record<string, unknown>
}

const CONTRIBUTION_KINDS: Record<string, ContributionKind> = {
  sidebar: {
    emoji: '📋',
    labelKey: 'desktop.plugin.chip.sidebar',
    count: (p) => p.contributes.views?.filter((v) => v.type === 'sidebar').length ?? 0,
  },
  toolbox: {
    emoji: '🧰',
    labelKey: 'desktop.plugin.chip.toolbox',
    count: (p) => p.contributes.views?.filter((v) => v.type === 'toolbox').length ?? 0,
  },
  statusbar: {
    emoji: '📊',
    labelKey: 'desktop.plugin.chip.statusbar',
    count: (p) => p.contributes.views?.filter((v) => v.type === 'statusbar').length ?? 0,
  },
  commands: {
    emoji: '🔧',
    labelKey: 'desktop.plugin.chip.commands',
    count: (p) => p.contributes.commands?.length ?? 0,
    params: (p) => ({ count: p.contributes.commands?.length ?? 0 }),
  },
  terminal: {
    emoji: '⌨️',
    labelKey: 'desktop.plugin.chip.terminal',
    count: (p) => (p.contributes.terminal ? 1 : 0),
  },
  // `toolProviders` / `fileHandlers` 不再列为 chip：宿主侧从未落地消费实现
  // （声明了也不会生效），展示为「扩展点」是误导——摘除展示，类型字段暂留兼容。
  configuration: {
    emoji: '🎛️',
    labelKey: 'desktop.plugin.chip.configuration',
    count: (p) => (p.contributes.configuration ? 1 : 0),
  },
  lifecycle: {
    emoji: '🔄',
    labelKey: 'desktop.plugin.chip.lifecycle',
    count: (p) => (p.contributes.lifecycle ? 1 : 0),
  },
}

// ==================== 权限元数据 ====================

/** 权限元数据项 */
export interface PermissionMeta {
  emoji: string
  titleKey: string
  descKey: string
  /** 高危位的后果文案（仅高危位有；审批弹层据此红色强调，见 HIGH_RISK_PERMISSIONS） */
  riskKey?: string
}

/**
 * 高危权限位（ADR 0020 审批裁决 3：整单批准 + 高位视觉强调）
 *
 * 判据是「一旦授予即可在用户机器上执行任意代码 / 读写他人数据」：
 * 进程执行、伪终端创建（等价于任意命令）、终端输入（注入执行）、主库面（内含
 * 设备/信任/设置等他方数据）。清单固定，变更需同步 risk 文案与弹层行为。
 */
export const HIGH_RISK_PERMISSIONS: readonly string[] = [
  'process:run',
  'pty:spawn',
  'terminal:input',
  'database:main',
]

/** 权限元数据注册表（覆盖 SDK 词汇表全部条目；未知权限回退原始串，见 getPermissionMeta） */
const PERMISSION_META: Record<string, PermissionMeta> = {
  storage: {
    emoji: '💾',
    titleKey: 'desktop.plugin.perm.storage.title',
    descKey: 'desktop.plugin.perm.storage.desc',
  },
  'terminal:input': {
    emoji: '⌨️',
    titleKey: 'desktop.plugin.perm.terminal:input.title',
    descKey: 'desktop.plugin.perm.terminal:input.desc',
    riskKey: 'desktop.plugin.perm.terminal:input.risk',
  },
  'terminal:output': {
    emoji: '📺',
    titleKey: 'desktop.plugin.perm.terminal:output.title',
    descKey: 'desktop.plugin.perm.terminal:output.desc',
  },
  'terminal:observe': {
    emoji: '👁️',
    titleKey: 'desktop.plugin.perm.terminal:observe.title',
    descKey: 'desktop.plugin.perm.terminal:observe.desc',
  },
  'session:read': {
    emoji: '📄',
    titleKey: 'desktop.plugin.perm.session:read.title',
    descKey: 'desktop.plugin.perm.session:read.desc',
  },
  // 票 04：`host-connection` 独立原语的判据位（读宿主 server 在册连接）
  'connection:read': {
    emoji: '🔌',
    titleKey: 'desktop.plugin.perm.connection:read.title',
    descKey: 'desktop.plugin.perm.connection:read.desc',
  },
  'session:write': {
    emoji: '✏️',
    titleKey: 'desktop.plugin.perm.session:write.title',
    descKey: 'desktop.plugin.perm.session:write.desc',
  },
  'session:config': {
    emoji: '🛠️',
    titleKey: 'desktop.plugin.perm.session:config.title',
    descKey: 'desktop.plugin.perm.session:config.desc',
  },
  'ui:sidebar': {
    emoji: '📋',
    titleKey: 'desktop.plugin.perm.ui:sidebar.title',
    descKey: 'desktop.plugin.perm.ui:sidebar.desc',
  },
  'ui:input': {
    emoji: '🔤',
    titleKey: 'desktop.plugin.perm.ui:input.title',
    descKey: 'desktop.plugin.perm.ui:input.desc',
  },
  'ui:toolbox': {
    emoji: '🧰',
    titleKey: 'desktop.plugin.perm.ui:toolbox.title',
    descKey: 'desktop.plugin.perm.ui:toolbox.desc',
  },
  'ui:settings': {
    emoji: '⚙️',
    titleKey: 'desktop.plugin.perm.ui:settings.title',
    descKey: 'desktop.plugin.perm.ui:settings.desc',
  },
  'ui:statusbar': {
    emoji: '📊',
    titleKey: 'desktop.plugin.perm.ui:statusbar.title',
    descKey: 'desktop.plugin.perm.ui:statusbar.desc',
  },
  'ui:dialog': {
    emoji: '🪟',
    titleKey: 'desktop.plugin.perm.ui:dialog.title',
    descKey: 'desktop.plugin.perm.ui:dialog.desc',
  },
  'ui:pageToolbar': {
    emoji: '🧷',
    titleKey: 'desktop.plugin.perm.ui:pageToolbar.title',
    descKey: 'desktop.plugin.perm.ui:pageToolbar.desc',
  },
  'ui:fileHandler': {
    emoji: '🗂️',
    titleKey: 'desktop.plugin.perm.ui:fileHandler.title',
    descKey: 'desktop.plugin.perm.ui:fileHandler.desc',
  },
  'network:http': {
    emoji: '🌐',
    titleKey: 'desktop.plugin.perm.network:http.title',
    descKey: 'desktop.plugin.perm.network:http.desc',
  },
  'database:main': {
    emoji: '🗄️',
    titleKey: 'desktop.plugin.perm.database:main.title',
    descKey: 'desktop.plugin.perm.database:main.desc',
    riskKey: 'desktop.plugin.perm.database:main.risk',
  },
  'fs:read': {
    emoji: '📂',
    titleKey: 'desktop.plugin.perm.fs:read.title',
    descKey: 'desktop.plugin.perm.fs:read.desc',
  },
  'fs:write': {
    emoji: '📝',
    titleKey: 'desktop.plugin.perm.fs:write.title',
    descKey: 'desktop.plugin.perm.fs:write.desc',
  },
  broadcast: {
    emoji: '📩',
    titleKey: 'desktop.plugin.perm.broadcast.title',
    descKey: 'desktop.plugin.perm.broadcast.desc',
  },
  'timer:schedule': {
    emoji: '⏱️',
    titleKey: 'desktop.plugin.perm.timer:schedule.title',
    descKey: 'desktop.plugin.perm.timer:schedule.desc',
  },
  'process:run': {
    emoji: '⚡',
    titleKey: 'desktop.plugin.perm.process:run.title',
    descKey: 'desktop.plugin.perm.process:run.desc',
    riskKey: 'desktop.plugin.perm.process:run.risk',
  },
  'app:cli': {
    emoji: '🔗',
    titleKey: 'desktop.plugin.perm.app:cli.title',
    descKey: 'desktop.plugin.perm.app:cli.desc',
  },
  peer: {
    emoji: '📡',
    titleKey: 'desktop.plugin.perm.peer.title',
    descKey: 'desktop.plugin.perm.peer.desc',
  },
  mdns: {
    emoji: '🔍',
    titleKey: 'desktop.plugin.perm.mdns.title',
    descKey: 'desktop.plugin.perm.mdns.desc',
  },
  'ws:client': {
    emoji: '🔌',
    titleKey: 'desktop.plugin.perm.ws:client.title',
    descKey: 'desktop.plugin.perm.ws:client.desc',
  },
  'ws:server': {
    emoji: '🛰️',
    titleKey: 'desktop.plugin.perm.ws:server.title',
    descKey: 'desktop.plugin.perm.ws:server.desc',
  },
  auth: {
    emoji: '🔑',
    titleKey: 'desktop.plugin.perm.auth.title',
    descKey: 'desktop.plugin.perm.auth.desc',
  },
  'pty:spawn': {
    emoji: '🖥️',
    titleKey: 'desktop.plugin.perm.pty:spawn.title',
    descKey: 'desktop.plugin.perm.pty:spawn.desc',
    riskKey: 'desktop.plugin.perm.pty:spawn.risk',
  },
  'pty:io': {
    emoji: '⌨️',
    titleKey: 'desktop.plugin.perm.pty:io.title',
    descKey: 'desktop.plugin.perm.pty:io.desc',
  },
  'task:run': {
    emoji: '🧵',
    titleKey: 'desktop.plugin.perm.task:run.title',
    descKey: 'desktop.plugin.perm.task:run.desc',
  },
  // 票 03（host-crypto）：crypto 三权限位（crypto:aead / asym / kdf）。
  // 密钥由宿主托管，算法运算是中危能力——不进 HIGH_RISK_PERMISSIONS，
  // 按「高危位才带 risk 文案」不变量，不设 riskKey。
  'crypto:aead': {
    emoji: '🔐',
    titleKey: 'desktop.plugin.perm.crypto:aead.title',
    descKey: 'desktop.plugin.perm.crypto:aead.desc',
  },
  'crypto:asym': {
    emoji: '🔑',
    titleKey: 'desktop.plugin.perm.crypto:asym.title',
    descKey: 'desktop.plugin.perm.crypto:asym.desc',
  },
  'crypto:kdf': {
    emoji: '🔁',
    titleKey: 'desktop.plugin.perm.crypto:kdf.title',
    descKey: 'desktop.plugin.perm.crypto:kdf.desc',
  },
}

// ==================== 展示辅助函数 ====================

/** 获取插件的扩展点 chips（仅保留 count > 0 的条目） */
export function getContributionChips(plugin: PluginInfo): ContributionChip[] {
  const chips: ContributionChip[] = []
  for (const [key, kind] of Object.entries(CONTRIBUTION_KINDS)) {
    const n = kind.count(plugin)
    if (n > 0) {
      chips.push({
        key,
        emoji: kind.emoji,
        labelKey: kind.labelKey,
        params: kind.params?.(plugin),
      })
    }
  }
  return chips
}

/** 获取权限元数据（未知权限回退原始字符串） */
export function getPermissionMeta(perm: string): {
  emoji: string
  title: string
  desc: string
  risk?: string
} {
  const t = i18n.global.t
  const meta = PERMISSION_META[perm]
  if (!meta) {
    return { emoji: '🔐', title: perm, desc: t('desktop.plugin.perm.unknown') }
  }
  return {
    emoji: meta.emoji,
    title: t(meta.titleKey),
    desc: t(meta.descKey),
    risk: meta.riskKey ? t(meta.riskKey) : undefined,
  }
}

/**
 * 是否为高危权限位（审批弹层红色强调 + 后果文案）
 *
 * 真源是 [`HIGH_RISK_PERMISSIONS`]；弹层只对命中的位追加风险行，其余位不出现红色元素。
 */
export function isHighRiskPermission(perm: string): boolean {
  return HIGH_RISK_PERMISSIONS.includes(perm)
}

/** 是否为「待人工批准」状态（未确认权限清单，启用前需审批） */
export function isNeedsApproval(state: PluginState): boolean {
  return state.state === 'NeedsApproval'
}

/** 详细信息行（详情页"详细信息"折叠区使用） */
export function getDetailRows(
  plugin: PluginInfo,
): { key: string; label: string; value: string; mono?: boolean }[] {
  const t = i18n.global.t
  return [
    { key: 'id', label: t('desktop.plugin.detail.id'), value: plugin.id, mono: true },
    {
      key: 'source',
      label: t('desktop.plugin.detail.source'),
      value: t(`desktop.plugin.source.${plugin.source}`) || plugin.source,
    },
    { key: 'type', label: t('desktop.plugin.detail.type'), value: plugin.pluginType },
    {
      key: 'entry',
      label: t('desktop.plugin.detail.entry'),
      value: plugin.main || '—',
      mono: true,
    },
    { key: 'size', label: t('desktop.plugin.detail.size'), value: formatBytes(plugin.sizeBytes) },
    {
      key: 'installedAt',
      label: t('desktop.plugin.detail.installedAt'),
      value: formatTime(plugin.installedAt),
    },
  ]
}

/** 获取插件状态 i18n key */
export function getStateKey(state: PluginState): string {
  if (state.state === 'Error') return 'desktop.plugin.error'
  if (state.state === 'Activated') return 'desktop.plugin.activated'
  if (state.state === 'Degraded') return 'desktop.plugin.degraded'
  if (state.state === 'Activating') return 'desktop.plugin.activating'
  if (state.state === 'NeedsApproval') return 'desktop.plugin.needsApproval'
  if (state.state === 'Loaded') return 'desktop.plugin.loaded'
  if (state.state === 'Deactivated') return 'desktop.plugin.deactivated'
  return 'desktop.plugin.loaded'
}

/** 判断插件是否为激活状态 */
export function isActivated(state: PluginState): boolean {
  return state.state === 'Activated'
}

/** 判断插件是否为降级态（activate 成功但 on_startup 失败：实例在运行，启动初始化未完成） */
export function isDegraded(state: PluginState): boolean {
  return state.state === 'Degraded'
}

/** 获取降级原因（on_startup 失败信息） */
export function getDegradedMessage(state: PluginState): string {
  return state.state === 'Degraded' ? state.error || '' : ''
}

/**
 * 判断实例是否在运行（Activated 或 Degraded）。
 *
 * Degraded 实例 activate 成功且 phase 3 扩展点注册已完成，仅 on_startup 失败 ——
 * 对「运行中」语义（列表已启用分区归属、启停开关 ON 态、配置入口放行）应视为运行；
 * 依据 spec §5.1 开放问题裁决：功能门禁先放行 + UI 降级标识。
 * 注意 `is_activated()` 后端 API 门禁仍严格 Activated，本函数只用于前端展示/入口归类。
 */
export function isRunning(state: PluginState): boolean {
  return state.state === 'Activated' || state.state === 'Degraded'
}

/** 判断插件是否为错误状态 */
export function isErrorState(state: PluginState): boolean {
  return state.state === 'Error'
}

/** 错误信息 */
export function getErrorMessage(state: PluginState): string {
  return state.state === 'Error' ? state.error || '' : ''
}

// 字节数 / 时间戳格式化已收敛到 @/utils/format，此处 import + re-export
// 保持既有调用（模块内与 PluginDetailView 等）不断链
import { formatBytes, formatTime } from '@/utils/format'
export { formatBytes, formatTime }

/** 插件是否有可配置项（实例运行中 + 有 configuration 声明；Degraded 放行——用户可能正是要改配置修复启动失败） */
export function hasConfiguration(plugin: PluginInfo): boolean {
  return isRunning(plugin.state) && !!plugin.contributes.configuration
}
