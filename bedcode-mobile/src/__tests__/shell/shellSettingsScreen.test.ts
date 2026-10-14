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
import { flushPromises } from '@vue/test-utils'
import { ref } from 'vue'
import { mount } from '@vue/test-utils'
import zhShell from '@/locales/zh-CN/shell'
import enShell from '@/locales/en/shell'

const mockPush = vi.fn()
const mockOpenPermissions = vi.fn()
const mockConfirm = vi.fn(async () => true)
const mockClearAllData = vi.fn(async () => ({ disconnected: true, completed: true }))
const mockToastError = vi.fn()

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
// 危险区依赖宿主 composable（其内部链到 useMobileConnection 等，会把真实 vue-i18n
// 拉进来而与本文件的 vue-i18n 替身冲突）——这里替身掉，行为另由
// useClearAllData.test.ts 单独覆盖
vi.mock('@/composables/useClearAllData', () => ({
  clearAllData: (...args: unknown[]) => mockClearAllData(...(args as [])),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  confirm: (...args: unknown[]) => mockConfirm(...(args as [])),
}))
vi.mock('@/shell/composables/useToast', () => ({
  useToast: () => ({ error: mockToastError, success: vi.fn(), warning: vi.fn(), info: vi.fn() }),
}))

import ShellSettingsScreen from '@/shell/components/screens/ShellSettingsScreen.vue'

/** 按文案定位平台设置按钮 */
function entryButtons(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('button')
}

beforeEach(() => {
  mockPush.mockClear()
  mockOpenPermissions.mockClear()
  mockConfirm.mockClear()
  mockConfirm.mockResolvedValue(true)
  mockClearAllData.mockClear()
  mockClearAllData.mockResolvedValue({ disconnected: true, completed: true })
  mockToastError.mockClear()
})

describe('C-ST1 平台设置门类齐全', () => {
  // 票 2026-10-10 C4：平台项 = 链路加密 / 生物凭证 / 出站授权 / 权限总览 / 外观 / 关于。
  // 「通知」入口随业务设置下沉 terminal-session 一并退役（下方 C-ST4 反向断言钉住）。
  it('should_renderEveryPlatformEntry_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    for (const key of [
      'shell.settings.connection',
      'shell.settings.authentication',
      'shell.settings.egress',
      'shell.settings.appearance',
      'shell.settings.permissions',
      'shell.settings.about',
    ]) {
      expect(text, `缺少平台设置项 ${key}`).toContain(key)
    }
    wrapper.unmount()
  })
})

describe('C-ST4 已下沉的业务设置入口不得回接（票 2026-10-10 C4）', () => {
  it('should_notRenderBusinessSettingEntries_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    // 通知三开关 / 震动 / 声音 / 自动重连 / 首选认证的真源与 UI 归 terminal-session，
    // 壳再放一份入口就会让用户面对两个都能改、却只有一个生效的开关
    expect(text, '通知入口已下沉，不应回接').not.toContain('shell.settings.notifications')
    wrapper.unmount()
  })

  it('should_notPushRetiredNotificationRoute_when_anyEntryTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)
    for (const btn of entryButtons(wrapper)) {
      await btn.trigger('click')
    }
    const pushed = mockPush.mock.calls.map((c) => (c[0] as { name?: string }).name)
    expect(pushed, '退役路由不得再被跳转').not.toContain('mobile-settings-notifications')
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
  it('should_keepPlatformRoutesAndShellPermissions_when_appearancePermissionsAboutTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)
    const buttons = entryButtons(wrapper)

    // 票 2026-10-10 C4：通知入口退役后下标前移——3=外观 4=权限总览 5=关于
    await buttons[3].trigger('click') // 外观
    await buttons[4].trigger('click') // 权限总览 → 壳内屏
    await buttons[5].trigger('click') // 关于

    expect(mockPush.mock.calls.map((c) => (c[0] as { name: string }).name)).toEqual([
      'mobile-settings-appearance',
      'mobile-settings-about',
    ])
    expect(mockOpenPermissions).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})

describe('C-ST5 危险区「清除所有数据」（票 2026-10-10 补回 spec §1.6 回归）', () => {
  /** 定位危险区按钮（文案键即渲染文本：useI18n 替身回显 key） */
  function dangerButton(wrapper: ReturnType<typeof mount>) {
    return entryButtons(wrapper).find((b) => b.text().includes('shell.settings.clearAllData'))!
  }

  it('should_renderDangerEntry_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()
    expect(text, '应有危险区标题').toContain('shell.settings.dangerZone')
    expect(text, '应有擦除入口').toContain('shell.settings.clearAllData')
    expect(dangerButton(wrapper), '危险区按钮应可定位').toBeTruthy()
    wrapper.unmount()
  })

  it('should_gateWipeBehindConfirm_when_dangerEntryTapped', async () => {
    // 不可撤销动作：必须先过二次确认，否则误触代价是重新配对 + 重建全部配置。
    // 用 deferred 让确认停在未决状态，直接验证「确认是擦除的闸门」这一顺序契约，
    // 而不是只验证 confirm 被调用过（那测不出有没有真的挡住执行）
    let settle!: (ok: boolean) => void
    mockConfirm.mockImplementationOnce(() => new Promise<boolean>((res) => (settle = res)))

    const wrapper = mount(ShellSettingsScreen)
    await dangerButton(wrapper).trigger('click')

    expect(mockConfirm, '擦除前必须弹确认').toHaveBeenCalledTimes(1)
    expect(mockClearAllData, '确认未决时不得擦除').not.toHaveBeenCalled()

    settle(true)
    await flushPromises()

    expect(mockClearAllData, '确认通过后才擦除').toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('should_notWipe_when_userCancels', async () => {
    mockConfirm.mockResolvedValueOnce(false as never)
    const wrapper = mount(ShellSettingsScreen)
    await dangerButton(wrapper).trigger('click')

    expect(mockConfirm).toHaveBeenCalledTimes(1)
    expect(mockClearAllData, '取消后不得擦除').not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('should_wipeWhen_confirmed', async () => {
    const wrapper = mount(ShellSettingsScreen)
    await dangerButton(wrapper).trigger('click')

    expect(mockClearAllData).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('should_reportFailureAndNotClaimSuccess_when_wipeIncomplete', async () => {
    mockClearAllData.mockResolvedValueOnce({
      disconnected: false,
      completed: false,
      error: 'db locked',
    } as never)
    const wrapper = mount(ShellSettingsScreen)
    await dangerButton(wrapper).trigger('click')
    await wrapper.vm.$nextTick()

    // 清理没做完就不能让用户以为已清干净（真源 composable 此时也不会 reload）
    expect(mockToastError, '失败必须可见').toHaveBeenCalledWith('shell.settings.clearAllDataFailed')
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
    'dangerZone',
    'clearAllData',
    'clearAllDataHint',
    'clearAllDataFailed',
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
