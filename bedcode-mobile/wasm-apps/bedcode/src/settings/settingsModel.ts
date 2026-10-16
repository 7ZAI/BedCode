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
 * 平台项（主题深浅 / 语言 / UI 字号缩放 / 关于 / 出站授权 / 链路加密）**不在本表**：
 * 它们是平台机制与安全闸门，由宿主设置页持有（见 spec §2 设置切分口径）。
 *
 * **主色色板在本表**（2026-10-16 从宿主外观页下沉）：主色是本应用的产品外观（B5 业务
 * 默认值 / 策略），宿主外壳固定用默认象牙白，不再全局改色。选择器只改本应用运行面
 * 根节点上的 `data-palette`（见 ../app/AppRoot.vue），不外溢到壳。
 * 选择器带色块预览（`swatchPreview: true`）：色值与 `mobile.css` 的
 * `[data-palette=…]` 主色 token 同源，所见即所改；实时生效靠
 * `useAppSettings` 的模块级共享状态（运行面与设置页同读一份值）。
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
  /**
   * 枚举项以「色块预览 + 文字」单选按钮渲染（当前仅主色色板用）
   *
   * 色块颜色取全局 `--mobile-palette-swatch-<value>` token（mobile.css 定义，
   * 随主题深浅自动切换），选中态除色块外还有文字/勾选/aria-pressed 多重承载
   * ——E2E 断言与无障碍都依赖「不只靠颜色传达」这条口径。
   */
  swatchPreview?: boolean
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
    id: 'appearance',
    titleKey: 'settings.group.appearance',
    items: [
      {
        key: 'accentPalette',
        kind: 'enum',
        fallback: 'default',
        labelKey: 'settings.accentPalette',
        hintKey: 'settings.accentPaletteHint',
        swatchPreview: true,
        options: [
          { value: 'default', labelKey: 'settings.palette.default' },
          { value: 'forest', labelKey: 'settings.palette.forest' },
          { value: 'ocean', labelKey: 'settings.palette.ocean' },
          { value: 'sunset', labelKey: 'settings.palette.sunset' },
          { value: 'violet', labelKey: 'settings.palette.violet' },
        ],
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