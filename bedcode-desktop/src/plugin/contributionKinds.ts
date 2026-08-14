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
    count: (p) => p.contributes.views?.filter(v => v.type === 'sidebar').length ?? 0,
  },
  toolbox: {
    emoji: '🧰',
    labelKey: 'desktop.plugin.chip.toolbox',
    count: (p) => p.contributes.views?.filter(v => v.type === 'toolbox').length ?? 0,
  },
  statusbar: {
    emoji: '📊',
    labelKey: 'desktop.plugin.chip.statusbar',
    count: (p) => p.contributes.views?.filter(v => v.type === 'statusbar').length ?? 0,
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
    count: (p) => p.contributes.terminal ? 1 : 0,
  },
  toolProviders: {
    emoji: '🛠️',
    labelKey: 'desktop.plugin.chip.toolProviders',
    count: (p) => p.contributes.toolProviders?.length ?? 0,
    params: (p) => ({ count: p.contributes.toolProviders?.length ?? 0 }),
  },
  fileHandlers: {
    emoji: '📁',
    labelKey: 'desktop.plugin.chip.fileHandlers',
    count: (p) => p.contributes.fileHandlers?.length ?? 0,
    params: (p) => ({ count: p.contributes.fileHandlers?.length ?? 0 }),
  },
  configuration: {
    emoji: '🎛️',
    labelKey: 'desktop.plugin.chip.configuration',
    count: (p) => p.contributes.configuration ? 1 : 0,
  },
  lifecycle: {
    emoji: '🔄',
    labelKey: 'desktop.plugin.chip.lifecycle',
    count: (p) => p.contributes.lifecycle ? 1 : 0,
  },
}

// ==================== 权限元数据 ====================

/** 权限元数据项 */
export interface PermissionMeta {
  emoji: string
  titleKey: string
  descKey: string
}

/** 14 项桌面端权限元数据注册表 */
const PERMISSION_META: Record<string, PermissionMeta> = {
  storage: { emoji: '💾', titleKey: 'desktop.plugin.perm.storage.title', descKey: 'desktop.plugin.perm.storage.desc' },
  'terminal:input': { emoji: '⌨️', titleKey: 'desktop.plugin.perm.terminal:input.title', descKey: 'desktop.plugin.perm.terminal:input.desc' },
  'terminal:output': { emoji: '📺', titleKey: 'desktop.plugin.perm.terminal:output.title', descKey: 'desktop.plugin.perm.terminal:output.desc' },
  'terminal:observe': { emoji: '👁️', titleKey: 'desktop.plugin.perm.terminal:observe.title', descKey: 'desktop.plugin.perm.terminal:observe.desc' },
  'session:read': { emoji: '📄', titleKey: 'desktop.plugin.perm.session:read.title', descKey: 'desktop.plugin.perm.session:read.desc' },
  'session:write': { emoji: '✏️', titleKey: 'desktop.plugin.perm.session:write.title', descKey: 'desktop.plugin.perm.session:write.desc' },
  'ui:sidebar': { emoji: '📋', titleKey: 'desktop.plugin.perm.ui:sidebar.title', descKey: 'desktop.plugin.perm.ui:sidebar.desc' },
  'ui:input': { emoji: '🔤', titleKey: 'desktop.plugin.perm.ui:input.title', descKey: 'desktop.plugin.perm.ui:input.desc' },
  'ui:toolbox': { emoji: '🧰', titleKey: 'desktop.plugin.perm.ui:toolbox.title', descKey: 'desktop.plugin.perm.ui:toolbox.desc' },
  'network:http': { emoji: '🌐', titleKey: 'desktop.plugin.perm.network:http.title', descKey: 'desktop.plugin.perm.network:http.desc' },
  'fs:read': { emoji: '📂', titleKey: 'desktop.plugin.perm.fs:read.title', descKey: 'desktop.plugin.perm.fs:read.desc' },
  'fs:write': { emoji: '📝', titleKey: 'desktop.plugin.perm.fs:write.title', descKey: 'desktop.plugin.perm.fs:write.desc' },
  fileservice: { emoji: '🗂️', titleKey: 'desktop.plugin.perm.fileservice.title', descKey: 'desktop.plugin.perm.fileservice.desc' },
  broadcast: { emoji: '📩', titleKey: 'desktop.plugin.perm.broadcast.title', descKey: 'desktop.plugin.perm.broadcast.desc' },
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
export function getPermissionMeta(perm: string): { emoji: string; title: string; desc: string } {
  const t = i18n.global.t
  const meta = PERMISSION_META[perm]
  if (!meta) {
    return { emoji: '🔐', title: perm, desc: t('desktop.plugin.perm.unknown') }
  }
  return { emoji: meta.emoji, title: t(meta.titleKey), desc: t(meta.descKey) }
}

/** 详细信息行（详情页"详细信息"折叠区使用） */
export function getDetailRows(plugin: PluginInfo): { key: string; label: string; value: string; mono?: boolean }[] {
  const t = i18n.global.t
  return [
    { key: 'id', label: t('desktop.plugin.detail.id'), value: plugin.id, mono: true },
    { key: 'source', label: t('desktop.plugin.detail.source'), value: t(`desktop.plugin.source.${plugin.source}`) || plugin.source },
    { key: 'type', label: t('desktop.plugin.detail.type'), value: plugin.pluginType },
    { key: 'entry', label: t('desktop.plugin.detail.entry'), value: plugin.main || '—', mono: true },
    { key: 'size', label: t('desktop.plugin.detail.size'), value: formatBytes(plugin.sizeBytes) },
    { key: 'installedAt', label: t('desktop.plugin.detail.installedAt'), value: formatTime(plugin.installedAt) },
  ]
}

/** 获取插件状态 i18n key */
export function getStateKey(state: PluginState): string {
  if (state.state === 'Error') return 'desktop.plugin.error'
  if (state.state === 'Activated') return 'desktop.plugin.activated'
  if (state.state === 'NeedsApproval') return 'desktop.plugin.needsApproval'
  if (state.state === 'Loaded') return 'desktop.plugin.loaded'
  if (state.state === 'Deactivated') return 'desktop.plugin.deactivated'
  return 'desktop.plugin.loaded'
}

/** 判断插件是否为激活状态 */
export function isActivated(state: PluginState): boolean {
  return state.state === 'Activated'
}

/** 判断插件是否为错误状态 */
export function isErrorState(state: PluginState): boolean {
  return state.state === 'Error'
}

/** 获取错误信息 */
export function getErrorMessage(state: PluginState): string {
  return state.state === 'Error' ? state.error || '' : ''
}

/** 字节数格式化 */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

/** unix 毫秒时间戳格式化，缺失时显示 '—' */
export function formatTime(ms?: number): string {
  if (!ms) return '—'
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 插件是否有可配置项（激活 + 有 configuration 声明） */
export function hasConfiguration(plugin: PluginInfo): boolean {
  return isActivated(plugin.state) && !!plugin.contributes.configuration
}
