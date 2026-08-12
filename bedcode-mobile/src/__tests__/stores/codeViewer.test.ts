/**
 * codeViewer store 单元测试
 *
 * 移动端 codeViewer 是纯前端设置存储（localStorage 持久化，不经过 Rust 后端），
 * 不含文件内容加载/打开关闭状态（该能力在桌面端实现）。
 * 覆盖：默认值、saveSettings 持久化与入参拷贝、resetSettings、
 * 损坏存储回退默认、resolveCodeTheme / mapThemeForMode 主题解析与明暗映射、CODE_THEMES 元数据。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import {
  useCodeViewerStore,
  resolveCodeTheme,
  mapThemeForMode,
  CODE_THEMES,
  type CodeViewerSettings,
} from '@/stores/codeViewer'

const STORAGE_KEY = 'bedcode-code-viewer-settings'

function readStorage(): CodeViewerSettings | null {
  const raw = localStorage.getItem(STORAGE_KEY)
  return raw ? JSON.parse(raw) : null
}

describe('codeViewer store', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('fresh storage: defaults (fontSize 10, lineHeight 1.0, theme system, tabSize 4, line numbers on)', () => {
    const store = useCodeViewerStore()
    expect(store.settings).toEqual({
      fontSize: 10,
      lineHeight: 1.0,
      theme: 'system',
      tabSize: 4,
      showLineNumbers: true,
    })
  })

  it('store creation merges saved settings over defaults (partial storage)', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ fontSize: 16, theme: 'nord' }))
    const store = useCodeViewerStore()
    expect(store.settings.fontSize).toBe(16)
    expect(store.settings.theme).toBe('nord')
    // 未保存的字段回退默认
    expect(store.settings.tabSize).toBe(4)
    expect(store.settings.lineHeight).toBe(1.0)
    expect(store.settings.showLineNumbers).toBe(true)
  })

  it('corrupt storage falls back to defaults with a console warning', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    localStorage.setItem(STORAGE_KEY, '{not json')
    const store = useCodeViewerStore()
    expect(store.settings).toEqual({
      fontSize: 10,
      lineHeight: 1.0,
      theme: 'system',
      tabSize: 4,
      showLineNumbers: true,
    })
    expect(warnSpy).toHaveBeenCalled()
    warnSpy.mockRestore()
  })

  it('saveSettings replaces settings wholesale (type contract is a full object) and persists', () => {
    const store = useCodeViewerStore()
    store.saveSettings({
      fontSize: 14,
      lineHeight: 1.2,
      theme: 'monokai',
      tabSize: 8,
      showLineNumbers: false,
    })
    expect(store.settings).toEqual({
      fontSize: 14,
      lineHeight: 1.2,
      theme: 'monokai',
      tabSize: 8,
      showLineNumbers: false,
    })
    expect(readStorage()).toEqual(store.settings)
  })

  it('saveSettings copies the input object instead of aliasing the caller', () => {
    const store = useCodeViewerStore()
    const input: CodeViewerSettings = {
      fontSize: 12,
      lineHeight: 1.2,
      theme: 'nord',
      tabSize: 2,
      showLineNumbers: false,
    }
    store.saveSettings(input)
    input.fontSize = 99
    // 后续修改调用方对象不影响 store 内状态
    expect(store.settings.fontSize).toBe(12)
  })

  it('saveSettings with a partial object drops unspecified fields (replacement, not merge)', () => {
    const store = useCodeViewerStore()
    store.saveSettings({ fontSize: 20 } as CodeViewerSettings)
    expect(store.settings).toEqual({ fontSize: 20 })
    expect(readStorage()).toEqual({ fontSize: 20 })
  })

  it('resetSettings restores defaults and persists them', () => {
    const store = useCodeViewerStore()
    store.saveSettings({ fontSize: 20, theme: 'github-dark' })
    store.resetSettings()
    expect(store.settings).toEqual({
      fontSize: 10,
      lineHeight: 1.0,
      theme: 'system',
      tabSize: 4,
      showLineNumbers: true,
    })
    expect(readStorage()).toEqual(store.settings)
  })
})

describe('codeViewer theme resolution', () => {
  it('resolveCodeTheme: explicit theme passes through untouched', () => {
    expect(resolveCodeTheme('nord', true)).toBe('nord')
    expect(resolveCodeTheme('github-light', false)).toBe('github-light')
  })

  it('resolveCodeTheme: system mode resolves to vitesse-dark/light by app theme', () => {
    expect(resolveCodeTheme('system', true)).toBe('vitesse-dark')
    expect(resolveCodeTheme('system', false)).toBe('vitesse-light')
  })

  it('mapThemeForMode: system stays system in both modes', () => {
    expect(mapThemeForMode('system', true)).toBe('system')
    expect(mapThemeForMode('system', false)).toBe('system')
  })

  it('mapThemeForMode: dark mode maps light theme to its dark counterpart', () => {
    expect(mapThemeForMode('one-light', true)).toBe('one-dark-pro')
    expect(mapThemeForMode('github-light', true)).toBe('github-dark')
    expect(mapThemeForMode('vitesse-light', true)).toBe('vitesse-dark')
  })

  it('mapThemeForMode: light mode maps dark theme to its light counterpart', () => {
    expect(mapThemeForMode('one-dark-pro', false)).toBe('one-light')
    expect(mapThemeForMode('github-dark', false)).toBe('github-light')
    expect(mapThemeForMode('vitesse-dark', false)).toBe('vitesse-light')
  })

  it('mapThemeForMode: theme without a counterpart (nord/monokai) stays unchanged', () => {
    expect(mapThemeForMode('nord', true)).toBe('nord')
    expect(mapThemeForMode('nord', false)).toBe('nord')
    expect(mapThemeForMode('monokai', true)).toBe('monokai')
  })

  it('CODE_THEMES metadata: every theme carries label/background/foreground', () => {
    const keys = Object.keys(CODE_THEMES)
    expect(keys).toContain('system')
    expect(keys).toContain('nord')
    expect(keys).toContain('github-light')
    for (const meta of Object.values(CODE_THEMES)) {
      expect(meta.label).toBeTruthy()
      expect(typeof meta.background).toBe('string')
      expect(typeof meta.foreground).toBe('string')
    }
    // system 主题跟随 CSS 变量，其余为硬编码色值
    expect(CODE_THEMES['system'].background).toBe('var(--mobile-bg-secondary)')
    expect(CODE_THEMES['system'].foreground).toBe('var(--mobile-text-primary)')
    expect(CODE_THEMES['nord'].background).toBe('#2e3440')
  })
})
