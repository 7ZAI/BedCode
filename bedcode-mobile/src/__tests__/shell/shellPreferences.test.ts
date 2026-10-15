/**
 * 壳 UI 偏好（壁纸 / 遮罩）行为契约测试（2026-10-14 Android 平台化）
 *
 * 被测：`src/shell/composables/useShellPreferences.ts`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-P1 | load 缺省分支 | 无存储时用默认档（aurora / 30） | 默认值 |
 * | C-P2 | load 正常分支 | 合法存储被读取 | 读回存储值 |
 * | C-P3 | load 逐项校验 | 非法 wallpaper / 越界 dim 各自回落默认 | 单项回落，不整表重置 |
 * | C-P4 | load 异常分支 | 损坏 JSON | 回落默认且不抛（warn 可见） |
 * | C-P5 | setWallDim | 越界与小数 | 钳制到 [0,70] 并取整 |
 * | C-P6 | watch 写穿 | 变更即持久化 | localStorage 里有新值 |
 * | C-P7 | watch 写入异常 | 存储不可用（隐私模式 / 配额） | 不抛，仅 warn |
 * | C-P8 | setWallpaperImage(路径) | 记路径并切到 custom 档 | 档位=custom + 路径持久化 |
 * | C-P9 | setWallpaperImage(null) | 移除图片；正停在 custom 档时回落默认 | 档位回落 + 路径清空 |
 * | C-P10 | load 一致性 | custom 档但没有图 = 没选过 | 回落默认档，不渲染空背景 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

const STORAGE_KEY = 'bedcode.shell-ui-prefs.v1'

/** 以给定存储内容重新加载模块（模块级单例只在 import 时读一次） */
async function loadPrefsWith(raw?: string) {
  vi.resetModules()
  localStorage.clear()
  if (raw !== undefined) localStorage.setItem(STORAGE_KEY, raw)
  return await import('@/shell/composables/useShellPreferences')
}

beforeEach(() => {
  localStorage.clear()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('C-P1..C-P4 偏好加载', () => {
  it('should_useDefaults_when_noStoredPrefs', async () => {
    const { useShellPreferences, WALLPAPER_DEFAULT, WALL_DIM_DEFAULT } = await loadPrefsWith()
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe(WALLPAPER_DEFAULT)
    expect(prefs.wallDim.value).toBe(WALL_DIM_DEFAULT)
    expect(prefs.wallpaperImage.value, '未选过自定义图时为 null').toBeNull()
  })

  it('should_readStoredPrefs_when_valid', async () => {
    const { useShellPreferences } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'reef', wallDim: 55 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe('reef')
    expect(prefs.wallDim.value).toBe(55)
  })

  it('should_fallBackPerField_when_valuesInvalid', async () => {
    // 手改 / 旧版本数据：未知壁纸档与越界遮罩各自回落，不因为一个坏值把整表重置
    const { useShellPreferences, WALLPAPER_DEFAULT, WALL_DIM_DEFAULT } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'not-a-wallpaper', wallDim: 999 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe(WALLPAPER_DEFAULT)
    expect(prefs.wallDim.value).toBe(WALL_DIM_DEFAULT)
  })

  it('should_keepValidField_when_otherFieldBroken', async () => {
    const { useShellPreferences, WALLPAPER_DEFAULT, WALL_DIM_DEFAULT } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'dune', wallDim: -3 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value, '合法壁纸档必须保留').toBe('dune')
    expect(prefs.wallpaper.value).not.toBe(WALLPAPER_DEFAULT)
    expect(prefs.wallDim.value, '越界值回落到默认值').toBe(WALL_DIM_DEFAULT)
  })

  it('should_fallBackToDefaults_when_storageCorrupted', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { useShellPreferences, WALL_DIM_DEFAULT } = await loadPrefsWith('{not json')

    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe('aurora')
    expect(prefs.wallDim.value).toBe(WALL_DIM_DEFAULT)
    expect(warn, '损坏数据要可观测，不能静默').toHaveBeenCalled()
  })
})

