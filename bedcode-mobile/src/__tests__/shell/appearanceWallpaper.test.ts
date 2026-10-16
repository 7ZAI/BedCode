/**
 * 外观设置页「主屏背景」区块 契约测试
 * （2026-10-15 壁纸区块由壳设置页迁入本页，行为契约随迁——
 *  契约本体来自原 shellSettingsScreen.test.ts 的 C-ST4 / C-ST5）
 *
 * 被测：`src/views/settings/AppearanceSettingsView.vue`（壁纸区块）
 *
 * | 契约 | 来源 | 预期 |
 * |---|---|---|
 * | 档位渲染 | wallpaperOptions + 滑杆 | 四档 + 自定义 + range 在场 |
 * | 点选切档 | setWallpaper | 偏好切换 + aria-pressed 跟随 |
 * | 滑杆钳制 | onDimInput → setWallDim | 越界值收敛到 [0, 70] |
 * | 选图落盘 | importShellWallpaper → setWallpaperImage | 存路径 + 切 custom + 缩略图 |
 * | 取消选图 | 返回 null | 现有偏好不被改动 |
 * | 选图失败 | 选图器异常 | 不抛未处理 rejection、偏好不变 |
 * | 移除自定义 | removeCustomWallpaper | 回落到默认内置档（不留「自定义无图」空档） |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'

const mockImportWallpaper = vi.fn(async (): Promise<string | null> => null)

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key, locale: ref('zh-CN') }),
}))
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn(), back: vi.fn() }),
}))
vi.mock('@/composables/useMobileSettings', () => ({
  useMobileSettings: () => ({
    settings: ref({ fontSize: 'normal' }),
    themeMode: ref('light'),
    // 2026-10-16：主色色板移出宿主外观页，paletteMode 一并退役（真源在 wasm-app 设置域）
    currentLanguage: ref('zh-CN'),
    loadSettings: vi.fn(async () => undefined),
  }),
}))
vi.mock('@/plugin/assets', () => ({
  importShellWallpaper: (...args: unknown[]) => mockImportWallpaper(...(args as [])),
  toFileAssetUrl: (path: string) => `asset://${path}`,
}))

import AppearanceSettingsView from '@/views/settings/AppearanceSettingsView.vue'

/** 壁纸档位按钮：i18n 替身回显 key，文本即定位（wall* key 全页唯一） */
function wallButton(wrapper: ReturnType<typeof mount>, key: string) {
  return wrapper.findAll('button').find((b) => b.text().includes(key))
}

beforeEach(async () => {
  mockImportWallpaper.mockClear()
  mockImportWallpaper.mockResolvedValue(null)
  // 壁纸偏好是模块级单例（localStorage + 模块内 ref）：清存储不够，还要把内存值复位，
  // 否则上一个用例选过的自定义图会漏到下一个用例
  const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
  const prefs = useShellPreferences()
  prefs.setWallpaperImage(null)
  prefs.setWallpaper('aurora')
  prefs.setWallDim(30)
  localStorage.clear()
})

describe('壁纸档位分组（随壳设置页迁入）', () => {
  it('should_renderWallpaperOptionsAndDimSlider_when_mounted', () => {
    const wrapper = mount(AppearanceSettingsView)

    expect(wrapper.text(), '应有主屏背景分组标题').toContain('settings.appearance.backgroundSection')
    for (const key of [
      'settings.appearance.wallNone',
      'settings.appearance.wallAurora',
      'settings.appearance.wallDune',
      'settings.appearance.wallReef',
      'settings.appearance.wallCustom',
    ]) {
      expect(wallButton(wrapper, key)?.exists(), `缺少壁纸档位 ${key}`).toBe(true)
    }
    expect(wrapper.find('input[type="range"]').exists(), '应有遮罩强度滑杆').toBe(true)
    wrapper.unmount()
  })

  it('should_updatePreference_when_wallpaperTapped', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    prefs.setWallpaper('aurora')
    const wrapper = mount(AppearanceSettingsView)

    await wallButton(wrapper, 'settings.appearance.wallDune')!.trigger('click')

    expect(prefs.wallpaper.value, '点选后偏好应切到该档').toBe('dune')
    // 选中态是 aria-pressed（读屏可见），不是只靠描边颜色
    expect(
      wallButton(wrapper, 'settings.appearance.wallDune')!.attributes('aria-pressed'),
    ).toBe('true')
    expect(
      wallButton(wrapper, 'settings.appearance.wallAurora')!.attributes('aria-pressed'),
    ).toBe('false')
    wrapper.unmount()
  })

  it('should_clampDim_when_sliderGoesBeyondRange', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    const wrapper = mount(AppearanceSettingsView)
    const slider = wrapper.find('input[type="range"]')

    // 越界值不抛错：钳制到 [0, 70]
    ;(slider.element as HTMLInputElement).value = '200'
    await slider.trigger('input')
    expect(prefs.wallDim.value).toBe(70)

    ;(slider.element as HTMLInputElement).value = '-5'
    await slider.trigger('input')
    expect(prefs.wallDim.value).toBe(0)
    wrapper.unmount()
  })
})

describe('自定义壁纸（宿主选图 + 单槽位偏好，随迁）', () => {
  it('should_importThroughHostAndSwitchToCustom_when_customTileTapped', async () => {
    mockImportWallpaper.mockResolvedValueOnce('/data/app/shell-assets/wallpaper/wallpaper-1.png')
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    const wrapper = mount(AppearanceSettingsView)

    await wallButton(wrapper, 'settings.appearance.wallCustom')!.trigger('click')
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
    const wrapper = mount(AppearanceSettingsView)

    await wallButton(wrapper, 'settings.appearance.wallCustom')!.trigger('click')
    await wrapper.vm.$nextTick()

    expect(prefs.wallpaperImage.value, '取消不当成失败').toBeNull()
    expect(prefs.wallpaper.value, '取消不改动现有档位').toBe('reef')
    wrapper.unmount()
  })

  it('should_notThrow_when_hostImportFails', async () => {
    mockImportWallpaper.mockRejectedValueOnce(new Error('picker unavailable'))
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    const wrapper = mount(AppearanceSettingsView)

    // 选图器异常：不产生未处理 rejection，现有偏好不被改动
    await expect(
      wallButton(wrapper, 'settings.appearance.wallCustom')!.trigger('click'),
    ).resolves.toBeUndefined()
    await wrapper.vm.$nextTick()

    expect(prefs.wallpaperImage.value).toBeNull()
    expect(prefs.wallpaper.value).toBe('aurora')
    wrapper.unmount()
  })

  it('should_fallBackToBuiltin_when_customImageRemoved', async () => {
    const { useShellPreferences } = await import('@/shell/composables/useShellPreferences')
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/data/x.png')
    const wrapper = mount(AppearanceSettingsView)

    const remove = wallButton(wrapper, 'settings.appearance.wallRemove')!
    expect(remove, '选过图后应出现移除入口').toBeTruthy()
    await remove.trigger('click')

    expect(prefs.wallpaperImage.value).toBeNull()
    expect(prefs.wallpaper.value, '回落默认内置档，不留「自定义无图」空档').toBe('aurora')
    wrapper.unmount()
  })
})