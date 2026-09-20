import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import i18n from '@/locales'
import SettingsView from '@/views/SettingsView.vue'
import { getPluginRegistry } from '@/plugin/registry'

// Mock Tauri APIs（设置 store / 更新检查均经 invoke）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'get_app_settings') {
      return {
        network: { port: 8765 },
        session: { default_environment: 'windows', default_command: 'claude' },
        ui: { theme: 'system', language: 'zh-CN', animations_enabled: true },
        log: { level: 'info' },
      }
    }
    return undefined
  }),
  convertFileSrc: vi.fn((p: string) => p),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

vi.mock('@tauri-apps/plugin-os', () => ({
  platform: vi.fn(async () => 'windows'),
}))

/**
 * 设置页渲染测试 — 分组合并渲染的外部可见行为
 *
 * 断言渲染出的分组标题序列（外部可见输出），而非内部函数：
 * 未注册贡献时与改造前逐分组等价；贡献分组按 order 落到正确位置；
 * 插件 error 态后设置页回落到纯内置形态（兜底壳，不残留空分组）。
 */
describe('SettingsView 分组渲染', () => {
  const registry = getPluginRegistry()

  /** 内置分组标题 key（顺序与写死的分组组件一一对应；
   * 票 14：`settings.pairing.title` 已退役——配对分组改由 com.bedcode.session 贡献） */
  const BUILTIN_TITLE_KEYS = [
    'settings.ui.title',
    'settings.linkCrypto.title',
    'settings.session.title',
    'settings.system.title',
    'settings.log.title',
    'settings.about.title',
  ]
  const BUILTIN_TITLES = BUILTIN_TITLE_KEYS.map((k) => i18n.global.t(k))

  let router: ReturnType<typeof createRouter>

  async function mountView(): Promise<VueWrapper> {
    const wrapper = mount(SettingsView, {
      global: {
        plugins: [createPinia(), i18n, router],
      },
    })
    await flushPromises()
    return wrapper
  }

  /** 渲染出的分组标题文本（按 DOM 顺序） */
  function titles(wrapper: VueWrapper): string[] {
    return wrapper.findAll('h3.wb-section-title').map((h) => h.text().trim())
  }

  beforeEach(() => {
    setActivePinia(createPinia())
    router = createRouter({
      history: createWebHistory(),
      routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
    })
  })

  afterEach(() => {
    registry.clearPlugin('com.bedcode.session')
  })

  it('未注册贡献时渲染 6 个内置分组，标题序列与内置分组定义一致', async () => {
    const wrapper = await mountView()
    expect(titles(wrapper)).toEqual(BUILTIN_TITLES)
  })

  it('贡献分组按 order 落到内置分组之间，标题取插件命名空间文案', async () => {
    registry.setPluginState('com.bedcode.session', { state: 'Activated' })
    // 插件文案按「插件 id + 扁平点号 key」合并（与 context.i18n.registerMessages 同一形态）；
    // 覆盖 zh-CN / zh / en 三种 locale 取值，避免受设置 store 的语言默认值影响
    for (const locale of ['zh-CN', 'zh', 'en']) {
      i18n.global.mergeLocaleMessage(locale, {
        'com.bedcode.session.settings.session.title': '终端会话与设备',
      })
    }
    registry.registerSettingsSection('com.bedcode.session', {
      id: 'session-settings',
      titleKey: 'settings.session.title',
      order: 150,
      component: { template: '<div data-testid="plugin-body">plugin body</div>' },
    })

    const wrapper = await mountView()
    const rendered = titles(wrapper)
    // order 150：插到「界面」与「链路加密」之间，标题取插件自身命名空间文案
    expect(rendered[0]).toBe(BUILTIN_TITLES[0])
    expect(rendered[1]).toBe('终端会话与设备')
    expect(rendered[2]).toBe(BUILTIN_TITLES[1])
    expect(wrapper.find('[data-testid="plugin-body"]').exists()).toBe(true)
  })

  it('插件进入 error 态后贡献分组被摘除，设置页回落到纯内置形态', async () => {
    registry.setPluginState('com.bedcode.session', { state: 'Activated' })
    registry.registerSettingsSection('com.bedcode.session', {
      id: 'session-settings',
      titleKey: 'settings.session.title',
      order: 150,
      component: { template: '<div data-testid="plugin-body">plugin body</div>' },
    })
    const wrapper = await mountView()
    expect(wrapper.find('[data-testid="plugin-body"]').exists()).toBe(true)

    registry.setPluginState('com.bedcode.session', { state: 'Error', error: 'wasm trap' })
    await flushPromises()

    expect(wrapper.find('[data-testid="plugin-body"]').exists()).toBe(false)
    expect(titles(wrapper)).toEqual(BUILTIN_TITLES)
  })
})