describe('C-P5..C-P7 偏好写入', () => {
  it('should_clampAndRound_when_settingOutOfRangeDim', async () => {
    const { useShellPreferences, WALL_DIM_MAX } = await loadPrefsWith()
    const prefs = useShellPreferences()

    prefs.setWallDim(200)
    expect(prefs.wallDim.value).toBe(WALL_DIM_MAX)

    prefs.setWallDim(-10)
    expect(prefs.wallDim.value).toBe(0)

    prefs.setWallDim(33.6)
    expect(prefs.wallDim.value).toBe(34)
  })

  it('should_persistToStorage_when_preferenceChanges', async () => {
    const { useShellPreferences } = await loadPrefsWith()
    const prefs = useShellPreferences()

    prefs.setWallpaper('dune')
    prefs.setWallDim(42)
    await Promise.resolve() // watch 是异步 flush：等一拍再读存储

    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}')).toEqual({
      wallpaper: 'dune',
      wallpaperImage: null,
      wallDim: 42,
    })
  })

  it('should_notThrow_when_storageWriteFails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError')
    })
    const { useShellPreferences } = await loadPrefsWith()
    const prefs = useShellPreferences()

    // 存储不可用（隐私模式）时偏好只在本次会话生效，不能把主流程带崩
    expect(() => prefs.setWallpaper('reef')).not.toThrow()
    expect(prefs.wallpaper.value).toBe('reef')
    await Promise.resolve()
    expect(warn).toHaveBeenCalled()
  })
})

describe('C-P8..C-P10 自定义壁纸', () => {
  it('should_switchToCustomAndPersistPath_when_imageSet', async () => {
    const { useShellPreferences, WALLPAPER_CUSTOM } = await loadPrefsWith()
    const prefs = useShellPreferences()

    prefs.setWallpaperImage('/data/app/shell-assets/wallpaper/wallpaper-1.png')
    await Promise.resolve()

    expect(prefs.wallpaperImage.value).toBe('/data/app/shell-assets/wallpaper/wallpaper-1.png')
    expect(prefs.wallpaper.value, '选图即切换到自定义档').toBe(WALLPAPER_CUSTOM)
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}').wallpaperImage).toBe(
      '/data/app/shell-assets/wallpaper/wallpaper-1.png',
    )
  })

  it('should_fallBackToBuiltin_when_customImageRemoved', async () => {
    const { useShellPreferences, WALLPAPER_CUSTOM, WALLPAPER_DEFAULT } = await loadPrefsWith()
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/tmp/a.png')
    expect(prefs.wallpaper.value).toBe(WALLPAPER_CUSTOM)

    prefs.setWallpaperImage(null)

    expect(prefs.wallpaperImage.value).toBeNull()
    expect(prefs.wallpaper.value, '停在 custom 档但不能没有图').toBe(WALLPAPER_DEFAULT)
  })

  it('should_keepBuiltinChoice_when_removingImageNotInCustomMode', async () => {
    const { useShellPreferences } = await loadPrefsWith()
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/tmp/a.png')
    prefs.setWallpaper('reef')

    prefs.setWallpaperImage(null)

    expect(prefs.wallpaper.value, '移除图不影响已选的内置档').toBe('reef')
  })

  it('should_loadCustomWallpaper_when_storeHasPath', async () => {
    const { useShellPreferences, WALLPAPER_CUSTOM } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'custom', wallpaperImage: '/data/x.png', wallDim: 30 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe(WALLPAPER_CUSTOM)
    expect(prefs.wallpaperImage.value).toBe('/data/x.png')
  })

  it('should_fallBackToDefault_when_customModeHasNoImage', async () => {
    // 旧数据 / 手改数据：custom 档但没有图 = 没选过，必须回落而不是渲染空背景
    const { useShellPreferences, WALLPAPER_DEFAULT } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'custom', wallDim: 30 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe(WALLPAPER_DEFAULT)
    expect(prefs.wallpaperImage.value).toBeNull()
  })
})
