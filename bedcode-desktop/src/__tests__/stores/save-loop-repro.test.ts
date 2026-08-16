import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'

// Mock Tauri invoke：模拟有状态后端（get 返回最近一次 save 的内容），带 IPC 延迟
const mockInvoke = vi.fn()
let backend: any = {}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

/** 与 SettingsView.vue 相同的 watch + 防抖逻辑 */
function setupDebouncedWatch(store: ReturnType<typeof useSettingsStore>) {
  let saveTimeout: ReturnType<typeof setTimeout> | null = null
  watch(
    () => store.settings,
    () => {
      if (store.isPersisted(store.settings)) return
      if (saveTimeout) clearTimeout(saveTimeout)
      saveTimeout = setTimeout(() => {
        void store.saveSettings(store.settings)
      }, 500)
    },
    { deep: true },
  )
  /** 组件卸载时的立即 flush（与 onBeforeUnmount 一致） */
  const flush = () => {
    if (saveTimeout) {
      clearTimeout(saveTimeout)
      saveTimeout = null
      if (!store.isPersisted(store.settings)) void store.saveSettings(store.settings)
    }
  }
  return { flush }
}

const countSaves = () => mockInvoke.mock.calls.filter((c) => c[0] === 'save_app_settings').length

describe('settings save loop regression', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    backend = {
      network: { port: 8765, prevent_sleep: true },
      session: { default_environment: 'windows', session_timeout: 3600 },
      ui: {
        theme: 'dark', font_size: 12, terminal_font_size: 12,
        terminal_font_family: 'Consolas', terminal_theme: 'oneDark',
        show_preview: true, language: 'zh-CN', terminal_bg_opacity: 30,
      },
    }
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      await sleep(15)
      if (cmd === 'get_app_settings') return JSON.parse(JSON.stringify(backend))
      if (cmd === 'save_app_settings') {
        backend = JSON.parse(JSON.stringify(args.settings))
        return undefined
      }
    })
  })

  it('进入设置页只保存一次，不产生保存循环（回归：引用比对失效导致 500ms 循环刷写）', async () => {
    const store = useSettingsStore()
    setupDebouncedWatch(store)

    await store.loadSettings()
    await sleep(2500)

    expect(countSaves()).toBeLessThanOrEqual(1)
  })

  it('修改字体大小后切走再回来，值应持久化不回到默认（回归：防抖/回写竞态丢失）', async () => {
    const store = useSettingsStore()
    const { flush } = setupDebouncedWatch(store)

    // 第一次进入设置页
    await store.loadSettings()
    await sleep(700) // 等首次同步保存完成

    // 用户拖动滑杆到 14 并立即切走（<500ms 防抖窗口内触发 flush）
    store.settings.ui.font_size = 14
    flush()
    await sleep(300)

    // 再次进入设置页：应从后端读到 14 而非默认 12
    await store.loadSettings()
    expect(store.settings.ui.font_size).toBe(14)
  })

  it('防抖窗口内连续修改后保存的是最新值', async () => {
    const store = useSettingsStore()
    setupDebouncedWatch(store)

    await store.loadSettings()
    // 连续拖滑杆
    store.settings.ui.font_size = 13
    store.settings.ui.font_size = 15
    await sleep(800)

    expect(backend.ui.font_size).toBe(15)
  })
})
