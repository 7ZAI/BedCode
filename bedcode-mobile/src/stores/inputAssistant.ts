import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { AGENT_PRESETS, detectAgentType, type AgentType, type CommandMode } from '@/config/agentPresets'
import { invoke } from '@tauri-apps/api/core'

export interface ShortcutStats {
  [key: string]: number
}

export interface ShortcutItem {
  /** 键码，如 'ctrl+c'、'enter'、'shift+up' */
  code: string
  /** 显示标签，如 'Ctrl+C'、'Enter' */
  label: string
  /** 是否在面板中显示 */
  visible: boolean
  /** 是否为默认快捷键（不可删除，只能隐藏） */
  builtin: boolean
}

/** 面板命令项：命令文本 + 模式（发送/执行）+ builtin 标记（预设命令不可删除） */
export interface QuickCommand {
  id: string
  command: string
  mode: CommandMode
  /** 是否为命令预设命令（不可删除） */
  builtin: boolean
}

/** 快捷键条项：快捷键或快捷命令 */
export interface QuickBarItem {
  type: 'shortcut' | 'custom'
  /** 快捷键 code 或命令 id */
  key: string
  /** 显示标签 */
  label: string
  /** 使用频次 */
  count: number
  /** 颜色分类：enter=绿色, del=红色, arrow=黄色, shortcut=紫色, custom=绿色 */
  category: 'enter' | 'del' | 'arrow' | 'shortcut' | 'custom'
}

export interface InputAssistantSettings {
  size: number
  gestures: {
    doubleTap: boolean
    swipeDown: boolean
    swipeUp: boolean
    swipeLeft: boolean
    swipeRight: boolean
  }
  /** 快捷键条显示数量（3-10） */
  quickBarCount: number
  /** 悬浮球启用开关 */
  floatingBall: boolean
  /** 终端 Header 常驻工具按钮 key 列表（其余收入溢出菜单） */
  headerToolbarItems: string[]
  /** 终端字体大小（10-24） */
  terminalFontSize: number
  /** 终端主题名（dark/light/dracula 等），null 表示跟随外观设置 */
  terminalTheme: string | null
  /** 用户是否手动指定了终端主题（false 时跟随外观设置） */
  isTerminalThemeUserSet: boolean
}

const DEFAULT_SETTINGS: InputAssistantSettings = {
  size: 48,
  gestures: {
    doubleTap: true,
    swipeDown: true,
    swipeUp: true,
    swipeLeft: true,
    swipeRight: true,
  },
  quickBarCount: 6,
  floatingBall: false,
  headerToolbarItems: ['folder'],
  terminalFontSize: 12,
  terminalTheme: null,
  isTerminalThemeUserSet: false,
}

const STORAGE_KEY_STATS = 'terminal_shortcut_stats'
const STORAGE_KEY_POSITION = 'input_assistant_position'
const STORAGE_KEY_SETTINGS = 'input_assistant_settings'
const STORAGE_KEY_CUSTOM_CMD_STATS = 'terminal_custom_cmd_stats'
const STORAGE_KEY_SHORTCUT_CONFIG = 'terminal_shortcut_config'
/** Agent CLI 覆盖映射的 settings DB 键（JSON 文件存储，与 custom_commands 同模式） */
const STORAGE_KEY_AGENT_OVERRIDES = 'agent_type_overrides'

/** 根据快捷键 code 判断颜色分类 */
function getShortcutCategory(code: string): QuickBarItem['category'] {
  if (code === 'enter') return 'enter'
  if (code === 'backspace') return 'del'
  if (code.startsWith('arrow_')) return 'arrow'
  return 'shortcut'
}

