/**
 * 壳设置屏 行为契约测试
 * （票 2026-10-09 阶段 B 前置：壳必须覆盖旧宿主设置的全部门类，否则退役旧宿主=功能回归；
 *  2026-10-14 二改：连接 / 认证入口删除、危险区迁「关于」、背景支持自定义壁纸
 *  2026-10-15 三改：出站策略入口删除、背景（壁纸）整体迁外观设置页（契约随迁））
 *
 * 被测：`src/shell/components/screens/ShellSettingsScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-ST1 | platformEntries | 平台项 = 外观 / 权限 / 关于；出站·背景·连接·认证不在宿主 | 文案键在场 + 缺席键不出现 |
 * | C-ST2 | 入口跳转 | 三项走既有设置页路由，权限走壳内屏，返回走 nav.back | push / openPermissions / back |
 * | C-ST3 | 危险区归属 | 擦除入口已迁「关于 BedCode」，一级页不得再有 | 文案键缺席 |
 * | C-ST6 | 应用设置项兜底 | 应用没给 onSelect 时落应用详情 | nav.openDetail |
 * | C-ST7 | 双语 | 平台项与背景档文案键在 zh-CN / en 都在场 | 两棵树都能解析 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { ref } from 'vue'
import zhShell from '@/locales/zh-CN/shell'
import enShell from '@/locales/en/shell'
import type { ShellApp } from '@/shell/types'

const mockPush = vi.fn()
const mockOpenPermissions = vi.fn()
const mockNavBack = vi.fn()
const mockOpenDetail = vi.fn()
const apps = ref<ShellApp[]>([])

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: (...args: any[]) => mockPush(...args) }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({
    openPermissions: mockOpenPermissions,
    openDetail: mockOpenDetail,
    back: mockNavBack,
  }),
}))
vi.mock('@/shell/composables/usePlatform', () => ({
  usePlatform: () => ({ platformInfo: ref({ platform: 'android', osType: 'android' }) }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({ apps }),
}))
import ShellSettingsScreen from '@/shell/components/screens/ShellSettingsScreen.vue'

/**
 * 按文案定位平台设置按钮
 *
 * 不按下标定位：本屏会继续新增分组（如「背景」的壁纸色块也是 button），
 * 下标会随插入位置漂移，测试会以「点错按钮」的方式红掉而不是报出真实意图。
 */
function entryButton(wrapper: ReturnType<typeof mount>, key: string) {
  return wrapper.findAll('button').find((b) => b.text().includes(key))
}

beforeEach(async () => {
  mockPush.mockClear()
  mockOpenPermissions.mockClear()
  mockNavBack.mockClear()
  mockOpenDetail.mockClear()
  apps.value = []
})

describe('C-ST1 平台设置门类', () => {
  it('should_renderHostOwnedEntries_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    for (const key of [
      'shell.settings.appearance',
      'shell.settings.permissions',
      'shell.settings.about',
    ]) {
      expect(text, `缺少平台设置项 ${key}`).toContain(key)
    }
    // 2026-10-15：出站策略入口已移除、背景迁外观设置页——壳设置页只剩平台项与应用设置
    expect(text, '出站策略入口不应回接').not.toContain('shell.settings.egress')
    expect(text, '背景区块不应回接').not.toContain('shell.settings.background')
    wrapper.unmount()
  })

  it('should_notRenderConnectionOrAuthentication_when_mounted', () => {
    // 2026-10-14：连接与凭据归各应用自持，不是宿主面——入口从平台设置删除
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    expect(text, '连接入口不应回接').not.toContain('shell.settings.connection')
    expect(text, '认证入口不应回接').not.toContain('shell.settings.authentication')
    wrapper.unmount()
  })

  it('should_notRenderDangerZone_when_mounted', () => {
    // 危险区整段迁入「关于 BedCode」（AboutSettingsView），一级页不再出现
    const wrapper = mount(ShellSettingsScreen)

    expect(wrapper.text()).not.toContain('shell.settings.dangerZone')
    expect(wrapper.text()).not.toContain('shell.settings.clearAllData')
    wrapper.unmount()
  })
})

describe('C-ST2 入口跳转口径', () => {
  it('should_pushLegacySettingsRoutes_when_entriesTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)

    // 文案键即渲染文本（useI18n 替身回显 key）：按 key 定位，不依赖按钮下标
    await entryButton(wrapper, 'shell.settings.appearance')!.trigger('click')
    await entryButton(wrapper, 'shell.settings.about')!.trigger('click')

    expect(mockPush.mock.calls.map((c) => (c[0] as { name: string }).name)).toEqual([
      'mobile-settings-appearance',
      'mobile-settings-about',
    ])
    wrapper.unmount()
  })

  it('should_openShellPermissions_when_permissionsEntryTapped', async () => {
    const wrapper = mount(ShellSettingsScreen)

    await entryButton(wrapper, 'shell.settings.permissions')!.trigger('click')

    expect(mockOpenPermissions).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('should_callNavBack_when_backButtonTapped', async () => {
    // 系统宫格进入本屏（不是栈底）：顶部返回是主屏之外的唯一退路（底部系统条已退役）。
    // 返回键是图标按钮（无可读文本），按 aria-label 定位
    const wrapper = mount(ShellSettingsScreen)
    const back = wrapper.find('button[aria-label="shell.detail.back"]')
    expect(back.exists(), '顶部返回按钮应在场').toBe(true)
    await back.trigger('click')
    expect(mockNavBack).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})

describe('C-ST6 应用设置项兜底', () => {
  it('should_openAppDetail_when_entryHasNoHandler', async () => {
    apps.value = [
      {
        id: 'app-1',
        name: 'AppOne',
        version: '1.0.0',
        state: 'stopped',
        permissions: [],
        contributions: { settingsEntries: [{ id: 'e1', label: '无处理器的入口' }] },
      },
    ]
    const wrapper = mount(ShellSettingsScreen)

    await entryButton(wrapper, '无处理器的入口')!.trigger('click')

    expect(mockOpenDetail, '没有 onSelect 时落应用详情，不留死点击').toHaveBeenCalledWith('app-1')
    wrapper.unmount()
  })
})

describe('C-ST7 设置项文案双语在场', () => {
  const KEYS = [
    'about',
    'appSettings',
    'runtime',
    'appearance',
    'appearanceHint',
    'permissions',
    'permissionsHint',
    'platform',
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

  it('should_notKeepRetiredSettingsKeys_when_localesLoaded', () => {
    // 连接 / 认证 / 出站策略 / 背景迁走后，文案键一并删除——留着会诱导下一个人
    // 「反正有 key」把入口接回来
    for (const locale of [((zhShell as any).shell.settings), ((enShell as any).shell.settings)]) {
      for (const key of [
        'connection', 'connectionHint',
        'authentication', 'authenticationHint',
        'dangerZone', 'clearAllData', 'clearAllDataHint', 'clearAllDataFailed',
        'egress', 'egressHint',
        'background', 'backgroundNote',
        'wallNone', 'wallAurora', 'wallDune', 'wallReef',
        'wallCustom', 'wallPick', 'wallReplace', 'wallRemove', 'wallPickFailed', 'wallDim',
      ]) {
        expect(locale[key], `退役 key shell.settings.${key} 不应保留`).toBeUndefined()
      }
    }
  })
})
