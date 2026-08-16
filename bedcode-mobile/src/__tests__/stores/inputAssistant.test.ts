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

  it('defaults: 18 builtin shortcuts (16 grid keys + enter/backspace), size 48, quickBarCount 6, floating ball off', () => {
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
    expect(store.shortcutConfig).toHaveLength(18)
    expect(store.shortcutConfig.every((s) => s.builtin && s.visible)).toBe(true)
    // 16 个网格键（agent CLI 场景精选）：无 ctrl+z（挂起语义对移动端无意义）
    const codes = store.shortcutConfig.map((s) => s.code)
    expect(codes).toContain('shift+tab')
    expect(codes).toContain('ctrl+o')
    expect(codes).toContain('ctrl+t')
    expect(codes).toContain('ctrl+r')
    expect(codes).toContain('ctrl+w')
    expect(codes).toContain('alt+p')
    expect(codes).toContain('ctrl+g')
    expect(codes).not.toContain('ctrl+z')
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
    expect(store.shortcutConfig).toHaveLength(19)
    expect(store.shortcutConfig[18]).toEqual({
      code: 'ctrl+shift+p',
      label: 'Ctrl+Shift+P',
      visible: true,
      builtin: false,
    })
    // 重复 code 与内置 code 冲突均忽略
    store.addShortcut('ctrl+shift+p', 'Dup')
    store.addShortcut('tab', 'Tab')
    expect(store.shortcutConfig).toHaveLength(19)
    const persisted = JSON.parse(localStorage.getItem(KEYS.shortcutConfig)!) as ShortcutItem[]
    expect(persisted).toHaveLength(19)
  })

  it('removeShortcut deletes custom shortcuts but protects builtin ones', () => {
    const store = newStore()
    store.addShortcut('ctrl+shift+p', 'Ctrl+Shift+P')
    store.removeShortcut('ctrl+shift+p')
    expect(store.shortcutConfig).toHaveLength(18)
    // builtin 不可删除
    store.removeShortcut('tab')
    expect(store.shortcutConfig.some((s) => s.code === 'tab')).toBe(true)
    // 未知 code 无操作
    store.removeShortcut('nope')
    expect(store.shortcutConfig).toHaveLength(18)
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
    expect(store.shortcutConfig).toHaveLength(18)
  })

  it('visiblePanelShortcuts excludes enter/backspace and hidden entries', () => {
    const store = newStore()
    store.toggleShortcutVisibility('tab') // hide tab
    const visible = store.visiblePanelShortcuts.map((s) => s.code)
    expect(visible).not.toContain('tab')
    expect(visible).not.toContain('enter')
    expect(visible).not.toContain('backspace')
    // 16 网格键 - 1 隐藏 = 15
    expect(visible).toHaveLength(15)
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

  it('getQuickBarItems: no stats → fixed enter/del at front + default quick keys, top N with count 0', () => {
    const store = newStore()
    const items = store.getQuickBarItems([])
    expect(items.map((i) => i.key)).toEqual(['enter', 'backspace', 'escape', 'tab', 'shift+tab', 'ctrl+c', 'ctrl+o', 'ctrl+t'])
    expect(items.every((i) => i.count === 0)).toBe(true)
  })

  it('getQuickBarItems: merges shortcuts and custom commands sorted by usage desc, enter/del fixed at front', () => {
    const store = newStore()
    store.recordShortcut('ctrl_c')
    store.recordShortcut('ctrl_c')
    store.recordShortcut('enter')
    store.recordShortcut('enter')
    store.recordShortcut('enter')
    store.recordCustomCommand('cmd-1')
    store.recordCustomCommand('cmd-1')
    store.recordCustomCommand('cmd-1')
    const items = store.getQuickBarItems([{ id: 'cmd-1', command: 'git status' }])
    expect(items.map((i) => i.key)).toEqual(['enter', 'backspace', 'cmd-1', 'ctrl_c'])
    expect(items[0].type).toBe('shortcut')
    expect(items[0].label).toBe('Enter')
    expect(items[1].type).toBe('shortcut')
    expect(items[1].label).toBe('Del')
    expect(items[2].type).toBe('custom')
    expect(items[2].label).toBe('git status')
    expect(items[3].type).toBe('shortcut')
    expect(items[3].label).toBe('Ctrl+C')
  })

  it('getQuickBarItems: quickBarCount clamped to [3, 10] plus 2 fixed (enter/del)', () => {
    const store = newStore()
    store.saveSettings({ quickBarCount: 2 })
    expect(store.getQuickBarItems([])).toHaveLength(5)
    store.saveSettings({ quickBarCount: 20 })
    expect(store.getQuickBarItems([])).toHaveLength(11) // 默认池 9 项 + 2 固定
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
    // 模拟旧版本保存的配置：缺少后来新增的 ctrl+w
    const oldConfig = store.shortcutConfig.filter((s) => s.code !== 'ctrl+w')
    localStorage.setItem(KEYS.shortcutConfig, JSON.stringify(oldConfig))
    store.loadFromStorage()
    const codes = store.shortcutConfig.map((s) => s.code)
    expect(codes).toContain('ctrl+w')
    expect(new Set(codes).size).toBe(codes.length)
    expect(store.shortcutConfig).toHaveLength(18)
  })

  it('resetShortcutConfig restores 18 builtin defaults and persists', () => {
    const store = newStore()
    store.addShortcut('ctrl+shift+p', 'Ctrl+Shift+P')
    store.resetShortcutConfig()
    expect(store.shortcutConfig).toHaveLength(18)
    expect(store.shortcutConfig.every((s) => s.builtin)).toBe(true)
    const persisted = JSON.parse(localStorage.getItem(KEYS.shortcutConfig)!) as ShortcutItem[]
    expect(persisted).toHaveLength(18)
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

  // ==================== Agent CLI 预设 ====================

  it('setAgentPreset: claude_code loads 12 builtin preset commands, generic clears', () => {
    const store = newStore()
    store.setAgentPreset('claude_code')
    expect(store.activeAgentType).toBe('claude_code')
    expect(store.presetCommands).toHaveLength(12)
    expect(store.presetCommands.every((c) => c.builtin)).toBe(true)
    // skills 位为发送模式
    const skills = store.presetCommands.find((c) => c.command === '/')
    expect(skills?.mode).toBe('send')
    // 其余执行
    expect(store.presetCommands.filter((c) => c.mode === 'execute')).toHaveLength(11)
    // generic 清空预设
    store.setAgentPreset('generic')
    expect(store.presetCommands).toHaveLength(0)
  })

  it('setAgentPreset: pi preset has /skill: in send mode; codex/opencode have execute substitutes', () => {
    const store = newStore()
    store.setAgentPreset('pi')
    expect(store.presetCommands.find((c) => c.command === '/skill:')?.mode).toBe('send')
    store.setAgentPreset('codex')
    const codex = store.presetCommands.map((c) => c.command)
    expect(codex).toContain('/init') // skills 替代
    expect(store.presetCommands.every((c) => c.mode === 'execute')).toBe(true)
    store.setAgentPreset('opencode')
    const opencode = store.presetCommands.map((c) => c.command)
    expect(opencode).toContain('/templates')
    expect(store.presetCommands.every((c) => c.mode === 'execute')).toBe(true)
  })

  it('getEffectiveAgentType: override wins, otherwise keyword detection from command', async () => {
    // 测试环境无 Tauri invoke：覆盖表为空，保存失败仅打日志
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const store = newStore()
    // 无覆盖：按启动命令关键词识别
    expect(store.getEffectiveAgentType('cfg-1', 'claude --dangerously-skip-permissions')).toBe('claude_code')
    expect(store.getEffectiveAgentType('cfg-1', 'npx opencode')).toBe('opencode')
    expect(store.getEffectiveAgentType('cfg-1', 'pi')).toBe('pi')
    expect(store.getEffectiveAgentType('cfg-1', 'npm run dev')).toBe('generic')
    // 手动覆盖优先：包装脚本启动 codex 但配置里声明 claude_code
    await store.setAgentTypeOverride('cfg-1', 'codex')
    expect(store.getEffectiveAgentType('cfg-1', 'claude')).toBe('codex')
    errorSpy.mockRestore()
  })
})
