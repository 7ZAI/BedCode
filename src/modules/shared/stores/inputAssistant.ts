import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface ShortcutStats {
  [key: string]: number
}

/** 快捷键条项：快捷键或自定义命令 */
export interface QuickBarItem {
  type: 'shortcut' | 'custom'
  /** 快捷键 code 或自定义命令 id */
  key: string
  /** 显示标签 */
  label: string
  /** 使用频次 */
  count: number
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
}

const STORAGE_KEY_STATS = 'terminal_shortcut_stats'
const STORAGE_KEY_POSITION = 'input_assistant_position'
const STORAGE_KEY_SETTINGS = 'input_assistant_settings'
const STORAGE_KEY_CUSTOM_CMD_STATS = 'terminal_custom_cmd_stats'

/** 快捷键 code → 显示标签映射 */
const SHORTCUT_LABELS: Record<string, string> = {
  tab: 'Tab',
  enter: 'Enter',
  escape: 'Esc',
  backspace: 'Del',
  ctrl_c: 'Ctrl+C',
  ctrl_z: 'Ctrl+Z',
  ctrl_l: 'Ctrl+L',
  ctrl_d: 'Ctrl+D',
  ctrl_a: 'Ctrl+A',
  ctrl_e: 'Ctrl+E',
  ctrl_r: 'Ctrl+R',
  ctrl_u: 'Ctrl+U',
  ctrl_k: 'Ctrl+K',
  arrow_up: '↑',
  arrow_down: '↓',
  arrow_left: '←',
  arrow_right: '→',
  home: 'Home',
  end: 'End',
  page_up: 'PgUp',
  page_down: 'PgDn',
}

/** 无统计数据时的默认快捷键（按常用程度排序） */
const DEFAULT_QUICK_KEYS = ['tab', 'enter', 'escape', 'ctrl_c', 'ctrl_z', 'arrow_up', 'ctrl_d', 'ctrl_l', 'ctrl_a', 'ctrl_e']

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

  // 获取高频快捷键（top 3，兼容旧用法）
  const topShortcuts = computed(() => {
    return Object.entries(shortcutStats.value)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([key]) => key)
  })

  /**
   * 获取快捷键条项目：合并快捷键和自定义命令，按频次排序取 top N
   *
   * @param customCommands - 当前自定义命令列表（需要传入以获取命令文本作为 label）
   * @returns 排序后的 QuickBarItem 列表
   */
  function getQuickBarItems(customCommands: { id: string; command: string }[]): QuickBarItem[] {
    const count = Math.max(3, Math.min(10, settings.value.quickBarCount))

    // 收集快捷键项
    const shortcutItems: QuickBarItem[] = Object.entries(shortcutStats.value)
      .map(([key, cnt]) => ({
        type: 'shortcut' as const,
        key,
        label: SHORTCUT_LABELS[key] || key,
        count: cnt,
      }))

    // 收集自定义命令项
    const cmdItems: QuickBarItem[] = customCommands
      .map(cmd => ({
        type: 'custom' as const,
        key: cmd.id,
        label: cmd.command,
        count: customCommandStats.value[cmd.id] || 0,
      }))

    // 合并排序：按频次升序，最常用的排在末尾（右侧），方便拇指操作
    const all = [...shortcutItems, ...cmdItems]
      .sort((a, b) => a.count - b.count)

    // 有统计数据时取 top N
    if (all.some(item => item.count > 0)) {
      return all.slice(-count)
    }

    // 无统计数据时返回默认快捷键（升序：最常用的在右）
    return DEFAULT_QUICK_KEYS.slice(0, count).reverse().map(key => ({
      type: 'shortcut' as const,
      key,
      label: SHORTCUT_LABELS[key] || key,
      count: 0,
    }))
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
  }
})
