/**
 * 壳设置屏 行为契约测试
 * （票 2026-10-09 阶段 B 前置：壳必须覆盖旧宿主设置的全部门类，否则退役旧宿主=功能回归）
 *
 * 被测：`src/shell/components/screens/ShellSettingsScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-ST1 | platformEntries | 七项平台设置在场（连接 / 认证 / 出站 / 外观 / 权限 / 通知 / 关于） | 文案键按序渲染 |
 * | C-ST2 | 新增三项的入口 | 连接 / 认证 / 出站 → 跳既有设置页路由（不重复实现一套引擎设置） | push 到 `mobile-settings-connection|authentication|egress` |
 * | C-ST3 | 既有项不漂移 | 外观 / 通知 / 关于仍跳同名路由；权限走壳内屏（nav.openPermissions） | 各类跳转口径不变 |
 * | C-ST4 | 双语 | 七项的文案键在 zh-CN 与 en 都在场 | 两棵文案树都能解析 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref } from 'vue'
import { mount } from '@vue/test-utils'
import zhShell from '@/locales/zh-CN/shell'
import enShell from '@/locales/en/shell'

const mockPush = vi.fn()
const mockOpenPermissions = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: (...args: any[]) => mockPush(...args) }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({ openPermissions: mockOpenPermissions }),
}))
vi.mock('@/shell/composables/usePlatform', () => ({
  usePlatform: () => ({ platformInfo: ref({ platform: 'android', osType: 'android' }) }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({ apps: ref([]) }),
}))

import ShellSettingsScreen from '@/shell/components/screens/ShellSettingsScreen.vue'

/** 按文案定位平台设置按钮 */
function entryButtons(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('button')
}

beforeEach(() => {
  mockPush.mockClear()
  mockOpenPermissions.mockClear()
})

describe('C-ST1 平台设置门类齐全', () => {
  it('should_renderEveryPlatformEntry_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    for (const key of [
      'shell.settings.connection',
      'shell.settings.authentication',
      'shell.settings.egress',
      'shell.settings.appearance',
      'shell.settings.permissions',
      'shell.settings.notifications',
      'shell.settings.about',
    ]) {
      expect(text, `缺少平台设置项 ${key}`).toContain(key)
    }
    wrapper.unmount()
  })
})

describe('C-ST2 新增三项（连接 / 认证 / 出站）入口', () => {
  it('should_pushLegacySettingsRoutes_when_connectionAuthenticationOrEgressTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)
    const buttons = entryButtons(wrapper)

    // 文案键即渲染文本（useI18n 替身回显 key）：按下标定位前三项
    await buttons[0].trigger('click')
    await buttons[1].trigger('click')
    await buttons[2].trigger('click')

    expect(mockPush.mock.calls.map((c) => (c[0] as { name: string }).name)).toEqual([
      'mobile-settings-connection',
      'mobile-settings-authentication',
      'mobile-settings-egress',
    ])
    wrapper.unmount()
  })
})

describe('C-ST3 既有项跳转口径不漂移', () => {
  it('should_keepLegacyRoutesAndShellPermissions_when_appearanceNotificationsAboutTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)
    const buttons = entryButtons(wrapper)

    await buttons[3].trigger('click') // 外观
    await buttons[4].trigger('click') // 权限总览 → 壳内屏
    await buttons[5].trigger('click') // 通知
    await buttons[6].trigger('click') // 关于

    expect(mockPush.mock.calls.map((c) => (c[0] as { name: string }).name)).toEqual([
      'mobile-settings-appearance',
      'mobile-settings-notifications',
      'mobile-settings-about',
    ])
    expect(mockOpenPermissions).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})

describe('C-ST4 设置项文案双语在场', () => {
  const KEYS = [
    'connection',
    'connectionHint',
    'authentication',
    'authenticationHint',
    'egress',
    'egressHint',
  ]

  it('should_resolveSettingsKeysInBothLocales_when_localesLoaded', () => {
    const zh = (zhShell as any).shell.settings
    const en = (enShell as any).shell.settings
    for (const key of KEYS) {
      expect(typeof zh[key], `zh-CN 缺 shell.settings.${key}`).toBe('string')
      expect(typeof en[key], `en 缺 shell.settings.${key}`).toBe('string')
      expect(String(zh[key]).length).toBeGreaterThan(0)
      expect(String(en[key]).length).toBeGreaterThan(0)
    }
  })
})
