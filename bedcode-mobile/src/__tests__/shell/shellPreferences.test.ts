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
    const { useShellPreferences, WALLPAPER_DEFAULT } = await loadPrefsWith(
      JSON.stringify({ wallpaper: 'dune', wallDim: -3 }),
    )
    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value, '合法壁纸档必须保留').toBe('dune')
    expect(prefs.wallpaper.value).not.toBe(WALLPAPER_DEFAULT)
    expect(prefs.wallDim.value).toBe(30)
  })

  it('should_fallBackToDefaults_when_storageCorrupted', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { useShellPreferences } = await loadPrefsWith('{not json')

    const prefs = useShellPreferences()

    expect(prefs.wallpaper.value).toBe('aurora')
    expect(prefs.wallDim.value).toBe(30)
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
