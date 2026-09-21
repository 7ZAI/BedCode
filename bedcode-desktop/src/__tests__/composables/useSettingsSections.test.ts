import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { getPluginRegistry } from '@/plugin/registry'
import { createPluginContext } from '@/plugin/context'
import {
  useSettingsSections,
  BUILTIN_SECTION_ORDERS,
  type SettingsSharedState,
} from '@/composables/useSettingsSections'
import { isValidPermission, hasPermissionForApi } from '@/plugin/permission'
import { makePluginInfo } from '@/__tests__/fixtures/plugin'

/**
 * 设置分组扩展点测试 — 注册 / 排序 / 权限门 / 停用与 error 态摘除
 *
 * 只断言外部可见行为：分组列表的 key 顺序、传给分组的 props、权限门抛错与
 * 摘除后的列表形态；不测内部函数，不用快照替代行为断言。
 */
describe('useSettingsSections', () => {
  const registry = getPluginRegistry()
  const disposables: { dispose: () => void }[] = []
  const usedPluginIds = new Set<string>()

  /** 下传的共享状态（语言 / 动画），由设置页父级持有 */
  function makeShared(): SettingsSharedState {
    return {
      languageOptions: [
        { value: 'zh-CN', label: '中文' },
        { value: 'en', label: 'English' },
      ],
      currentLanguage: 'zh-CN',
      animationsEnabled: true,
      onSwitchLanguage: () => {},
      onToggleAnimations: () => {},
    }
  }

  /** 模拟插件激活后贡献一个设置分组（登记运行态即视为贡献生效） */
  function contribute(pluginId: string, id: string, order?: number): void {
    registry.setPluginState(pluginId, { state: 'Activated' })
    usedPluginIds.add(pluginId)
    disposables.push(
      registry.registerSettingsSection(pluginId, {
        id,
        titleKey: `settings.${id}.title`,
        icon: 'M4 6h16M4 12h16M4 18h7',
        order,
        component: { name: `${pluginId}-${id}` },
      }),
    )
  }

  function keys(): string[] {
    return useSettingsSections(makeShared).sections.value.map((s) => s.key)
  }

  beforeEach(() => {
    disposables.length = 0
  })

  afterEach(() => {
    for (const d of disposables.splice(0)) {
      d.dispose()
    }
    // 清掉运行态登记，避免污染后续用例（注册表为进程级单例）
    for (const id of usedPluginIds) {
      registry.clearPlugin(id)
    }
    usedPluginIds.clear()
  })

  it('未注册贡献时只有内置分组，顺序与改造前逐分组一致（退役：pairing 票 14 / session 随域下沉）', () => {
    expect(keys()).toEqual(['appearance', 'linkCrypto', 'system', 'logging', 'about'])
  })

  it('内置分组排序槽位间隔 100，且「关于」恒在最末', () => {
    expect(BUILTIN_SECTION_ORDERS.about).toBeGreaterThan(BUILTIN_SECTION_ORDERS.logging)
    const sections = useSettingsSections(makeShared).sections.value
    const orders = sections.map((s) => s.order)
    expect(orders).toEqual([...orders].sort((a, b) => a - b))
  })

  it('贡献分组缺省 order 时排在「日志」之后、「关于」之前', () => {
    contribute('com.bedcode.session', 'session-settings')
    expect(keys()).toEqual([
      'appearance',
      'linkCrypto',
      'system',
      'logging',
      'plugin-com.bedcode.session-session-settings',
      'about',
    ])
  })

  it('贡献分组可通过 order 插入任意内置分组之间', () => {
    contribute('com.bedcode.session', 'pairing', 150)
    contribute('com.bedcode.session', 'task', 450)
    expect(keys()).toEqual([
      'appearance',
      'plugin-com.bedcode.session-pairing',
      'linkCrypto',
      'plugin-com.bedcode.session-task',
      'system',
      'logging',
      'about',
    ])
  })

  it('同 order 时内置分组排在贡献分组之前（稳定排序）', () => {
    contribute('com.bedcode.session', 'tie', BUILTIN_SECTION_ORDERS.logging)
    const list = keys()
    expect(list.indexOf('logging')).toBeLessThan(list.indexOf('plugin-com.bedcode.session-tie'))
  })

  it('共享状态由父级下传：内置外观分组与贡献分组拿到同一份语言/动画状态', () => {
    const shared = makeShared()
    contribute('com.bedcode.session', 'session-settings')
    const sections = useSettingsSections(() => shared).sections.value
    const appearance = sections.find((s) => s.key === 'appearance')!
    const contributed = sections.find((s) => s.key === 'plugin-com.bedcode.session-session-settings')!

    expect(appearance.props.languageOptions).toBe(shared.languageOptions)
    expect(appearance.props.onSwitchLanguage).toBe(shared.onSwitchLanguage)
    // 贡献分组只消费共享状态，不自行推导语言/动画
    expect(contributed.props.shared).toBe(shared)
  })

  it('插件停用（clearPlugin）后贡献分组被摘除，列表回到纯内置形态', () => {
    contribute('com.bedcode.session', 'session-settings')
    expect(keys()).toContain('plugin-com.bedcode.session-session-settings')

    registry.clearPlugin('com.bedcode.session')
    expect(keys()).not.toContain('plugin-com.bedcode.session-session-settings')
    // 内置分组共 5 项（原 7 项中的 pairing「票 14」与 session「随域下沉」已退役）
    expect(keys()).toHaveLength(5)
  })

  it('插件进入 error 态后贡献分组被摘除，恢复激活后重新出现', () => {
    contribute('com.bedcode.session', 'session-settings')
    expect(keys()).toContain('plugin-com.bedcode.session-session-settings')

    registry.setPluginState('com.bedcode.session', { state: 'Error', error: 'trap' })
    expect(keys()).not.toContain('plugin-com.bedcode.session-session-settings')

    registry.setPluginState('com.bedcode.session', { state: 'Activated' })
    expect(keys()).toContain('plugin-com.bedcode.session-session-settings')
  })

  it('Degraded 态（实例仍在运行）保留贡献分组', () => {
    contribute('com.bedcode.session', 'session-settings')
    registry.setPluginState('com.bedcode.session', { state: 'Degraded', error: 'startup failed' })
    expect(keys()).toContain('plugin-com.bedcode.session-session-settings')
  })

  it('未登记运行态的插件（尚未激活完成）贡献不生效', () => {
    usedPluginIds.add('com.bedcode.ghost')
    disposables.push(
      registry.registerSettingsSection('com.bedcode.ghost', {
        id: 'ghost',
        titleKey: 'settings.ghost.title',
        component: {},
      }),
    )
    expect(keys()).not.toContain('plugin-com.bedcode.ghost-ghost')
  })

  it('dispose 后贡献分组从注册表摘除', () => {
    const d = registry.registerSettingsSection('com.bedcode.session', {
      id: 'session-settings',
      titleKey: 'settings.session.title',
      component: {},
    })
    registry.setPluginState('com.bedcode.session', { state: 'Activated' })
    usedPluginIds.add('com.bedcode.session')

    expect(keys()).toContain('plugin-com.bedcode.session-session-settings')
    d.dispose()
    expect(keys()).not.toContain('plugin-com.bedcode.session-session-settings')
  })
})