/**
 * 默认快捷键列表（builtin，不可删除）
 *
 * 16 个网格键按 agent CLI 场景高频精选（调研 Claude Code / pi / Codex / OpenCode
 * 官方键位，见 ADR-0014）：Esc 中断、Tab/Shift+Tab 补全与模式循环、Ctrl+C/D/L/R
 * 中断退出清屏历史搜索、Ctrl+A/E/K/U/W 行编辑、Ctrl+O/T 工具输出与 thinking、
 * Alt+P 切模型、Ctrl+G 外部编辑器；Ctrl+Z（挂起）对移动端无意义且 codex 保留给终端，移除。
 * Enter/Del 为固定按钮，由中间区域独立渲染，同样 builtin。
 */
const DEFAULT_SHORTCUTS: ShortcutItem[] = [
  { code: 'escape', label: 'Esc', visible: true, builtin: true },
  { code: 'tab', label: 'Tab', visible: true, builtin: true },
  { code: 'shift+tab', label: 'Shift+Tab', visible: true, builtin: true },
  { code: 'ctrl+c', label: 'Ctrl+C', visible: true, builtin: true },
  { code: 'ctrl+d', label: 'Ctrl+D', visible: true, builtin: true },
  { code: 'ctrl+l', label: 'Ctrl+L', visible: true, builtin: true },
  { code: 'ctrl+r', label: 'Ctrl+R', visible: true, builtin: true },
  { code: 'ctrl+a', label: 'Ctrl+A', visible: true, builtin: true },
  { code: 'ctrl+e', label: 'Ctrl+E', visible: true, builtin: true },
  { code: 'ctrl+k', label: 'Ctrl+K', visible: true, builtin: true },
  { code: 'ctrl+u', label: 'Ctrl+U', visible: true, builtin: true },
  { code: 'ctrl+w', label: 'Ctrl+W', visible: true, builtin: true },
  { code: 'ctrl+o', label: 'Ctrl+O', visible: true, builtin: true },
  { code: 'ctrl+t', label: 'Ctrl+T', visible: true, builtin: true },
  { code: 'alt+p', label: 'Alt+P', visible: true, builtin: true },
  { code: 'ctrl+g', label: 'Ctrl+G', visible: true, builtin: true },
  { code: 'enter', label: 'Enter', visible: true, builtin: true },
  { code: 'backspace', label: 'Del', visible: true, builtin: true },
]

/** 快捷键 code → 显示标签映射 */
const SHORTCUT_LABELS: Record<string, string> = {
  tab: 'Tab',
  enter: 'Enter',
  escape: 'Esc',
  backspace: 'Del',
  'shift+tab': 'Shift+Tab',
  'ctrl+c': 'Ctrl+C',
  'ctrl+d': 'Ctrl+D',
  'ctrl+l': 'Ctrl+L',
  'ctrl+r': 'Ctrl+R',
  'ctrl+a': 'Ctrl+A',
  'ctrl+e': 'Ctrl+E',
  'ctrl+k': 'Ctrl+K',
  'ctrl+u': 'Ctrl+U',
  'ctrl+w': 'Ctrl+W',
  'ctrl+o': 'Ctrl+O',
  'ctrl+t': 'Ctrl+T',
  'alt+p': 'Alt+P',
  'ctrl+g': 'Ctrl+G',
  // 旧格式兼容（历史频次统计数据）
  ctrl_c: 'Ctrl+C',
  ctrl_z: 'Ctrl+Z',
  ctrl_l: 'Ctrl+L',
  ctrl_d: 'Ctrl+D',
  arrow_up: '↑',
  arrow_down: '↓',
  arrow_left: '←',
  arrow_right: '→',
  home: 'Home',
  end: 'End',
  page_up: 'PgUp',
  page_down: 'PgDn',
}

/** 无统计数据时的默认快捷键（按 agent CLI 场景常用程度排序；不含 Enter/Del，它们固定显示在最右） */
const DEFAULT_QUICK_KEYS = ['escape', 'tab', 'shift+tab', 'ctrl+c', 'ctrl+o', 'ctrl+t', 'ctrl+l', 'arrow_up', 'ctrl+r']

