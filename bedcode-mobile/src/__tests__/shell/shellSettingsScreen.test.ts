/**
 * 壳设置屏 行为契约测试
 * （票 2026-10-09 阶段 B 前置：壳必须覆盖旧宿主设置的全部门类，否则退役旧宿主=功能回归；
 *  2026-10-14 二改：连接 / 认证入口删除、危险区迁「关于」、背景支持自定义壁纸）
 *
 * 被测：`src/shell/components/screens/ShellSettingsScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-ST1 | platformEntries | 平台项 = 出站 / 外观 / 权限 / 关于；连接·认证**不在**宿主 | 文案键在场 + 两项缺席 |
 * | C-ST2 | 入口跳转 | 三项走既有设置页路由，权限走壳内屏，返回走 nav.back | push / openPermissions / back |
 * | C-ST3 | 危险区归属 | 擦除入口已迁「关于 WasmApp」，一级页不得再有 | 文案键缺席 |
 * | C-ST4 | 背景分组 | 四档内置壁纸 + 自定义档 + 遮罩滑杆 | 档位按钮与滑杆在场 |
 * | C-ST5 | 自定义壁纸 | 宿主选图 → 存路径 + 切 custom；移除 → 回落内置档；失败可见 | 偏好与 toast 契约 |
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
const mockToastError = vi.fn()
const mockImportWallpaper = vi.fn(async (): Promise<string | null> => null)
/** 组件把偏好路径交给宿主 asset 工具转 URL；测试只关心「有没有被交出去」 */
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
vi.mock('@/shell/composables/useToast', () => ({
  useToast: () => ({ error: mockToastError, success: vi.fn(), warning: vi.fn(), info: vi.fn() }),
}))
vi.mock('@/plugin/assets', () => ({
  importShellWallpaper: (...args: unknown[]) => mockImportWallpaper(...(args as [])),
  toFileAssetUrl: (path: string) => `asset://${path}`,
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

/** 壁纸档位按钮：aria-label 即文案键（useI18n 替身回显 key） */
function wallButton(wrapper: ReturnType<typeof mount>, key: string) {
  return wrapper.find(`button[aria-label="${key}"]`)
}

beforeEach(async () => {
  mockPush.mockClear()
  mockOpenPermissions.mockClear()
  mockNavBack.mockClear()
  mockOpenDetail.mockClear()
  mockToastError.mockClear()
  mockImportWallpaper.mockClear()
  mockImportWallpaper.mockResolvedValue(null)
  apps.value = []
  // 壁纸偏好是模块级单例（localStorage + 模块内 ref）：清存储不够，还要把内存值复位，
  // 否则上一个用例选过的自定义图会漏到下一个用例（表现为「点了没走选图分支」）
  const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
  const prefs = useShellPreferences()
  prefs.setWallpaperImage(null)
  prefs.setWallpaper('aurora')
  prefs.setWallDim(30)
  localStorage.clear()
})

describe('C-ST1 平台设置门类', () => {
  it('should_renderHostOwnedEntries_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)
    const text = wrapper.text()

    for (const key of [
      'shell.settings.egress',
      'shell.settings.appearance',
      'shell.settings.permissions',
      'shell.settings.about',
    ]) {
      expect(text, `缺少平台设置项 ${key}`).toContain(key)
    }
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
    // 危险区整段迁入「关于 WasmApp」（AboutSettingsView），一级页不再出现
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
    await entryButton(wrapper, 'shell.settings.egress')!.trigger('click')
    await entryButton(wrapper, 'shell.settings.appearance')!.trigger('click')
    await entryButton(wrapper, 'shell.settings.about')!.trigger('click')

    expect(mockPush.mock.calls.map((c) => (c[0] as { name: string }).name)).toEqual([
      'mobile-settings-egress',
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

describe('C-ST4 背景（主屏壁纸）分组', () => {
  it('should_renderWallpaperOptionsAndDimSlider_when_mounted', () => {
    const wrapper = mount(ShellSettingsScreen)

    expect(wrapper.text(), '应有背景分组标题').toContain('shell.settings.background')
    for (const key of [
      'shell.settings.wallNone',
      'shell.settings.wallAurora',
      'shell.settings.wallDune',
      'shell.settings.wallReef',
      'shell.settings.wallCustom',
    ]) {
      expect(wallButton(wrapper, key).exists(), `缺少壁纸档位 ${key}`).toBe(true)
    }
    expect(wrapper.find('input[type="range"]').exists(), '应有遮罩强度滑杆').toBe(true)
    wrapper.unmount()
  })

  it('should_updatePreference_when_wallpaperTapped', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    prefs.setWallpaper('aurora')
    const wrapper = mount(ShellSettingsScreen)

    await wallButton(wrapper, 'shell.settings.wallDune').trigger('click')

    expect(prefs.wallpaper.value, '点选后偏好应切到该档').toBe('dune')
    // 选中态是 aria-pressed（读屏可见），不是只靠描边颜色
    expect(wallButton(wrapper, 'shell.settings.wallDune').attributes('aria-pressed')).toBe('true')
    expect(wallButton(wrapper, 'shell.settings.wallAurora').attributes('aria-pressed')).toBe('false')
    wrapper.unmount()
  })

  it('should_clampDim_when_sliderGoesBeyondRange', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    const wrapper = mount(ShellSettingsScreen)
    const slider = wrapper.find('input[type="range"]')

    // 越界值不抛错：钳制到 [0, 70]（滑杆理论上给不出 200，但偏好模块是公开 API）
    ;(slider.element as HTMLInputElement).value = '200'
    await slider.trigger('input')
    expect(prefs.wallDim.value).toBe(70)

    ;(slider.element as HTMLInputElement).value = '-5'
    await slider.trigger('input')
    expect(prefs.wallDim.value).toBe(0)
    wrapper.unmount()
  })
})

describe('C-ST5 自定义壁纸（宿主选图 + 单槽位偏好）', () => {
  it('should_importThroughHostAndSwitchToCustom_when_customTileTapped', async () => {
    mockImportWallpaper.mockResolvedValueOnce('/data/app/shell-assets/wallpaper/wallpaper-1.png')
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    const wrapper = mount(ShellSettingsScreen)

    await wallButton(wrapper, 'shell.settings.wallCustom').trigger('click')
    await wrapper.vm.$nextTick()

    expect(mockImportWallpaper, '选图必须由宿主完成（前端零资源访问）').toHaveBeenCalledTimes(1)
    expect(prefs.wallpaperImage.value).toBe('/data/app/shell-assets/wallpaper/wallpaper-1.png')
    expect(prefs.wallpaper.value).toBe('custom')
    // 选过后档位显示缩略图（同一路径经宿主 asset 工具转 URL）
    expect(wrapper.find('img').attributes('src')).toBe(
      'asset:///data/app/shell-assets/wallpaper/wallpaper-1.png',
    )
    wrapper.unmount()
  })

  it('should_keepPreferences_when_userCancelsPicker', async () => {
    mockImportWallpaper.mockResolvedValueOnce(null)
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    prefs.setWallpaper('reef')
    const wrapper = mount(ShellSettingsScreen)

    await wallButton(wrapper, 'shell.settings.wallCustom').trigger('click')
    await wrapper.vm.$nextTick()

    expect(prefs.wallpaperImage.value, '取消不当成失败').toBeNull()
    expect(prefs.wallpaper.value, '取消不改动现有档位').toBe('reef')
    expect(mockToastError, '取消不弹错误').not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('should_reportError_when_hostImportFails', async () => {
    mockImportWallpaper.mockRejectedValueOnce(new Error('picker unavailable'))
    const wrapper = mount(ShellSettingsScreen)

    await wallButton(wrapper, 'shell.settings.wallCustom').trigger('click')
    await wrapper.vm.$nextTick()

    expect(mockToastError).toHaveBeenCalledWith('shell.settings.wallPickFailed')
    wrapper.unmount()
  })

  it('should_fallBackToBuiltin_when_customImageRemoved', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/data/x.png')
    const wrapper = mount(ShellSettingsScreen)

    const remove = entryButton(wrapper, 'shell.settings.wallRemove')!
    expect(remove, '选过图后应出现移除入口').toBeTruthy()
    await remove.trigger('click')

    expect(prefs.wallpaperImage.value).toBeNull()
    expect(prefs.wallpaper.value, '停在 custom 档但不能没有图').toBe('aurora')
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
    'background',
    'backgroundNote',
    'wallNone',
    'wallAurora',
    'wallDune',
    'wallReef',
    'wallCustom',
    'wallPick',
    'wallReplace',
    'wallRemove',
    'wallPickFailed',
    'wallDim',
    'egress',
    'egressHint',
    'about',
    'appSettings',
    'runtime',
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
    // 连接 / 认证入口删除后，文案键一并删除——留着会诱导下一个人「反正有 key」把入口接回来
    for (const locale of [((zhShell as any).shell.settings), ((enShell as any).shell.settings)]) {
      for (const key of ['connection', 'connectionHint', 'authentication', 'authenticationHint', 'dangerZone', 'clearAllData', 'clearAllDataHint', 'clearAllDataFailed']) {
        expect(locale[key], `退役 key shell.settings.${key} 不应保留`).toBeUndefined()
      }
    }
  })
})
