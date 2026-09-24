/**
 * 插件窗口宿主的渲染契约（`views/PluginWindowHostView.vue`）
 *
 * 背景（2026-09-24）：终端窗口打开即白屏 + 「插件视图未找到」。根因是窗口路由在
 * `loadAll` 完成前就挂载了插件视图宿主——那时既拿不到视图组件（旧实现读非响应式
 * Map，undefined 被永久缓存），也没机会在 setup 里 provide `pluginContext`。
 * 修复后宿主改为「通用插件窗口」：目标插件/视图可指定，渲染前先等目标就绪。
 *
 * 被测行为（用 loader 替身控制启动加载节奏，复现真实时序）：
 * - 未就绪：显示加载态，不渲染插件视图
 * - 缺省目标：`/terminal-window/:id` → 会话插件 `session.terminal-window`
 * - 通用目标：`/plugin/window/:pluginId/:viewId` 与 query 覆盖 → 指定插件视图
 * - 就绪后缺失目标：显性报出 pluginId/viewId，不白屏
 * - 晚注册自愈：缺失态之后完成注册 → 自动渲染
 * - 未随启动加载的插件：走一次懒激活
 * - 宿主终端能力桥（宿主存储/命令面原语）仍能注入插件视图
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import { defineComponent, inject, nextTick, type Component } from 'vue'
import i18n from '@/locales'
import PluginWindowHostView from '@/views/PluginWindowHostView.vue'
import { getPluginRegistry } from '@/plugin/registry'
import { logger } from '@/utils/frontendLogger'

const SESSION_PLUGIN = 'com.bedcode.terminal-session'
const SESSION_WINDOW_VIEW = 'session.terminal-window'
const OTHER_PLUGIN = 'com.bedcode.other-plugin'
const OTHER_WINDOW_VIEW = 'other.window'

// ==================== 替身：启动加载节奏 + Tauri 边界 ====================

const hooks = vi.hoisted(() => {
  // 每个用例一条独立的「启动加载闸门」：放行即模拟 loadAll 结束，
  // 不放行则复现「窗口已挂载、插件仍在加载」的真实首帧时序
  const state: { gate: Promise<void>; release: () => void } = {
    gate: Promise.resolve(),
    release: () => {},
  }
  function resetGate(): void {
    state.gate = new Promise<void>((resolve) => {
      state.release = resolve
    })
  }
  resetGate()
  return {
    /** 启动加载句柄：测试可控地「放行」 */
    ensureLoaded: vi.fn(() => state.gate),
    activate: vi.fn(async () => {}),
    getActivePlugin: vi.fn(() => undefined as unknown),
    releaseStartupLoad: () => state.release(),
    resetGate,
  }
})

vi.mock('@/plugin/loader', () => ({
  pluginLoader: {
    ensureLoaded: hooks.ensureLoaded,
    activate: hooks.activate,
    getActivePlugin: hooks.getActivePlugin,
  },
}))

// 终端能力桥依赖宿主存储/命令面： Tauri 边界桩掉（插件视图自身也经过这里）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
  convertFileSrc: (p: string) => p,
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(async () => null),
}))

// ==================== 测试用插件视图 ====================

/**
 * 插件视图替身：把「是否拿到 pluginContext」与「是否拿到宿主能力注入」渲染出来。
 *
 * 这两个上下文都只能在宿主组件 setup 期 provide —— 渲染出来即证明宿主的提供时机正确。
 */
function makePluginView(label: string): Component {
  return defineComponent({
    setup() {
      const ctx = inject<{ id?: string } | null>('pluginContext', null)
      const caps = inject<unknown>('terminalHostCapabilities', null)
      return { label, ctxId: ctx?.id ?? 'no-context', hasCaps: caps !== null }
    },
    template: `<div data-testid="plugin-view">{{ label }}|{{ ctxId }}|{{ hasCaps ? 'caps' : 'no-caps' }}</div>`,
  })
}

/** 目标插件进入「已加载」状态（PluginViewHost 据此拿到 context） */
function markPluginLoaded(pluginId: string): void {
  hooks.getActivePlugin.mockReturnValue({ id: pluginId })
  getPluginRegistry().setContext(pluginId, { id: pluginId } as never)
}

/** 注册目标视图（模拟插件 activate 里的 registerPage） */
function registerView(pluginId: string, viewId: string, component: Component): void {
  getPluginRegistry().registerView(pluginId, 'page', { id: viewId, title: viewId, component })
}

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/terminal-window/:id', name: 'terminal-window', component: PluginWindowHostView },
      {
        path: '/plugin/window/:pluginId/:viewId',
        name: 'plugin-window-view',
        component: PluginWindowHostView,
      },
    ],
  })
}

async function mountHost(path: string): Promise<VueWrapper> {
  const router = makeRouter()
  await router.push(path)
  await router.isReady()
  const wrapper = mount(PluginWindowHostView, {
    global: { plugins: [createPinia(), i18n, router] },
  })
  await nextTick()
  return wrapper
}

