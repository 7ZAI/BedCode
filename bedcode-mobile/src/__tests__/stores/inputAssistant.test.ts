/**
 * inputAssistant store 单元测试
 *
 * 移动端独立实现（与桌面端同名 store 无关），纯 localStorage 持久化。
 * 覆盖：默认配置、位置/频次/设置/快捷键配置的持久化与恢复、
 * 快捷键增删改与 builtin 保护、可见性切换、高频排序
 * （topShortcuts / getQuickBarItems 频次排序、数量钳制 3-10、无统计默认列表、颜色分类映射）。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useInputAssistantStore, type ShortcutItem, type QuickBarItem } from '@/stores/inputAssistant'

const KEYS = {
  stats: 'terminal_shortcut_stats',
  position: 'input_assistant_position',
  settings: 'input_assistant_settings',
  cmdStats: 'terminal_custom_cmd_stats',
  shortcutConfig: 'terminal_shortcut_config',
} as const

describe('inputAssistant store', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  function newStore() {
    return useInputAssistantStore()
  }

  it('defaults: 12 builtin shortcuts, size 48, quickBarCount 6, floating ball off', () => {
    const store = newStore()
    expect(store.position).toEqual({ x: -1, y: -1 })
    expect(store.isExpanded).toBe(false)
    expect(store.settings.size).toBe(48)
    expect(store.settings.quickBarCount).toBe(6)
    expect(store.settings.floatingBall).toBe(false)
    expect(store.settings.headerToolbarItems).toEqual(['folder'])
    expect(store.settings.terminalFontSize).toBe(12)
    expect(store.settings.terminalTheme).toBeNull()
    expect(store.settings.isTerminalThemeUserSet).toBe(false)
    expect(store.settings.gestures).toEqual({
      doubleTap: true,
      swipeDown: true,
      swipeUp: true,
      swipeLeft: true,
      swipeRight: true,
    })
    expect(store.shortcutConfig).toHaveLength(12)
    expect(store.shortcutConfig.every((s) => s.builtin && s.visible)).toBe(true)
  })

  it('savePosition updates state and persists; new store instance restores it', () => {
    const store = newStore()
    store.savePosition(120, 340)
    expect(store.position).toEqual({ x: 120, y: 340 })
    expect(JSON.parse(localStorage.getItem(KEYS.position)!)).toEqual({ x: 120, y: 340 })

    // 重新创建 store 模拟应用重启：从 localStorage 恢复
    setActivePinia(createPinia())
    const restored = newStore()
    expect(restored.position).toEqual({ x: 120, y: 340 })
  })

  it('recordShortcut increments count from zero and accumulates', () => {
    const store = newStore()
    store.recordShortcut('ctrl+c')
    store.recordShortcut('ctrl+c')
    store.recordShortcut('ctrl+v')
    expect(store.shortcutStats['ctrl+c']).toBe(2)
    expect(store.shortcutStats['ctrl+v']).toBe(1)
    expect(JSON.parse(localStorage.getItem(KEYS.stats)!)).toEqual({ 'ctrl+c': 2, 'ctrl+v': 1 })
  })

  it('recordCustomCommand increments per command id', () => {
    const store = newStore()
    store.recordCustomCommand('cmd-1')
    store.recordCustomCommand('cmd-1')
    expect(store.customCommandStats['cmd-1']).toBe(2)
    expect(JSON.parse(localStorage.getItem(KEYS.cmdStats)!)).toEqual({ 'cmd-1': 2 })
  })

  it('saveSettings merges partial settings and persists', () => {
    const store = newStore()
    store.saveSettings({ size: 56, floatingBall: true })
    expect(store.settings.size).toBe(56)
    expect(store.settings.floatingBall).toBe(true)
    // 未改动的字段保留
    expect(store.settings.quickBarCount).toBe(6)
    const saved = JSON.parse(localStorage.getItem(KEYS.settings)!)
    expect(saved.size).toBe(56)
    expect(saved.quickBarCount).toBe(6)
  })

  it('resetSettings restores defaults and persists', () => {
    const store = newStore()
    store.saveSettings({ size: 64, quickBarCount: 8, floatingBall: true })
    store.resetSettings()
    expect(store.settings.size).toBe(48)
    expect(store.settings.quickBarCount).toBe(6)
    expect(store.settings.floatingBall).toBe(false)
    const saved = JSON.parse(localStorage.getItem(KEYS.settings)!)
    expect(saved.quickBarCount).toBe(6)
  })

  it('addShortcut appends a non-builtin shortcut and persists; duplicate code is ignored', () => {
    const store = newStore()
    store.addShortcut('ctrl+shift+p', 'Ctrl+Shift+P')
    expect(store.shortcutConfig).toHaveLength(13)
    expect(store.shortcutConfig[12]).toEqual({
      code: 'ctrl+shift+p',
      label: 'Ctrl+Shift+P',
      visible: true,
      builtin: false,
    })
    // 重复 code 与内置 code 冲突均忽略
    store.addShortcut('ctrl+shift+p', 'Dup')
    store.addShortcut('tab', 'Tab')
    expect(store.shortcutConfig).toHaveLength(13)
    const persisted = JSON.parse(localStorage.getItem(KEYS.shortcutConfig)!) as ShortcutItem[]
    expect(persisted).toHaveLength(13)
  })

  it('removeShortcut deletes custom shortcuts but protects builtin ones', () => {
    const store = newStore()
    store.addShortcut('ctrl+shift+p', 'Ctrl+Shift+P')
    store.removeShortcut('ctrl+shift+p')
    expect(store.shortcutConfig).toHaveLength(12)
    // builtin 不可删除
    store.removeShortcut('tab')
    expect(store.shortcutConfig.some((s) => s.code === 'tab')).toBe(true)
    // 未知 code 无操作
    store.removeShortcut('nope')
    expect(store.shortcutConfig).toHaveLength(12)
  })

  it('toggleShortcutVisibility flips visible flag and persists; unknown code no-op', () => {
    const store = newStore()
    store.toggleShortcutVisibility('tab')
    expect(store.shortcutConfig.find((s) => s.code === 'tab')!.visible).toBe(false)
    store.toggleShortcutVisibility('tab')
    expect(store.shortcutConfig.find((s) => s.code === 'tab')!.visible).toBe(true)
    const persisted = JSON.parse(localStorage.getItem(KEYS.shortcutConfig)!) as ShortcutItem[]
    expect(persisted.find((s) => s.code === 'tab')!.visible).toBe(true)
    // 未知 code 无操作
    store.toggleShortcutVisibility('nope')
    expect(store.shortcutConfig).toHaveLength(12)
  })

  it('visiblePanelShortcuts excludes enter/backspace and hidden entries', () => {
    const store = newStore()
    store.toggleShortcutVisibility('tab') // hide tab
    const visible = store.visiblePanelShortcuts.map((s) => s.code)
    expect(visible).not.toContain('tab')
    expect(visible).not.toContain('enter')
    expect(visible).not.toContain('backspace')
    expect(visible).toHaveLength(9)
    expect(visible).toContain('escape')
  })

  it('topShortcuts returns top 3 keys by usage desc', () => {
    const store = newStore()
    store.recordShortcut('ctrl+a')
    store.recordShortcut('ctrl+a')
    store.recordShortcut('ctrl+b')
    store.recordShortcut('ctrl+c')
    store.recordShortcut('ctrl+c')
    store.recordShortcut('ctrl+c')
    expect(store.topShortcuts).toEqual(['ctrl+c', 'ctrl+a', 'ctrl+b'])
  })

  it('getQuickBarItems: no stats → default quick keys, top N with count 0', () => {
    const store = newStore()
    const items = store.getQuickBarItems([])
    expect(items.map((i) => i.key)).toEqual(['tab', 'enter', 'escape', 'ctrl_c', 'ctrl_z', 'arrow_up'])
    expect(items.every((i) => i.count === 0)).toBe(true)
  })

  it('getQuickBarItems: merges shortcuts and custom commands sorted by usage desc', () => {
    const store = newStore()
    store.recordShortcut('ctrl_c')
    store.recordShortcut('ctrl_c')
    store.recordShortcut('enter')
    store.recordCustomCommand('cmd-1')
    store.recordCustomCommand('cmd-1')
    store.recordCustomCommand('cmd-1')
    const items = store.getQuickBarItems([{ id: 'cmd-1', command: 'git status' }])
    expect(items.map((i) => i.key)).toEqual(['cmd-1', 'ctrl_c', 'enter'])
    expect(items[0].type).toBe('custom')
    expect(items[0].label).toBe('git status')
    expect(items[1].type).toBe('shortcut')
    expect(items[1].label).toBe('Ctrl+C')
  })

  it('getQuickBarItems: quickBarCount clamped to [3, 10]', () => {
    const store = newStore()
    store.saveSettings({ quickBarCount: 2 })
    expect(store.getQuickBarItems([])).toHaveLength(3)
    store.saveSettings({ quickBarCount: 20 })
    expect(store.getQuickBarItems([])).toHaveLength(10)
  })

  it('getQuickBarItems: category mapping (enter/del/arrow/shortcut/custom)', () => {
    const store = newStore()
    store.recordShortcut('enter')
    store.recordShortcut('backspace')
    store.recordShortcut('arrow_up')
    store.recordShortcut('ctrl_a')
    store.recordCustomCommand('c1')
    const items = store.getQuickBarItems([{ id: 'c1', command: 'echo hi' }])
    const byKey = Object.fromEntries(items.map((i: QuickBarItem) => [i.key, i.category]))
    expect(byKey['enter']).toBe('enter')
    expect(byKey['backspace']).toBe('del')
    expect(byKey['arrow_up']).toBe('arrow')
    expect(byKey['ctrl_a']).toBe('shortcut')
    expect(byKey['c1']).toBe('custom')
  })

  it('loadFromStorage: corrupt settings JSON keeps previous state and logs error', () => {
    const store = newStore()
    store.recordShortcut('ctrl+k')
    store.saveSettings({ size: 60 })
    localStorage.setItem(KEYS.settings, '{corrupt')
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    store.loadFromStorage()
    // 解析抛错发生在赋值表达式内 → settings 保持当前内存值不变
    expect(store.settings.size).toBe(60)
    expect(errorSpy).toHaveBeenCalled()
    errorSpy.mockRestore()
  })

  it('loadShortcutConfig merges missing default shortcuts into saved config without duplicates', () => {
    const store = newStore()
    // 模拟旧版本保存的配置：缺少后来新增的 ctrl+e
    const oldConfig = store.shortcutConfig.filter((s) => s.code !== 'ctrl+e')
    localStorage.setItem(KEYS.shortcutConfig, JSON.stringify(oldConfig))
    store.loadFromStorage()
    const codes = store.shortcutConfig.map((s) => s.code)
    expect(codes).toContain('ctrl+e')
    expect(new Set(codes).size).toBe(codes.length)
    expect(store.shortcutConfig).toHaveLength(12)
  })

  it('resetShortcutConfig restores 12 builtin defaults and persists', () => {
    const store = newStore()
    store.addShortcut('ctrl+shift+p', 'Ctrl+Shift+P')
    store.resetShortcutConfig()
    expect(store.shortcutConfig).toHaveLength(12)
    expect(store.shortcutConfig.every((s) => s.builtin)).toBe(true)
    const persisted = JSON.parse(localStorage.getItem(KEYS.shortcutConfig)!) as ShortcutItem[]
    expect(persisted).toHaveLength(12)
  })

  it('toggleExpanded / collapse manage the expanded flag', () => {
    const store = newStore()
    expect(store.isExpanded).toBe(false)
    store.toggleExpanded()
    expect(store.isExpanded).toBe(true)
    store.toggleExpanded()
    expect(store.isExpanded).toBe(false)
    store.toggleExpanded()
    store.collapse()
    expect(store.isExpanded).toBe(false)
  })
})
