/**
 * 业务设置读写编排 行为契约测试
 * （票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 被测：`src/settings/useAppSettings.ts`。
 * 替身边界：只 mock 宿主 KV 桥（`getMobileApi` 的 readAllSettings / writeSetting）——
 * 那是跨进程边界；归一逻辑（settingsModel）是本域代码，不 mock。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-SA1 load 正常 | readAllSettings 成功 | 按键取值并归一填入 | values 与 KV 一致 |
 * | C-SA2 load 缺键 | KV 无某键 | 保留默认值，不写入 undefined | values 保持默认 |
 * | C-SA3 load **异常** | readAllSettings 抛错 | 回落默认值 + 落 error + 记日志 | 不抛出、error 非空 |
 * | C-SA4 set 正常 | writeSetting 成功 | 先归一再落库，成功才更新本地值 | 落库值 = 归一值 |
 * | C-SA5 set **异常** | writeSetting 抛错 | 不更新本地值（UI 弹回）+ 上抛 | values 不变、抛错 |
 * | C-SA6 set 未知键 | 键不在定义表 | 忽略且不落库 | writeSetting 零调用 |
 * | C-SA7 set 越界值 | 传入超区间数值 | 归一夹紧后落库 | 落库值 = 上界 |
 * | C-SA8 reset 正常 | 全部写成功 | 逐项写默认值 | 每项各写 1 次、值归默认 |
 * | C-SA9 reset **异常** | 中途写失败 | 中止并上抛（不回滚已写） | 后续项不再写 |
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

const readAllSettings = vi.fn()
const writeSetting = vi.fn()

vi.mock('@binblink/bedcode-plugin-sdk-mobile', async () => {
  const actual = await vi.importActual<typeof import('@binblink/bedcode-plugin-sdk-mobile')>(
    '@binblink/bedcode-plugin-sdk-mobile',
  )
  return {
    ...actual,
    getMobileApi: () => ({ readAllSettings, writeSetting }),
  }
})

const { useAppSettings } = await import('../useAppSettings')
const { ALL_SETTINGS } = await import('../settingsModel')

/** 最小插件上下文替身 */
function createFakeContext() {
  const logWarn = vi.fn()
  const logError = vi.fn()
  return {
    context: {
      i18n: { t: (k: string) => k },
      logger: { warn: logWarn, error: logError, info: vi.fn() },
      dialogs: { showToast: vi.fn(), showConfirm: vi.fn() },
    } as unknown as PluginContext,
    logWarn,
    logError,
  }
}

beforeEach(() => {
  readAllSettings.mockReset()
  writeSetting.mockReset()
  writeSetting.mockResolvedValue(undefined)
})

describe('C-SA1/C-SA2 load 正常与缺键', () => {
  it('should_fillValuesFromStore_when_readSucceeds', async () => {
    readAllSettings.mockResolvedValue({
      'mobile.autoReconnect': 'false',
      'mobile.maxOpenTerminals': '9',
      'mobile.preferredAuthMethod': 'biometric',
    })
    const { context } = createFakeContext()

    const ctrl = useAppSettings(context)
    await ctrl.load()

    expect(ctrl.values.autoReconnect).toBe(false)
    expect(ctrl.values.maxOpenTerminals).toBe(9)
    expect(ctrl.values.preferredAuthMethod).toBe('biometric')
  })

  it('should_keepDefaultsForAbsentKeys_when_storeHasNoSuchKey', async () => {
    readAllSettings.mockResolvedValue({ 'mobile.vibrate': 'false' })
    const { context } = createFakeContext()

    const ctrl = useAppSettings(context)
    await ctrl.load()

    // 未出现在 KV 的项不得被写成 undefined
    expect(ctrl.values.autoReconnect).toBe(true)
    expect(ctrl.values.vibrate).toBe(false)
    expect(ctrl.error.value).toBe('')
  })

  it('should_clampOutOfRangeValue_when_storeHasIllegalNumber', async () => {
    readAllSettings.mockResolvedValue({ 'mobile.maxOpenTerminals': '999' })
    const { context } = createFakeContext()

    const ctrl = useAppSettings(context)
    await ctrl.load()

    expect(ctrl.values.maxOpenTerminals).toBe(20)
  })
})

