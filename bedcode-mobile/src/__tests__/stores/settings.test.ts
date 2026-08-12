/**
 * settings store 单元测试
 *
 * mock @tauri-apps/api/core invoke，覆盖：
 * 默认值、loadSettings 与 Rust 后端返回值按节深合并（get_app_settings）、
 * 加载失败回退默认、saveSettings 合并与持久化（save_app_settings，浅合并语义）、
 * 保存失败不改本地状态、getMaxCachedTerminals 兜底逻辑。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

import { useSettingsStore } from '@/stores/settings'

/** 构造一个完整的 ui 节（saveSettings 浅合并按节整体替换，调用方需传完整节） */
function fullUi(overrides: Partial<{ theme: string; terminal_font_size: number; terminal_font_family: string; show_preview: boolean; language: string; max_cached_terminals: number; notify_in_background: boolean; palette: string }> = {}) {
  return {
    theme: 'system',
    terminal_font_size: 12,
    terminal_font_family: 'Consolas',
    show_preview: true,
    language: 'zh-CN',
    max_cached_terminals: 10,
    notify_in_background: true,
    palette: 'default',
    ...overrides,
  }
}

describe('settings store', () => {
  let store: ReturnType<typeof useSettingsStore>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useSettingsStore()
    vi.clearAllMocks()
  })

  it('defaults: port 8765, claude default command, zh-CN, 10 cached terminals', () => {
    expect(store.settings.network.port).toBe(8765)
    expect(store.settings.session.default_environment).toBe('windows')
    expect(store.settings.session.default_command).toBe('claude')
    expect(store.settings.session.session_timeout).toBe(3600)
    expect(store.settings.ui.theme).toBe('system')
    expect(store.settings.ui.terminal_font_size).toBe(12)
    expect(store.settings.ui.terminal_font_family).toBe('Consolas')
    expect(store.settings.ui.show_preview).toBe(true)
    expect(store.settings.ui.language).toBe('zh-CN')
    expect(store.settings.ui.max_cached_terminals).toBe(10)
    expect(store.settings.ui.notify_in_background).toBe(true)
    expect(store.settings.ui.palette).toBe('default')
  })

  it('loadSettings merges partial backend response over defaults per section', async () => {
    invokeMock.mockResolvedValue({
      network: { port: 9000 },
      ui: { theme: 'dark' },
    })
    await store.loadSettings()
    expect(invokeMock).toHaveBeenCalledWith('get_app_settings')
    expect(store.settings.network.port).toBe(9000)
    expect(store.settings.ui.theme).toBe('dark')
    // 后端未返回的字段保持默认
    expect(store.settings.session.default_command).toBe('claude')
    expect(store.settings.ui.language).toBe('zh-CN')
    expect(store.settings.ui.max_cached_terminals).toBe(10)
  })

  it('loadSettings failure keeps defaults and logs error', async () => {
    invokeMock.mockRejectedValue(new Error('backend down'))
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    await store.loadSettings()
    expect(store.settings.network.port).toBe(8765)
    expect(store.settings.ui.theme).toBe('system')
    expect(errorSpy).toHaveBeenCalled()
    errorSpy.mockRestore()
  })

  it('saveSettings persists merged settings via save_app_settings and updates state', async () => {
    invokeMock.mockResolvedValue(null)
    await store.saveSettings({ ui: fullUi({ theme: 'dark', terminal_font_size: 14 }) })
    expect(invokeMock).toHaveBeenCalledWith('save_app_settings', {
      settings: expect.objectContaining({
        network: expect.objectContaining({ port: 8765 }),
        session: expect.objectContaining({ default_command: 'claude' }),
        ui: expect.objectContaining({ theme: 'dark', terminal_font_size: 14 }),
      }),
    })
    expect(store.settings.ui.theme).toBe('dark')
    expect(store.settings.ui.terminal_font_size).toBe(14)
  })

  it('saveSettings shallow-merges top-level sections (whole section replaced)', async () => {
    invokeMock.mockResolvedValue(null)
    await store.saveSettings({ network: { port: 9123 } })
    expect(store.settings.network).toEqual({ port: 9123 })
    expect(invokeMock).toHaveBeenCalledWith('save_app_settings', {
      settings: expect.objectContaining({ network: { port: 9123 } }),
    })
  })

  it('saveSettings failure keeps previous state and logs error', async () => {
    invokeMock.mockRejectedValue(new Error('persist failed'))
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    await store.saveSettings({ ui: fullUi({ theme: 'dark' }) })
    expect(store.settings.ui.theme).toBe('system')
    expect(errorSpy).toHaveBeenCalled()
    errorSpy.mockRestore()
  })

  it('getMaxCachedTerminals falls back to 10 for unset/zero values', () => {
    expect(store.getMaxCachedTerminals()).toBe(10)
    store.settings.ui.max_cached_terminals = 5
    expect(store.getMaxCachedTerminals()).toBe(5)
    store.settings.ui.max_cached_terminals = 0
    expect(store.getMaxCachedTerminals()).toBe(10)
    store.settings.ui.max_cached_terminals = undefined
    expect(store.getMaxCachedTerminals()).toBe(10)
  })
})
