/**
 * Settings Sections — 设置页分组统一模型（宿主内置分组 + 插件贡献分组）
 *
 * 设置页从「7 个写死的分组组件」改为按 order 合并渲染：内置分组与插件经
 * `ui.registerSettingsSection` 贡献的分组共用同一排序空间，插件可用 order
 * 插到任意内置分组之间（语义与侧边栏菜单一致，见 useSidebarMenu）。
 *
 * 共享状态（语言选项 / 动画开关 / 切换回调）由设置页父级持有并下传给贡献分组，
 * 贡献分组不得自行推导——沿用 2026-09-17 设置页拆分的教训：语言切换淡出容器
 * 与动画开关语义只有一处真源。
 */
import { computed, type ComputedRef } from 'vue'
import { getPluginRegistry, isContributionActiveState } from '@/plugin/registry'
import PluginSettingsSection from '@/plugin/components/PluginSettingsSection.vue'
import SettingsAppearanceSection from '@/components/settings/SettingsAppearanceSection.vue'
import SettingsLinkCryptoSection from '@/components/settings/SettingsLinkCryptoSection.vue'
import SettingsSystemSection from '@/components/settings/SettingsSystemSection.vue'
import SettingsLoggingSection from '@/components/settings/SettingsLoggingSection.vue'
import SettingsAboutSection from '@/components/settings/SettingsAboutSection.vue'

/** 内置分组排序槽位 — 区间间隔 100，供贡献分组插入。
 * 「关于」恒在最末（9999），贡献分组缺省 600 时落在「日志」之后、「关于」之前。
 * 退役槽位（内置分组已下沉，槽位值保留不复用，防止第三方分组撞位）：
 * - `pairing`（200）→ `com.bedcode.terminal-session` 贡献的「配对设置」分组（票 14）
 * - `session`（400）→ 同一插件贡献的「会话」分组（默认执行环境 / 默认启动命令）——
 *   宿主原内置分组改的是无人消费的 `settings.session.default_*`（失效 UI），
 *   下沉后写插件存储 `session.formDefaults`，即新建会话表单的真实默认值源 */
export const BUILTIN_SECTION_ORDERS = {
  appearance: 100,
  pairing: 200,
  linkCrypto: 300,
  session: 400,
  system: 500,
  logging: 600,
  about: 9999,
} as const

/** 设置页下传的共享状态（语言 / 动画），贡献分组按需消费，不得自行推导 */
export interface SettingsSharedState {
  /** 语言选项列表 */
  languageOptions: { value: string; label: string }[]
  /** 当前语言 */
  currentLanguage: string
  /** 全局动画总开关 */
  animationsEnabled: boolean
  /** 切换语言（含淡出过渡编排） */
  onSwitchLanguage: (value: string) => void
  /** 切换动画开关 */
  onToggleAnimations: () => void
}

/** 统一设置分组条目（渲染模型的单一形状，内置与贡献同构） */
export interface SettingsSectionEntry {
  /** 唯一 key（渲染 :key） */
  key: string
  /** 排序值，升序排列，同值保持 内置 → 贡献 的先后顺序 */
  order: number
  /** 渲染组件 */
  component: any
  /** 传给组件的 props */
  props: Record<string, unknown>
  /** 贡献方插件 id（内置分组为 undefined） */
  pluginId?: string
}

/**
 * 设置分组组合子 — 合并内置分组与插件贡献分组，按 order 升序稳定排列
 *
 * @param getShared 共享状态取值器：在 computed 内调用，从而把语言/动画的
 *   响应式依赖挂到本 computed 上（传值而非取值器会丢失依赖）
 */
export function useSettingsSections(
  getShared: () => SettingsSharedState,
): { sections: ComputedRef<SettingsSectionEntry[]> } {
  const registry = getPluginRegistry()

  const sections = computed<SettingsSectionEntry[]>(() => {
    const shared = getShared()

    const builtin: SettingsSectionEntry[] = [
      {
        key: 'appearance',
        order: BUILTIN_SECTION_ORDERS.appearance,
        component: SettingsAppearanceSection,
        props: {
          languageOptions: shared.languageOptions,
          currentLanguage: shared.currentLanguage,
          animationsEnabled: shared.animationsEnabled,
          onSwitchLanguage: shared.onSwitchLanguage,
          onToggleAnimations: shared.onToggleAnimations,
        },
      },
      // 票 14：原内置「配对设置」分组（key: 'pairing'，order 200）已退役——
      // 配对码 / QR 有效期改由 com.bedcode.terminal-session 贡献的分组（同 order 200）接管
      {
        key: 'linkCrypto',
        order: BUILTIN_SECTION_ORDERS.linkCrypto,
        component: SettingsLinkCryptoSection,
        props: {},
      },
      // 原内置「会话」分组（key: 'session'，order 400）已退役——会话默认值
      // 改由 com.bedcode.terminal-session 贡献的分组（同 order 400）接管，写插件存储
      {
        key: 'system',
        order: BUILTIN_SECTION_ORDERS.system,
        component: SettingsSystemSection,
        props: {},
      },
      {
        key: 'logging',
        order: BUILTIN_SECTION_ORDERS.logging,
        component: SettingsLoggingSection,
        props: {},
      },
      {
        key: 'about',
        order: BUILTIN_SECTION_ORDERS.about,
        component: SettingsAboutSection,
        props: {},
      },
    ]

    // 依赖运行态响应式副本：插件进入 error / 停用后贡献分组随之摘除，
    // 恢复激活后随之回来，与侧边栏让位共用 isContributionActiveState 判据
    const states = registry.pluginStatesRef.value
    const contributed: SettingsSectionEntry[] = registry.settingsSectionsRef.value
      .filter((s) => isContributionActiveState(states[s.pluginId]))
      .map((s) => ({
        key: `plugin-${s.pluginId}-${s.id}`,
        order: s.order,
        pluginId: s.pluginId,
        component: PluginSettingsSection,
        props: {
          pluginId: s.pluginId,
          titleKey: s.titleKey,
          icon: s.icon,
          component: s.component,
          shared,
        },
      }))

    const all = [...builtin, ...contributed]
    // sort 为稳定排序：同 order 时保持 内置 → 插件注册 的先后顺序
    all.sort((a, b) => a.order - b.order)
    return all
  })

  return { sections }
}
