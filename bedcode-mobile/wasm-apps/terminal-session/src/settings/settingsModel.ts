/**
 * 业务设置项定义（票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 归属（§5.1）：「有哪些设置项 / 默认值 / 取值范围」是产品事实，自持在插件侧。
 * 宿主只提供通用 KV 持久化通道（`mobileApi.readAllSettings` / `writeSetting`），
 * 不感知本表的任何一项。
 *
 * 键名与宿主 `MobileSettings` 字段同名是**刻意的**：桥的写穿规则按字段名判定
 * 「是否为宿主已知键」，同名才能让宿主消费者（自动重连 / 通知 / 终端上限）读到真值，
 * 从而真源下沉的同时宿主行为不回退。改名会让这些消费者静默读到默认值。
 *
 * 平台项（主题 / 语言 / UI 字号缩放 / 关于 / 出站授权 / 链路加密）**不在本表**：
 * 它们是平台机制与安全闸门，由宿主设置页持有（见 spec §2 设置切分口径）。
 */

export type SettingKind = 'boolean' | 'number' | 'enum'

export interface SettingDef {
  /** KV 键（宿主已知字段，全量迁移后再考虑加插件前缀） */
  key: string
  kind: SettingKind
  /** 默认值（与宿主 MobileSettings 的默认值保持一致，避免重置后两侧漂移） */
  fallback: boolean | number | string
  /** 文案键（域内相对键，settings.*） */
  labelKey: string
  /** 说明文案键（可选） */
  hintKey?: string
  /** enum 型的可选项 */
  options?: { value: string; labelKey: string }[]
  /** number 型的取值范围 */
  min?: number
  max?: number
}

export interface SettingGroup {
  id: string
  titleKey: string
  items: SettingDef[]
}

export const SETTING_GROUPS: SettingGroup[] = [
  {
    id: 'connection',
    titleKey: 'settings.group.connection',
    items: [
      { key: 'autoReconnect', kind: 'boolean', fallback: true, labelKey: 'settings.autoReconnect' },
      { key: 'keepAlive', kind: 'boolean', fallback: true, labelKey: 'settings.keepAlive' },
      {
        key: 'defaultPort',
        kind: 'number',
        fallback: 8765,
        labelKey: 'settings.defaultPort',
        min: 1,
        max: 65535,
      },
    ],
  },
  {
    id: 'notifications',
    titleKey: 'settings.group.notifications',
    items: [
      {
        key: 'notifyOnWaiting',
        kind: 'boolean',
        fallback: true,
        labelKey: 'settings.notifyOnWaiting',
      },
      {
        key: 'notifyOnConnection',
        kind: 'boolean',
        fallback: true,
        labelKey: 'settings.notifyOnConnection',
      },
      {
        key: 'notifyInBackground',
        kind: 'boolean',
        fallback: true,
        labelKey: 'settings.notifyInBackground',
      },
      { key: 'vibrate', kind: 'boolean', fallback: true, labelKey: 'settings.vibrate' },
      {
        key: 'soundOnTaskComplete',
        kind: 'boolean',
        fallback: true,
        labelKey: 'settings.soundOnTaskComplete',
      },
    ],
  },
  {
    id: 'terminal',
    titleKey: 'settings.group.terminal',
    items: [
      {
        key: 'maxOpenTerminals',
        kind: 'number',
        fallback: 5,
        labelKey: 'settings.maxOpenTerminals',
        min: 1,
        max: 20,
      },
    ],
  },
  {
    id: 'auth',
    titleKey: 'settings.group.auth',
    items: [
      {
        key: 'preferredAuthMethod',
        kind: 'enum',
        fallback: 'pairing_code',
        labelKey: 'settings.preferredAuthMethod',
        options: [
          { value: 'pairing_code', labelKey: 'settings.authMethod.pairingCode' },
          { value: 'biometric', labelKey: 'settings.authMethod.biometric' },
        ],
      },
    ],
  },
]

/** 全部业务设置项（扁平视图；写入 / 重置按此遍历） */
export const ALL_SETTINGS: SettingDef[] = SETTING_GROUPS.flatMap((g) => g.items)

/** 键 → 定义（读回填时按键查形状与默认值） */
export const SETTINGS_BY_KEY: Record<string, SettingDef> = Object.fromEntries(
  ALL_SETTINGS.map((item) => [item.key, item]),
)

/**
 * 按定义校验并归一化取值
 *
 * 落库前必过这道闸：KV 里的值来自历史版本 / 手改 / 越界输入，直接采信会让
 * `maxOpenTerminals` 这类有区间约束的项拿到非法值（终端上限被撑爆）。
 * 非法一律回退默认值——宁可回到已知良好态，不静默接受坏值。
 */
export function normalizeSetting(def: SettingDef, raw: unknown): boolean | number | string {
  if (def.kind === 'boolean') {
    if (typeof raw === 'boolean') return raw
    if (raw === 'true') return true
    if (raw === 'false') return false
    return def.fallback as boolean
  }
  if (def.kind === 'number') {
    const n = typeof raw === 'number' ? raw : Number(raw)
    if (Number.isNaN(n)) return def.fallback as number
    const min = def.min ?? Number.NEGATIVE_INFINITY
    const max = def.max ?? Number.POSITIVE_INFINITY
    return Math.min(max, Math.max(min, Math.round(n)))
  }
  const allowed = def.options?.map((o) => o.value) ?? []
  return typeof raw === 'string' && allowed.includes(raw) ? raw : (def.fallback as string)
}

/** 取值 → KV 字符串（布尔转 'true'/'false'，与宿主 loadSettings 的反解口径对齐） */
export function toSettingString(value: boolean | number | string): string {
  return typeof value === 'boolean' ? String(value) : String(value)
}