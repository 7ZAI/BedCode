import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useInputAssistantStore } from '@/stores/inputAssistant'

// localStorage 存储契约 key（与 store 内部常量一致，用于预置/断言持久化）
const KEY_POSITION = 'input_assistant_position'
const KEY_STATS = 'terminal_shortcut_stats'
const KEY_CMD_STATS = 'terminal_custom_cmd_stats'
const KEY_SETTINGS = 'input_assistant_settings'
const KEY_SHORTCUT_CONFIG = 'terminal_shortcut_config'

/** 内置快捷键数量（DEFAULT_SHORTCUTS） */
const BUILTIN_COUNT = 12

describe('Input Assistant Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  describe('initial state', () => {
    it('should initialize with default settings and builtin shortcuts', () => {
      const store = useInputAssistantStore()

      expect(store.settings.size).toBe(48)
      expect(store.settings.quickBarCount).toBe(6)
      expect(store.settings.floatingBall).toBe(false)
      expect(store.settings.headerToolbarItems).toEqual(['folder'])
      expect(store.settings.gestures).toEqual({
        doubleTap: true,
        swipeDown: true,
        swipeUp: true,
        swipeLeft: true,
        swipeRight: true,
      })
      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)
      expect(store.shortcutConfig.every(s => s.builtin && s.visible)).toBe(true)
      expect(store.position).toEqual({ x: -1, y: -1 })
      expect(store.isExpanded).toBe(false)
      expect(store.shortcutStats).toEqual({})
    })
  })

  describe('position', () => {
    it('savePosition should update position and persist to localStorage', () => {
      const store = useInputAssistantStore()

      store.savePosition(120, 240)

      expect(store.position).toEqual({ x: 120, y: 240 })
      expect(JSON.parse(localStorage.getItem(KEY_POSITION) || '{}')).toEqual({ x: 120, y: 240 })
    })
  })

  describe('usage stats', () => {
    it('recordShortcut should increment counters and persist', () => {
      const store = useInputAssistantStore()

      store.recordShortcut('tab')
      store.recordShortcut('tab')
      store.recordShortcut('enter')

      expect(store.shortcutStats).toEqual({ tab: 2, enter: 1 })
      expect(JSON.parse(localStorage.getItem(KEY_STATS) || '{}')).toEqual({ tab: 2, enter: 1 })
    })

    it('recordCustomCommand should increment counters and persist', () => {
      const store = useInputAssistantStore()

      store.recordCustomCommand('cmd-1')

      expect(store.customCommandStats).toEqual({ 'cmd-1': 1 })
      expect(JSON.parse(localStorage.getItem(KEY_CMD_STATS) || '{}')).toEqual({ 'cmd-1': 1 })
    })

    it('topShortcuts should return top 3 keys by count descending', () => {
      const store = useInputAssistantStore()
      store.shortcutStats = { a: 1, b: 5, c: 3, d: 2 }

      expect(store.topShortcuts).toEqual(['b', 'c', 'd'])
    })
  })

  describe('settings', () => {
    it('saveSettings should merge partial settings and persist', () => {
      const store = useInputAssistantStore()

      store.saveSettings({ floatingBall: true, quickBarCount: 8 })

      expect(store.settings.floatingBall).toBe(true)
      expect(store.settings.quickBarCount).toBe(8)
      expect(store.settings.size).toBe(48) // 未覆盖字段保留默认
      expect(JSON.parse(localStorage.getItem(KEY_SETTINGS) || '{}').floatingBall).toBe(true)
    })

    it('resetSettings should restore defaults and persist', () => {
      const store = useInputAssistantStore()
      store.saveSettings({ floatingBall: true, quickBarCount: 10 })

      store.resetSettings()

      expect(store.settings.floatingBall).toBe(false)
      expect(store.settings.quickBarCount).toBe(6)
      expect(store.settings.size).toBe(48)
      expect(JSON.parse(localStorage.getItem(KEY_SETTINGS) || '{}').floatingBall).toBe(false)
    })
  })

  describe('shortcut config', () => {
    it('addShortcut should append a custom shortcut and persist', () => {
      const store = useInputAssistantStore()

      store.addShortcut('f1', 'F1')

      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT + 1)
      expect(store.shortcutConfig[BUILTIN_COUNT]).toEqual({
        code: 'f1',
        label: 'F1',
        visible: true,
        builtin: false,
      })
      const persisted = JSON.parse(localStorage.getItem(KEY_SHORTCUT_CONFIG) || '[]')
      expect(persisted.some(s => s.code === 'f1' && !s.builtin)).toBe(true)
    })

    it('addShortcut should ignore duplicate codes', () => {
      const store = useInputAssistantStore()

      store.addShortcut('tab', 'Tab')
      store.addShortcut('f1', 'F1')
      store.addShortcut('f1', 'F1 again')

      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT + 1)
    })

    it('removeShortcut should remove custom shortcuts but keep builtin ones', () => {
      const store = useInputAssistantStore()
      store.addShortcut('f1', 'F1')

      store.removeShortcut('f1')
      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)

      store.removeShortcut('tab') // builtin 不可删除
      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)

      store.removeShortcut('not-exists')
      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)
    })

    it('toggleShortcutVisibility should flip visibility and persist', () => {
      const store = useInputAssistantStore()

      store.toggleShortcutVisibility('tab')

      expect(store.shortcutConfig.find(s => s.code === 'tab')?.visible).toBe(false)
      const persisted = JSON.parse(localStorage.getItem(KEY_SHORTCUT_CONFIG) || '[]')
      expect(persisted.find(s => s.code === 'tab').visible).toBe(false)

      store.toggleShortcutVisibility('tab')
      expect(store.shortcutConfig.find(s => s.code === 'tab')?.visible).toBe(true)
    })

    it('resetShortcutConfig should restore all builtin shortcuts as visible', () => {
      const store = useInputAssistantStore()
      store.toggleShortcutVisibility('tab')
      store.addShortcut('f1', 'F1')

      store.resetShortcutConfig()

      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)
      expect(store.shortcutConfig.every(s => s.builtin && s.visible)).toBe(true)
    })

    it('visiblePanelShortcuts should exclude hidden, enter and backspace', () => {
      const store = useInputAssistantStore()
      store.toggleShortcutVisibility('tab')

      const visible = store.visiblePanelShortcuts

      expect(visible).toHaveLength(BUILTIN_COUNT - 3) // tab 隐藏 + enter + backspace
      expect(visible.some(s => s.code === 'tab')).toBe(false)
      expect(visible.some(s => s.code === 'enter')).toBe(false)
      expect(visible.some(s => s.code === 'backspace')).toBe(false)
    })
  })

  describe('getQuickBarItems', () => {
    it('should merge shortcuts and custom commands sorted by count ascending', () => {
      const store = useInputAssistantStore()
      store.shortcutStats = { tab: 2, ctrl_c: 5 }
      store.customCommandStats = { 'cmd-1': 3 }

      const items = store.getQuickBarItems([{ id: 'cmd-1', command: 'ls -la' }])

      expect(items.map(i => i.key)).toEqual(['tab', 'cmd-1', 'ctrl_c'])
      expect(items[1]).toEqual({ type: 'custom', key: 'cmd-1', label: 'ls -la', count: 3 })
      expect(items[2]).toEqual({ type: 'shortcut', key: 'ctrl_c', label: 'Ctrl+C', count: 5 })
    })

    it('should fall back to default quick keys when no stats recorded', () => {
      const store = useInputAssistantStore()

      const items = store.getQuickBarItems([])

      // DEFAULT_QUICK_KEYS 前 6 个反序（最常用的在最右）
      expect(items.map(i => i.key)).toEqual(['arrow_up', 'ctrl_z', 'ctrl_c', 'escape', 'enter', 'tab'])
      expect(items.every(i => i.count === 0 && i.type === 'shortcut')).toBe(true)
    })

    it('should clamp quickBarCount to [3, 10]', () => {
      const store = useInputAssistantStore()
      store.saveSettings({ quickBarCount: 1 })
      expect(store.getQuickBarItems([])).toHaveLength(3)

      store.saveSettings({ quickBarCount: 20 })
      expect(store.getQuickBarItems([])).toHaveLength(10)
    })

    it('should keep only top-N items when stats exist', () => {
      const store = useInputAssistantStore()
      store.shortcutStats = { tab: 1, enter: 2, escape: 3, ctrl_c: 4, ctrl_d: 5, ctrl_z: 6, ctrl_l: 7 }
      store.saveSettings({ quickBarCount: 4 })

      const items = store.getQuickBarItems([])

      // 频次最高的 4 个，升序排列
      expect(items.map(i => i.key)).toEqual(['ctrl_c', 'ctrl_d', 'ctrl_z', 'ctrl_l'])
    })
  })

  describe('expand state', () => {
    it('toggleExpanded should flip state and collapse should close it', () => {
      const store = useInputAssistantStore()

      store.toggleExpanded()
      expect(store.isExpanded).toBe(true)
      store.toggleExpanded()
      expect(store.isExpanded).toBe(false)

      store.toggleExpanded()
      store.collapse()
      expect(store.isExpanded).toBe(false)
    })
  })

  describe('loadFromStorage', () => {
    it('should restore all persisted state from localStorage', () => {
      localStorage.setItem(KEY_POSITION, JSON.stringify({ x: 10, y: 20 }))
      localStorage.setItem(KEY_STATS, JSON.stringify({ tab: 7 }))
      localStorage.setItem(KEY_CMD_STATS, JSON.stringify({ 'cmd-9': 4 }))
      localStorage.setItem(KEY_SETTINGS, JSON.stringify({ floatingBall: true, quickBarCount: 9 }))
      localStorage.setItem(KEY_SHORTCUT_CONFIG, JSON.stringify([
        { code: 'custom-a', label: 'Custom A', visible: false, builtin: false },
      ]))

      // 重新创建 store 实例触发 loadFromStorage
      setActivePinia(createPinia())
      const store = useInputAssistantStore()

      expect(store.position).toEqual({ x: 10, y: 20 })
      expect(store.shortcutStats).toEqual({ tab: 7 })
      expect(store.customCommandStats).toEqual({ 'cmd-9': 4 })
      expect(store.settings.floatingBall).toBe(true)
      expect(store.settings.quickBarCount).toBe(9)
      expect(store.settings.size).toBe(48) // 未持久化字段回退默认
      // 自定义快捷键保留，且缺失的内置快捷键被合并补齐
      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT + 1)
      expect(store.shortcutConfig.find(s => s.code === 'custom-a')).toEqual({
        code: 'custom-a',
        label: 'Custom A',
        visible: false,
        builtin: false,
      })
      expect(store.shortcutConfig.filter(s => s.code === 'tab')).toHaveLength(1)
    })

    it('should fall back to defaults when saved shortcut config is corrupted', () => {
      localStorage.setItem(KEY_SHORTCUT_CONFIG, '{not-json{{')

      setActivePinia(createPinia())
      const store = useInputAssistantStore()

      expect(store.shortcutConfig).toHaveLength(BUILTIN_COUNT)
      expect(store.shortcutConfig.every(s => s.builtin)).toBe(true)
    })

    it('should keep existing state when no saved data exists', () => {
      const store = useInputAssistantStore()
      store.recordShortcut('tab')

      store.loadFromStorage()

      expect(store.shortcutStats).toEqual({ tab: 1 })
      expect(store.position).toEqual({ x: -1, y: -1 })
    })
  })
})
