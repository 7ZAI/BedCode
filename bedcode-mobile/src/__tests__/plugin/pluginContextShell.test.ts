/**
 * 插件上下文 → 宿主壳注册桥 行为契约测试
 * （票 2026-10-09-mobile-host-into-wasm-apps，阶段 A2：插件前端可注册壳运行面）
 *
 * 被测：`src/plugin/context.ts` 的 `ui.registerSurface / registerSlot /
 * registerCapsuleItem / registerSettingsEntry`（壳侧落点 `src/shell/registry.ts`）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | registerSurface | appId 由宿主代填为插件 id；其它应用不受影响 | 挂在本插件 id 下，别的 id 无运行面 |
 * | C-S2 | 运行面包装 | 壳直渲染运行面时插件组件仍能拿到 pluginContext | 探针渲染出 ctx:<插件 id> |
 * | C-S3 | dispose 语义 | 返回值入 `_disposables`（loader 统一回收）且 dispose 即时摘除 | 壳侧解析不到运行面 |
 * | C-S4 | 免权限 | 运行面注册不做 ui:* 权限快速失败（不触发宿主能力） | 无权限不抛；同上下文旧嵌入面仍抛 |
 * | C-S5 | 三面贡献 | slot / capsule / settings 落到壳注册表对应集合 | 各集合含注册 id，order 透传 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { defineComponent, h, inject } from 'vue'
import { mount } from '@vue/test-utils'
import { createPluginContext } from '@/plugin/context'
import { getPluginRegistry } from '@/plugin/registry'
import { getShellRegistry } from '@/shell/registry'
import type { PluginInfo } from '@/plugin/types'
import type { ShellApp, ShellAppSource } from '@/shell/types'

// 运行面经 PluginViewHost 包装（内部 useI18n）；本测试只验证包装行为本身，文案用替身
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

const PLUGIN_ID = 'com.test.surface-bridge'
const OTHER_APP_ID = 'com.test.other-app'
const SOURCE_ID = 'shell-bridge-test-source'

const shell = getShellRegistry()
const pluginRegistry = getPluginRegistry()

/** 最小应用描述（壳注册表的写入形状） */
function makeApp(id: string): ShellApp {
  return {
    id,
    name: id,
    version: '1.0.0',
    state: 'stopped',
    permissions: [],
    contributions: {},
  }
}

/** 最小插件信息（权限按用例给） */
function makeInfo(permissions: string[] = []): PluginInfo {
  return {
    id: PLUGIN_ID,
    name: 'Surface Bridge Test',
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'wasm' as any,
    permissions,
    state: 'activated' as any,
    contributes: {} as any,
    source: '',
    assetDir: '',
  } as PluginInfo
}

/**
 * 探针组件：把 inject 到的 pluginContext.id 渲染成文本。
 * 缺 provide 时渲染 ctx-missing —— 包装层若被摘掉，C-S2 立即测红。
 */
const CtxProbe = defineComponent({
  name: 'CtxProbe',
  setup() {
    const ctx = inject<{ id?: string } | undefined>('pluginContext')
    return () => h('div', { class: 'ctx-probe' }, ctx ? `ctx:${ctx.id}` : 'ctx-missing')
  },
})

/** 建一个已登记上下文的插件上下文（PluginViewHost 经注册表取上下文） */
function makeContext(permissions: string[] = []) {
  const ctx = createPluginContext(makeInfo(permissions))
  pluginRegistry.setContext(PLUGIN_ID, ctx)
  return ctx
}

beforeEach(() => {
  const source: ShellAppSource = {
    id: SOURCE_ID,
    list: async () => [],
    launch: async () => {},
    stop: async () => {},
  }
  shell.registerSource(source)
  shell.upsertApps(SOURCE_ID, [makeApp(PLUGIN_ID), makeApp(OTHER_APP_ID)])
})

afterEach(() => {
  shell.clearApp(PLUGIN_ID)
  shell.clearApp(OTHER_APP_ID)
  shell.clearSource(SOURCE_ID)
  pluginRegistry.clearPlugin(PLUGIN_ID)
})

describe('C-S1 运行面注册归属', () => {
  it('should_registerSurfaceUnderPluginIdOnly_when_registered', () => {
    const ctx = makeContext()
    ctx.ui.registerSurface({ component: CtxProbe })

    expect(shell.getApp(PLUGIN_ID)?.contributions.surface?.component).toBeTruthy()
    // 反例：appId 代填不得串台——其它应用不应拿到这份运行面
    expect(shell.getApp(OTHER_APP_ID)?.contributions.surface).toBeUndefined()
  })
})