describe('ui:settings 权限门', () => {
  it('ui:settings 是合法权限且只映射到 ui.registerSettingsSection', () => {
    expect(isValidPermission('ui:settings')).toBe(true)
    expect(hasPermissionForApi(['ui:settings'], 'ui.registerSettingsSection')).toBe(true)
    expect(hasPermissionForApi(['ui:settings'], 'ui.registerSidebarPanel')).toBe(false)
    expect(hasPermissionForApi(['ui:sidebar'], 'ui.registerSettingsSection')).toBe(false)
  })

  it('缺 ui:settings 权限时 registerSettingsSection 抛错，授权后返回可释放句柄', () => {
    const denied = createPluginContext(makePluginInfo({ id: 'com.bedcode.denied', permissions: [] }))
    expect(() =>
      denied.ui.registerSettingsSection({ id: 'x', titleKey: 'x.title', component: {} }),
    ).toThrow(/lacks permission for ui\.registerSettingsSection/)

    const registry = getPluginRegistry()
    const granted = createPluginContext(
      makePluginInfo({ id: 'com.bedcode.granted', permissions: ['ui:settings'] }),
    )
    const d = granted.ui.registerSettingsSection({
      id: 'x',
      titleKey: 'x.title',
      component: {},
    })
    registry.setPluginState('com.bedcode.granted', { state: 'Activated' })
    expect(registry.settingsSectionsRef.value.map((s) => `${s.pluginId}:${s.id}`)).toContain(
      'com.bedcode.granted:x',
    )

    d.dispose()
    registry.clearPlugin('com.bedcode.granted')
    expect(registry.settingsSectionsRef.value).toHaveLength(0)
  })
})