export const useInputAssistantStore = defineStore('inputAssistant', () => {
  // 悬浮球位置
  const position = ref<{ x: number; y: number }>({ x: -1, y: -1 })

  // 功能菜单展开状态
  const isExpanded = ref(false)

  // 快捷键使用频次
  const shortcutStats = ref<ShortcutStats>({})

  // 自定义命令使用频次
  const customCommandStats = ref<ShortcutStats>({})

  // 设置配置
  const settings = ref<InputAssistantSettings>({ ...DEFAULT_SETTINGS })

  // 快捷键配置
  const shortcutConfig = ref<ShortcutItem[]>(DEFAULT_SHORTCUTS.map(s => ({ ...s })))

  // ==================== Agent CLI 预设 ====================

  /** 当前会话的 Agent CLI（generic = 未识别，不加载预设） */
  const activeAgentType = ref<AgentType>('generic')

  /** 当前生效的命令预设（builtin 快捷命令，按 Agent CLI 切换整体覆盖） */
  const presetCommands = ref<QuickCommand[]>([])

  /** Agent CLI 覆盖映射（会话配置 id → AgentType，存移动端 JSON 文件） */
  const agentTypeOverrides = ref<Record<string, AgentType>>({})

  /** 从 settings DB（JSON 文件）加载覆盖映射 */
  async function loadAgentTypeOverrides() {
    try {
      const settings = await invoke<{ key: string; value: string }[]>('get_all_db_settings_mobile')
      const found = settings?.find(s => s.key === STORAGE_KEY_AGENT_OVERRIDES)
      if (found?.value) {
        agentTypeOverrides.value = JSON.parse(found.value)
      }
    } catch {
      agentTypeOverrides.value = {}
    }
  }

  /** 保存覆盖映射到 settings DB（JSON 文件） */
  async function saveAgentTypeOverrides() {
    try {
      await invoke('set_db_setting_mobile', {
        key: STORAGE_KEY_AGENT_OVERRIDES,
        value: JSON.stringify(agentTypeOverrides.value),
      })
    } catch (e) {
      console.error('[inputAssistant] Failed to save agent type overrides:', e)
    }
  }

  /**
   * 获取会话配置的有效 Agent CLI：手动覆盖优先，否则按启动命令关键词识别
   *
   * @param configId 会话配置 id（覆盖映射键）
   * @param command 会话配置启动命令（识别输入）
   */
  function getEffectiveAgentType(configId: string | undefined, command: string): AgentType {
    if (configId && agentTypeOverrides.value[configId]) {
      return agentTypeOverrides.value[configId]
    }
    return detectAgentType(command)
  }

  /** 为会话配置手动指定 Agent CLI（覆盖识别结果） */
  async function setAgentTypeOverride(configId: string, type: AgentType) {
    agentTypeOverrides.value[configId] = type
    await saveAgentTypeOverrides()
  }

  /**
   * 应用 Agent CLI 命令预设（整体覆盖 presetCommands，不做合并）
   *
   * generic 清空预设（保留用户自定义命令，行为与现状一致）；
   * 预设命令带 builtin 标记，不可删除。
   */
  function setAgentPreset(type: AgentType) {
    activeAgentType.value = type
    if (type === 'generic' || !AGENT_PRESETS[type]) {
      presetCommands.value = []
      return
    }
    presetCommands.value = AGENT_PRESETS[type].map((cmd, idx) => ({
      id: `preset-${type}-${idx}`,
      command: cmd.command,
      mode: cmd.mode,
      builtin: true,
    }))
  }

  // 从 localStorage 加载数据
  function loadFromStorage() {
    try {
      // 加载位置
      const savedPosition = localStorage.getItem(STORAGE_KEY_POSITION)
      if (savedPosition) {
        position.value = JSON.parse(savedPosition)
      }

      // 加载频次统计
      const savedStats = localStorage.getItem(STORAGE_KEY_STATS)
      if (savedStats) {
        shortcutStats.value = JSON.parse(savedStats)
      }

      // 加载自定义命令频次统计
      const savedCmdStats = localStorage.getItem(STORAGE_KEY_CUSTOM_CMD_STATS)
      if (savedCmdStats) {
        customCommandStats.value = JSON.parse(savedCmdStats)
      }

      // 加载设置
      const savedSettings = localStorage.getItem(STORAGE_KEY_SETTINGS)
      if (savedSettings) {
        settings.value = { ...DEFAULT_SETTINGS, ...JSON.parse(savedSettings) }
      }

      // 加载快捷键配置
      loadShortcutConfig()
    } catch (e) {
      console.error('Failed to load input assistant storage:', e)
    }
  }

  // 保存位置到 localStorage
  function savePosition(x: number, y: number) {
    position.value = { x, y }
    localStorage.setItem(STORAGE_KEY_POSITION, JSON.stringify({ x, y }))
  }

  // 记录快捷键使用
  function recordShortcut(key: string) {
    const current = shortcutStats.value[key] || 0
    shortcutStats.value[key] = current + 1
    localStorage.setItem(STORAGE_KEY_STATS, JSON.stringify(shortcutStats.value))
  }

  // 记录自定义命令使用
  function recordCustomCommand(id: string) {
    const current = customCommandStats.value[id] || 0
    customCommandStats.value[id] = current + 1
    localStorage.setItem(STORAGE_KEY_CUSTOM_CMD_STATS, JSON.stringify(customCommandStats.value))
  }

  // 保存设置
  function saveSettings(newSettings: Partial<InputAssistantSettings>) {
    settings.value = { ...settings.value, ...newSettings }
    localStorage.setItem(STORAGE_KEY_SETTINGS, JSON.stringify(settings.value))
  }

  // 重置设置
  function resetSettings() {
    settings.value = { ...DEFAULT_SETTINGS }
    localStorage.setItem(STORAGE_KEY_SETTINGS, JSON.stringify(settings.value))
  }

  // ==================== Shortcut Config ====================

  /** 从 localStorage 加载快捷键配置，合并新增的默认快捷键 */
  function loadShortcutConfig() {
    const saved = localStorage.getItem(STORAGE_KEY_SHORTCUT_CONFIG)
    if (saved) {
      try {
        const parsed: ShortcutItem[] = JSON.parse(saved)
        // 合并策略：保留用户的 visible 设置和自定义快捷键，补充新增的默认快捷键
        const existingCodes = new Set(parsed.map(s => s.code))
        const merged = [...parsed]
        for (const def of DEFAULT_SHORTCUTS) {
          if (!existingCodes.has(def.code)) {
            merged.push({ ...def })
          }
        }
        shortcutConfig.value = merged
      } catch {
        shortcutConfig.value = DEFAULT_SHORTCUTS.map(s => ({ ...s }))
      }
    }
  }

  /** 持久化快捷键配置到 localStorage */
  function saveShortcutConfig() {
    localStorage.setItem(STORAGE_KEY_SHORTCUT_CONFIG, JSON.stringify(shortcutConfig.value))
  }

  /** 添加自定义快捷键 */
  function addShortcut(code: string, label: string) {
    if (shortcutConfig.value.some(s => s.code === code)) return
    shortcutConfig.value.push({ code, label, visible: true, builtin: false })
    saveShortcutConfig()
  }

  /** 删除自定义快捷键（builtin 不可删除） */
  function removeShortcut(code: string) {
    const idx = shortcutConfig.value.findIndex(s => s.code === code)
    if (idx === -1 || shortcutConfig.value[idx].builtin) return
    shortcutConfig.value.splice(idx, 1)
    saveShortcutConfig()
  }

  /** 切换快捷键显示/隐藏 */
  function toggleShortcutVisibility(code: string) {
    const item = shortcutConfig.value.find(s => s.code === code)
    if (item) {
      item.visible = !item.visible
      saveShortcutConfig()
    }
  }

  /** 获取面板中可见的快捷键（不含 Enter/Del，它们由中间区域独立渲染） */
  const visiblePanelShortcuts = computed(() =>
    shortcutConfig.value.filter(s => s.visible && s.code !== 'enter' && s.code !== 'backspace')
  )

  /** 重置快捷键配置为默认 */
  function resetShortcutConfig() {
    shortcutConfig.value = DEFAULT_SHORTCUTS.map(s => ({ ...s }))
    saveShortcutConfig()
  }

  // 获取高频快捷键（top 3，兼容旧用法）
  const topShortcuts = computed(() => {
    return Object.entries(shortcutStats.value)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([key]) => key)
  })

  /**
   * 获取快捷键条项目：合并快捷键和自定义命令，按频次排序取 top N；
   * Enter/Del 不参与频次排序，恒固定显示在最右（配合 quick-bar RTL 布局，DOM 首位渲染在最右）
   *
   * @param customCommands - 当前自定义命令列表（需要传入以获取命令文本作为 label）
   * @returns 排序后的 QuickBarItem 列表（[enter, del, ...频次项]）
   */
  function getQuickBarItems(customCommands: { id: string; command: string }[]): QuickBarItem[] {
    const count = Math.max(3, Math.min(10, settings.value.quickBarCount))

    // 收集快捷键项（排除 Enter/Del：固定项不参与频次统计排序）
    const shortcutItems: QuickBarItem[] = Object.entries(shortcutStats.value)
      .filter(([key]) => key !== 'enter' && key !== 'backspace')
      .map(([key, cnt]) => ({
        type: 'shortcut' as const,
        key,
        label: SHORTCUT_LABELS[key] || key,
        count: cnt,
        category: getShortcutCategory(key),
      }))

    // 收集自定义命令项
    const cmdItems: QuickBarItem[] = customCommands
      .map(cmd => ({
        type: 'custom' as const,
        key: cmd.id,
        label: cmd.command,
        count: customCommandStats.value[cmd.id] || 0,
        category: 'custom' as const,
      }))

    // 合并排序：按频次降序，最常用的排在前面（配合 quick-bar RTL 布局，首项渲染在最右）
    const all = [...shortcutItems, ...cmdItems]
      .sort((a, b) => b.count - a.count)

    // 有统计数据时取 top N；无统计数据时返回默认快捷键（降序：最常用的首项渲染在最右）
    const pool: QuickBarItem[] = all.some(item => item.count > 0)
      ? all.slice(0, count)
      : DEFAULT_QUICK_KEYS.slice(0, count).map(key => ({
          type: 'shortcut' as const,
          key,
          label: SHORTCUT_LABELS[key] || key,
          count: 0,
          category: getShortcutCategory(key),
        }))

    // 固定项：Enter/Del 恒显示在最右（RTL 布局下 DOM 首位渲染在最右）
    return [
      { type: 'shortcut', key: 'enter', label: SHORTCUT_LABELS.enter, count: 0, category: 'enter' },
      { type: 'shortcut', key: 'backspace', label: SHORTCUT_LABELS.backspace, count: 0, category: 'del' },
      ...pool,
    ]
  }

  // 切换展开状态
  function toggleExpanded() {
    isExpanded.value = !isExpanded.value
  }

  // 收起菜单
  function collapse() {
    isExpanded.value = false
  }

  // 初始化
  loadFromStorage()

  return {
    position,
    isExpanded,
    shortcutStats,
    customCommandStats,
    topShortcuts,
    settings,
    savePosition,
    recordShortcut,
    recordCustomCommand,
    toggleExpanded,
    collapse,
    loadFromStorage,
    saveSettings,
    resetSettings,
    getQuickBarItems,
    shortcutConfig,
    visiblePanelShortcuts,
    addShortcut,
    removeShortcut,
    toggleShortcutVisibility,
    resetShortcutConfig,
    activeAgentType,
    presetCommands,
    agentTypeOverrides,
    loadAgentTypeOverrides,
    getEffectiveAgentType,
    setAgentTypeOverride,
    setAgentPreset,
  }
})