describe('C-S2 运行面包装补 pluginContext', () => {
  it('should_providePluginContextToSurfaceComponent_when_shellRendersIt', () => {
    const ctx = makeContext()
    ctx.ui.registerSurface({ component: CtxProbe })

    const surface = shell.getApp(PLUGIN_ID)?.contributions.surface?.component
    const wrapper = mount(surface as any, { props: { app: shell.getApp(PLUGIN_ID) } })

    expect(wrapper.find('.ctx-probe').exists()).toBe(true)
    expect(wrapper.text()).toContain(`ctx:${PLUGIN_ID}`)
    wrapper.unmount()
  })
})

describe('C-S3 运行面回收', () => {
  it('should_recordDisposableAndDropSurface_when_disposed', () => {
    const ctx = makeContext()
    const disposable = ctx.ui.registerSurface({ component: CtxProbe })

    // loader 依 `_disposables` 统一摘除（停用回收不能漏这一面）
    expect(ctx._disposables).toContain(disposable)
    expect(shell.getApp(PLUGIN_ID)?.contributions.surface?.component).toBeTruthy()

    disposable.dispose()
    expect(shell.getApp(PLUGIN_ID)?.contributions.surface).toBeUndefined()
  })
})

describe('C-S4 运行面免权限快速失败', () => {
  it('should_notRequireUiPermission_when_registerSurface', () => {
    const ctx = makeContext([])

    // 运行面不触发宿主能力 ⇒ 不设权限门
    expect(() => ctx.ui.registerSurface({ component: CtxProbe })).not.toThrow()
  })
})

describe('C-S6 已退役扩展点显性抛错（票 2026-10-10 C2 fail-visible）', () => {
  /** 退役方法不在 UIRegistry 类型面上（编译期就该红），测试只能按运行时形状取 */
  function retiredUiOf(permissions: string[] = []): Record<string, (...a: never[]) => unknown> {
    return makeContext(permissions).ui as unknown as Record<string, (...a: never[]) => unknown>
  }

  it('should_throwNamingTheApi_when_retiredExtensionPointCalled', () => {
    // 断链必须当场可见且指名扩展点 + 给出迁移出路；
    // 拿 undefined 会在插件内部炸成「不是函数」，把作者的排查方向带偏
    const ui = retiredUiOf()

    for (const api of [
      'registerToolboxPage',
      'registerNavTab',
      'registerTerminalToolbarItem',
      'registerTerminalView',
    ]) {
      expect(() => ui[api]({} as never), `${api} 应显性抛错`).toThrow(api)
      // 报错必须指向迁移出路，否则作者只知道「没了」不知道「改成什么」
      expect(() => ui[api]({} as never), `${api} 报错应指向 registerSurface`).toThrow(
        'registerSurface',
      )
    }
  })

  it('should_throwRegardlessOfPermissions_when_retiredExtensionPointCalled', () => {
    // 权限不是退役扩展点的准入条件：即便声明了对应权限位也照样抛——
    // 否则「有权限就能用旧面」会让人以为退役只是权限收紧
    const ui = retiredUiOf(['ui:toolbox', 'ui:navtab', 'ui:input'])
    expect(() => ui.registerToolboxPage({} as never)).toThrow('registerToolboxPage')
    expect(() => ui.registerTerminalView({} as never)).toThrow('registerTerminalView')
  })
})

describe('C-S5 slot / capsule / settings 三面贡献', () => {
  it('should_routeContributionsToShellRegistry_when_registered', () => {
    const ctx = makeContext()

    const slot = ctx.ui.registerSlot({ id: 'host-sessions', component: CtxProbe, order: 10 })
    const capsule = ctx.ui.registerCapsuleItem({ id: 'cap-1', label: 'Capsule', order: 20 })
    const entry = ctx.ui.registerSettingsEntry({ id: 'set-1', label: 'Entry', hint: 'hint' })

    const contributions = shell.getApp(PLUGIN_ID)?.contributions
    expect(contributions?.slots?.map((s) => s.id)).toEqual(['host-sessions'])
    expect(contributions?.slots?.[0].order).toBe(10)
    expect(contributions?.capsuleItems?.map((c) => c.id)).toEqual(['cap-1'])
    expect(contributions?.settingsEntries?.map((e) => e.id)).toEqual(['set-1'])

    // 逐项摘除（撤回一条不影响其余两条）
    slot.dispose()
    expect(shell.getApp(PLUGIN_ID)?.contributions.slots).toEqual([])
    expect(shell.getApp(PLUGIN_ID)?.contributions.capsuleItems?.map((c) => c.id)).toEqual(['cap-1'])
    expect(shell.getApp(PLUGIN_ID)?.contributions.settingsEntries?.map((e) => e.id)).toEqual([
      'set-1',
    ])
    capsule.dispose()
    entry.dispose()
  })
})