describe('PluginWindowHostView 渲染契约', () => {
  const registry = getPluginRegistry()
  let errorSpy: ReturnType<typeof vi.spyOn>
  let warnSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    hooks.resetGate()
    hooks.ensureLoaded.mockClear()
    hooks.activate.mockClear()
    hooks.getActivePlugin.mockReturnValue(undefined)
    registry.clearPlugin(SESSION_PLUGIN)
    registry.clearPlugin(OTHER_PLUGIN)
    warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {})
    errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  })

  it('启动加载未完成时显示加载态，不渲染插件视图（不白屏）', async () => {
    const wrapper = await mountHost('/terminal-window/session-1')

    expect(wrapper.text()).toContain(i18n.global.t('desktop.plugin.windowLoading'))
    expect(wrapper.find('[data-testid="plugin-view"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('终端窗口路由缺省渲染会话插件的终端视图，并带宿主能力注入与 pluginContext', async () => {
    const wrapper = await mountHost('/terminal-window/session-1')
    markPluginLoaded(SESSION_PLUGIN)
    registerView(SESSION_PLUGIN, SESSION_WINDOW_VIEW, makePluginView('terminal'))

    hooks.releaseStartupLoad()
    await flushPromises()
    await nextTick()

    const view = wrapper.find('[data-testid="plugin-view"]')
    expect(view.exists()).toBe(true)
    expect(view.text()).toBe(`terminal|${SESSION_PLUGIN}|caps`)
    // 已在启动加载里就绪 → 不得再触发一次后端激活
    expect(hooks.activate).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('通用窗口路由渲染指定插件的视图（不受终端缺省目标影响）', async () => {
    const wrapper = await mountHost(`/plugin/window/${OTHER_PLUGIN}/${OTHER_WINDOW_VIEW}`)
    markPluginLoaded(OTHER_PLUGIN)
    // 干扰项：会话插件的终端视图也注册了，但目标必须是路由指定的那个
    registerView(SESSION_PLUGIN, SESSION_WINDOW_VIEW, makePluginView('terminal'))
    registerView(OTHER_PLUGIN, OTHER_WINDOW_VIEW, makePluginView('other'))

    hooks.releaseStartupLoad()
    await flushPromises()
    await nextTick()

    expect(wrapper.find('[data-testid="plugin-view"]').text()).toBe(`other|${OTHER_PLUGIN}|caps`)
    wrapper.unmount()
  })

  it('query 参数可覆盖目标（同一 route 记录复用时的多级深链）', async () => {
    const wrapper = await mountHost(
      `/terminal-window/session-1?pluginId=${encodeURIComponent(OTHER_PLUGIN)}&viewId=${OTHER_WINDOW_VIEW}`,
    )
    markPluginLoaded(OTHER_PLUGIN)
    registerView(OTHER_PLUGIN, OTHER_WINDOW_VIEW, makePluginView('query'))

    hooks.releaseStartupLoad()
    await flushPromises()
    await nextTick()

    expect(wrapper.find('[data-testid="plugin-view"]').text()).toBe(`query|${OTHER_PLUGIN}|caps`)
    wrapper.unmount()
  })

  it('就绪后仍缺目标视图 → 显性报出 pluginId / viewId（fail-visible）', async () => {
    const wrapper = await mountHost(`/plugin/window/${OTHER_PLUGIN}/${OTHER_WINDOW_VIEW}`)

    hooks.releaseStartupLoad()
    await flushPromises()
    await nextTick()

    expect(wrapper.find('[data-testid="plugin-view"]').exists()).toBe(false)
    const text = wrapper.text()
    expect(text).toContain(OTHER_PLUGIN)
    expect(text).toContain(OTHER_WINDOW_VIEW)
    expect(text).not.toContain(i18n.global.t('desktop.plugin.windowLoading'))
    // 缺失必须留痕（不能静默吞掉）
    expect(warnSpy).toHaveBeenCalled()
    wrapper.unmount()
  })

  it('缺失态之后插件补注册 → 自动渲染（晚注册自愈）', async () => {
    const wrapper = await mountHost(`/plugin/window/${OTHER_PLUGIN}/${OTHER_WINDOW_VIEW}`)
    hooks.releaseStartupLoad()
    await flushPromises()
    expect(wrapper.find('[data-testid="plugin-view"]').exists()).toBe(false)

    markPluginLoaded(OTHER_PLUGIN)
    registerView(OTHER_PLUGIN, OTHER_WINDOW_VIEW, makePluginView('late'))
    await nextTick()
    await flushPromises()

    expect(wrapper.find('[data-testid="plugin-view"]').text()).toBe(`late|${OTHER_PLUGIN}|caps`)
    wrapper.unmount()
  })

  it('启动加载里未就绪的插件走一次懒激活', async () => {
    const wrapper = await mountHost('/terminal-window/session-1')
    hooks.getActivePlugin.mockReturnValue(undefined)

    hooks.releaseStartupLoad()
    await flushPromises()

    expect(hooks.activate).toHaveBeenCalledTimes(1)
    expect(hooks.activate).toHaveBeenCalledWith(SESSION_PLUGIN)
    wrapper.unmount()
  })

  it('懒激活失败不影响窗口可用（错误留痕，宿主不吞）', async () => {
    hooks.activate.mockRejectedValueOnce(new Error('plugin backend unavailable'))
    const wrapper = await mountHost('/terminal-window/session-1')

    hooks.releaseStartupLoad()
    await flushPromises()
    await nextTick()

    expect(errorSpy).toHaveBeenCalled()
    expect(wrapper.text()).toContain(SESSION_PLUGIN)
    expect(wrapper.find('[data-testid="plugin-view"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('启动加载未结束前不发起懒激活（两段式：先等 loadAll，再决定是否 activate）', async () => {
    const wrapper = await mountHost('/terminal-window/session-1')

    await flushPromises()

    expect(hooks.activate).not.toHaveBeenCalled()
    expect(wrapper.text()).toContain(i18n.global.t('desktop.plugin.windowLoading'))
    wrapper.unmount()
  })
})
