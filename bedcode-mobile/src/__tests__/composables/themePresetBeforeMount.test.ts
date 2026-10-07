/**
 * presetThemeBeforeMount 启动主题预应用契约测试
 *
 * 需求（用户报告 bug：移动端启动闪烁两次）：App.vue 的 setupTheme 在 onMounted
 * 才执行，首帧渲染时 .dark 类未上树——mobile.css 的 --mobile-* token 以
 * html:not(.dark) 为浅色、:root 为深色，深色用户首帧会以浅色 token 渲染一帧
 * 再翻转（闪变）。main.ts 在 app.mount 前调用 presetThemeBeforeMount()，把
 * .dark 类与 data-palette 提前挂到 <html>。
 *
 * 本测试验证该函数的行为契约：theme 取值（dark/light/system×系统深浅）与
 * palette 设置到 documentElement 的结果一致、幂等、无副作用残留。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore } from '@/stores/settings'
import { presetThemeBeforeMount } from '@/composables/useTheme'

// invoke 由全局 setup.ts 替身（@tauri-apps/api/core）兜底，set_status_bar_style
// 静默成功，不阻塞 applyTheme

// ==================== 夹具 ====================

/** 覆写 window.matchMedia，控制系统深浅色；返回恢复函数 */
function stubSystemDark(matches: boolean) {
  const stub = vi.fn((query: string) => ({
    matches,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }))
  vi.stubGlobal('matchMedia', stub)
  return stub
}

function setTheme(theme: string) {
  const store = useSettingsStore()
  store.settings.ui.theme = theme
  store.settings.ui.palette = 'ocean'
}

function cleanupDom() {
  document.documentElement.classList.remove('dark')
  document.documentElement.removeAttribute('data-palette')
}

// ==================== 行为契约 ====================

describe('presetThemeBeforeMount 首帧主题预应用', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    cleanupDom()
  })

  afterEach(() => {
    cleanupDom()
    vi.unstubAllGlobals()
  })

  it('theme=dark → html 挂上 .dark 类 + data-palette', () => {
    setTheme('dark')
    presetThemeBeforeMount()
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(document.documentElement.getAttribute('data-palette')).toBe('ocean')
  })

  it('theme=light → html 不带 .dark 类（浅色 token 生效），palette 仍写入', () => {
    setTheme('light')
    presetThemeBeforeMount()
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(document.documentElement.getAttribute('data-palette')).toBe('ocean')
  })

  it('theme=system + 系统深色 → .dark 类挂上（首帧即深色，消除浅色闪帧）', () => {
    stubSystemDark(true)
    setTheme('system')
    presetThemeBeforeMount()
    expect(document.documentElement.classList.contains('dark')).toBe(true)
  })

  it('theme=system + 系统浅色 → 不带 .dark 类', () => {
    stubSystemDark(false)
    setTheme('system')
    presetThemeBeforeMount()
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })

  it('重复调用幂等：类/属性结果不变（App.vue setupTheme 二次调用不翻转）', () => {
    stubSystemDark(true)
    setTheme('system')
    presetThemeBeforeMount()
    presetThemeBeforeMount()
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(document.documentElement.getAttribute('data-palette')).toBe('ocean')
  })
})
