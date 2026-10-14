/**
 * 业务设置定义表 + 取值归一 行为契约测试
 * （票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 被测：`src/settings/settingsModel.ts`。
 * 纯数据 + 纯函数，不触宿主。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-SM1 | SETTING_GROUPS 结构 | 每项有键 / 类型 / 默认值 / 文案键；键不重复 | 无重复键、无缺字段 |
 * | C-SM2 | 键名与宿主字段同名 | 键名与宿主 MobileSettings 字段一致（写穿判据） | 见用例 |
 * | C-SM3 | 平台项不得混入 | 主题 / 语言 / 字号 / 出站 / 链路加密不在表内 | 见用例 |
 * | C-SM4 | normalize boolean 分支 | 真值布尔 / 'true' / 'false' / 其他 | 见用例 |
 * | C-SM5 | normalize number 分支（**边界**） | 数值串 / NaN / 超上下限 | 夹紧到区间、NaN 落默认 |
 * | C-SM6 | normalize enum 分支（**反例**） | 白名单外的值 | 落默认 |
 * | C-SM7 | toSettingString | 布尔转 'true'/'false' | 与宿主 loadSettings 反解口径对齐 |
 */
import { describe, it, expect } from 'vitest'
import {
  ALL_SETTINGS,
  SETTING_GROUPS,
  SETTINGS_BY_KEY,
  normalizeSetting,
  toSettingString,
} from '../settingsModel'

/** 与宿主 useMobileSettings::MobileSettings 对齐的字段集合（写穿判据，见 settingsModel 头注） */
const HOST_KNOWN_FIELDS = [
  'autoReconnect',
  'keepAlive',
  'defaultPort',
  'notifyOnWaiting',
  'notifyOnConnection',
  'notifyInBackground',
  'vibrate',
  'soundOnTaskComplete',
  'fontSize',
  'maxOpenTerminals',
  'preferredAuthMethod',
]

describe('C-SM1 定义表结构', () => {
  it('should_haveNoDuplicateKeys_when_tableBuilt', () => {
    const keys = ALL_SETTINGS.map((s) => s.key)

    expect(new Set(keys).size).toBe(keys.length)
  })

  it('should_haveLabelAndFallbackForEveryItem_when_tableBuilt', () => {
    for (const item of ALL_SETTINGS) {
      expect(item.key, `键缺失: ${JSON.stringify(item)}`).toBeTruthy()
      expect(item.labelKey, `${item.key} 缺文案键`).toMatch(/^settings\./)
      expect(item.fallback, `${item.key} 缺默认值`).not.toBeUndefined()
    }
  })

  it('should_haveOptionsForEveryEnum_when_tableBuilt', () => {
    for (const item of ALL_SETTINGS.filter((s) => s.kind === 'enum')) {
      expect(item.options?.length, `${item.key} 是 enum 但没有 options`).toBeGreaterThan(0)
      // 枚举默认值必须在自己的选项里，否则归一后仍是非法值
      expect(item.options!.map((o) => o.value)).toContain(item.fallback)
    }
  })

  it('should_indexEveryItem_when_buildingLookup', () => {
    for (const item of ALL_SETTINGS) {
      expect(SETTINGS_BY_KEY[item.key]).toBe(item)
    }
  })
})

describe('C-SM2 键名与宿主字段同名（写穿判据）', () => {
  it('should_useHostFieldNames_when_keysComparedWithHost', () => {
    // 改名会让宿主消费者（自动重连 / 通知 / 终端上限）静默读到默认值
    for (const item of ALL_SETTINGS) {
      expect(
        HOST_KNOWN_FIELDS,
        `${item.key} 不是宿主 MobileSettings 的字段名，桥的写穿会失效`,
      ).toContain(item.key)
    }
  })
})

describe('C-SM3 平台项不得混入业务表', () => {
  it('should_excludePlatformItems_when_tableBuilt', () => {
    // 平台项由宿主设置页持有（spec §2 切分口径）；混进来会造成两处 UI 都写同一项
    const keys = ALL_SETTINGS.map((s) => s.key)

    expect(keys).not.toContain('fontSize')
  })
})

describe('C-SM4 布尔归一', () => {
  const def = SETTINGS_BY_KEY.vibrate

  it.each([
    [true, true],
    [false, false],
    ['true', true],
    ['false', false],
  ])('should_normalizeToBoolean_when_inputIs_%s', (input, expected) => {
    expect(normalizeSetting(def, input)).toBe(expected)
  })

  it('should_fallBackToDefault_when_inputIsGarbage', () => {
    expect(normalizeSetting(def, 'yes-please')).toBe(def.fallback)
  })
})

describe('C-SM5 数值归一（含边界）', () => {
  const def = SETTINGS_BY_KEY.maxOpenTerminals

  it('should_parseNumericString_when_inputIsString', () => {
    expect(normalizeSetting(def, '7')).toBe(7)
  })

  it('should_roundToInteger_when_inputHasFraction', () => {
    expect(normalizeSetting(def, 3.6)).toBe(4)
  })

  it('should_clampToMax_when_inputExceedsRange', () => {
    // 上界来自 host migrateLegacySettings 的 1-20，两侧必须一致否则上限被撑爆
    expect(normalizeSetting(def, 999)).toBe(def.max)
  })

  it('should_clampToMin_when_inputBelowRange', () => {
    expect(normalizeSetting(def, -5)).toBe(def.min)
  })

  it('should_fallBackToDefault_when_inputIsNaN', () => {
    expect(normalizeSetting(def, 'not-a-number')).toBe(def.fallback)
  })

  it('should_acceptBoundaryValues_when_inputAtRangeEdges', () => {
    expect(normalizeSetting(def, def.min!)).toBe(def.min)
    expect(normalizeSetting(def, def.max!)).toBe(def.max)
  })
})

describe('C-SM6 枚举归一（反例）', () => {
  const def = SETTINGS_BY_KEY.preferredAuthMethod

  it('should_keepValue_when_inputInWhitelist', () => {
    expect(normalizeSetting(def, 'biometric')).toBe('biometric')
  })

  it('should_fallBackToDefault_when_inputNotInWhitelist', () => {
    expect(normalizeSetting(def, 'sudo')).toBe(def.fallback)
  })

  it('should_fallBackToDefault_when_inputIsBoolean', () => {
    expect(normalizeSetting(def, true)).toBe(def.fallback)
  })
})

describe('C-SM7 取值转 KV 字符串', () => {
  it('should_serializeBooleansAsStringLiterals_when_converted', () => {
    expect(toSettingString(true)).toBe('true')
    expect(toSettingString(false)).toBe('false')
  })

  it('should_serializeNumbersAndStrings_when_converted', () => {
    expect(toSettingString(8765)).toBe('8765')
    expect(toSettingString('biometric')).toBe('biometric')
  })

  it('should_roundTripThroughNormalize_when_booleanConverted', () => {
    // 与宿主 loadSettings 的反解口径对齐：写出去的串必须能被自己的归一读回来
    const def = SETTINGS_BY_KEY.autoReconnect

    expect(normalizeSetting(def, toSettingString(true))).toBe(true)
    expect(normalizeSetting(def, toSettingString(false))).toBe(false)
  })
})

describe('定义表分组顺序', () => {
  it('should_keepLegacyGroupOrder_when_tableBuilt', () => {
    expect(SETTING_GROUPS.map((g) => g.id)).toEqual([
      'connection',
      'notifications',
      'terminal',
      'auth',
    ])
  })
})