describe('C-SA3 load 异常', () => {
  it('should_fallBackToDefaultsAndRecordError_when_readThrows', async () => {
    readAllSettings.mockRejectedValue(new Error('db offline'))
    const { context, logWarn } = createFakeContext()

    const ctrl = useAppSettings(context)
    await expect(ctrl.load()).resolves.toBeUndefined()

    expect(ctrl.values.autoReconnect).toBe(true)
    expect(ctrl.error.value).toBe('db offline')
    expect(logWarn).toHaveBeenCalledTimes(1)
    expect(ctrl.loading.value).toBe(false)
  })
})

describe('C-SA4/C-SA7 set 正常与越界', () => {
  it('should_persistAndUpdateValue_when_setSucceeds', async () => {
    const { context } = createFakeContext()
    const ctrl = useAppSettings(context)

    await ctrl.set('vibrate', false)

    expect(writeSetting).toHaveBeenCalledWith('mobile.vibrate', 'false')
    expect(ctrl.values.vibrate).toBe(false)
  })

  it('should_persistClampedValue_when_setExceedsRange', async () => {
    const { context } = createFakeContext()
    const ctrl = useAppSettings(context)

    await ctrl.set('maxOpenTerminals', 100)

    expect(writeSetting).toHaveBeenCalledWith('mobile.maxOpenTerminals', '20')
    expect(ctrl.values.maxOpenTerminals).toBe(20)
  })
})

describe('C-SA5 set 异常', () => {
  it('should_keepPreviousValueAndRethrow_when_writeThrows', async () => {
    writeSetting.mockRejectedValue(new Error('disk full'))
    const { context, logError } = createFakeContext()
    const ctrl = useAppSettings(context)

    await expect(ctrl.set('vibrate', false)).rejects.toThrow('disk full')

    // 关键：UI 不得停在「看着已保存」的状态
    expect(ctrl.values.vibrate).toBe(true)
    expect(ctrl.error.value).toBe('disk full')
    expect(logError).toHaveBeenCalledTimes(1)
  })
})

describe('C-SA6 set 未知键', () => {
  it('should_ignoreAndNotPersist_when_keyUnknown', async () => {
    const { context, logWarn } = createFakeContext()
    const ctrl = useAppSettings(context)

    await ctrl.set('notARealSetting', true)

    expect(writeSetting).not.toHaveBeenCalled()
    expect(logWarn).toHaveBeenCalledTimes(1)
  })
})

describe('C-SA8/C-SA9 reset', () => {
  it('should_writeEveryDefaultOnce_when_resetSucceeds', async () => {
    readAllSettings.mockResolvedValue({ 'mobile.vibrate': 'false', 'mobile.keepAlive': 'false' })
    const { context } = createFakeContext()
    const ctrl = useAppSettings(context)
    await ctrl.load()

    await ctrl.reset()

    expect(writeSetting).toHaveBeenCalledTimes(ALL_SETTINGS.length)
    expect(writeSetting).toHaveBeenCalledWith('mobile.vibrate', 'true')
    expect(ctrl.values.vibrate).toBe(true)
    expect(ctrl.values.keepAlive).toBe(true)
  })

  it('should_abortAndRethrow_when_aWriteFailsMidway', async () => {
    const { context } = createFakeContext()
    const ctrl = useAppSettings(context)
    // 第 3 项（index 2 = defaultPort）写失败
    writeSetting.mockImplementation(async (key: string) => {
      if (key === 'mobile.defaultPort') throw new Error('write rejected')
    })

    await expect(ctrl.reset()).rejects.toThrow('write rejected')

    // 中止：第 4 项起不再写
    const writtenKeys = writeSetting.mock.calls.map((c) => c[0])
    expect(writtenKeys).toContain('mobile.defaultPort')
    expect(writtenKeys).not.toContain('mobile.vibrate')
    expect(ctrl.saving.value).toBe(false)
  })
})