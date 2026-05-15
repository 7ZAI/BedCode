import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface ShortcutStats {
  [key: string]: number
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
}

const DEFAULT_SETTINGS: InputAssistantSettings = {
  size: 48,
  gestures: {
    doubleTap: true,
    swipeDown: true,
    swipeUp: true,
    swipeLeft: true,
    swipeRight: true,
  }
}

const STORAGE_KEY_STATS = 'terminal_shortcut_stats'
const STORAGE_KEY_POSITION = 'input_assistant_position'
const STORAGE_KEY_SETTINGS = 'input_assistant_settings'

export const useInputAssistantStore = defineStore('inputAssistant', () => {
  // 悬浮球位置
  const position = ref<{ x: number; y: number }>({ x: -1, y: -1 })

  // 功能菜单展开状态
  const isExpanded = ref(false)

  // 快捷键使用频次
  const shortcutStats = ref<ShortcutStats>({})

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

  // 获取高频快捷键（top 3）
  const topShortcuts = computed(() => {
    return Object.entries(shortcutStats.value)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([key]) => key)
  })

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
    topShortcuts,
    settings,
    savePosition,
    recordShortcut,
    toggleExpanded,
    collapse,
    loadFromStorage,
    saveSettings,
    resetSettings,
  }
})
