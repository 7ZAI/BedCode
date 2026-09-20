/**
 * Sidebar Menu — 侧边栏菜单统一模型 + 可配置排序的扩展点
 *
 * 内置菜单项、宿主自定义项（registerSidebarItem）与插件注册的面板
 * （ui.registerSidebarPanel / ui.registerToolboxPage）合并为单一菜单列表，
 * 全部按 order 升序稳定排列 —— 插件/自定义项可通过 order 值插入到任意内置项之间。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { getPluginRegistry, isContributionActiveState } from '@/plugin/registry'
import type { PluginState } from '@/plugin/types'

/** 统一侧边栏菜单项 */
export interface SidebarMenuItem {
  /** 唯一 id（渲染 :key） */
  id: string
  /** 路由路径 */
  path: string
  /** 显示文本：isI18nKey 为 true 时为 i18n key，否则为纯文本标题 */
  labelKey: string
  /** 是否为 i18n key */
  isI18nKey: boolean
  /** SVG path d 属性（Heroicons outline 风格，viewBox 0 0 24 24） */
  icon: string
  /** 排序值，升序排列（越小越靠前），同值保持注册顺序 */
  order: number
  /** true 时用 startsWith 匹配（插件页等多级路由） */
  prefix?: boolean
  /** true 表示该内置入口可被同槽位的插件贡献目录顶替（让位），见 builtinSupersededBy。
   * 业务域入口（设备配对 / 终端会话）声明之；插件管理与设置恒在最末，不让位 */
  supersedable?: boolean
}

/** 自定义菜单项注册描述符（registerSidebarItem 扩展点入参） */
export interface SidebarMenuItemDescriptor {
  id: string
  path: string
  /** 显示文本：纯文本标题，或 isI18nKey 置 true 时传 i18n key */
  labelKey: string
  isI18nKey?: boolean
  /** SVG path d 属性，缺省使用通用清单图标 */
  icon?: string
  /** 排序值，升序排列（越小越靠前） */
  order: number
  /** true 时用 startsWith 匹配路由 */
  prefix?: boolean
}

/** 内置菜单项排序槽位 — 区间间隔 100，供插件/自定义项插入。
 * 设备配对(100) 置于首位；插件管理(9998) 与 设置(9999) 置于所有插件
 * 排序值之后，保证这两个入口永远排在菜单最末。说明：server 槽位(300)
 * 保留不复用，防止插件排序撞位 */
export const BUILTIN_MENU_ORDERS = {
  devices: 100,
  sessions: 200,
  server: 300,
  plugins: 9998,
  settings: 9999,
} as const

/** 插件/自定义项未指定 icon 时的兜底图标 */
const DEFAULT_MENU_ICON = 'M4 6h16M4 12h16M4 18h7'

/**
 * 内置菜单项（与插件共用 Heroicons outline 图标体系）
 *
 * 服务器管理页面（/server）已从导航中移除入口（产品决策：服务器常驻，
 * 用户不可开关，见 ServerSupervisor）。路由与页面代码保留，调试者可直接
 * 访问 /server URL 预览，未来 CLI 开发工具可复用此页面。
 */
export const builtinMenuItems: SidebarMenuItem[] = [
  {
    id: 'sessions',
    path: '/sessions',
    labelKey: 'desktop.sidebar.terminalSession',
    isI18nKey: true,
    // 终端图标（与 /sessions 页面头部一致），替代原文档图标以符合"终端会话"含义
    icon: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
    order: BUILTIN_MENU_ORDERS.sessions,
    supersedable: true,
  },
  // 服务器管理入口已移除：页面保留于 /server 供调试者直接访问 URL 预览。
  // 原菜单项：
  // {
  //   id: 'server',
  //   path: '/server',
  //   labelKey: 'desktop.sidebar.server',
  //   isI18nKey: true,
  //   icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01',
  //   order: BUILTIN_MENU_ORDERS.server,
  // },
  {
    id: 'devices',
    path: '/devices',
    labelKey: 'desktop.sidebar.devicePairing',
    isI18nKey: true,
    icon: 'M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z',
    order: BUILTIN_MENU_ORDERS.devices,
    supersedable: true,
  },
  {
    id: 'plugins',
    path: '/plugins',
    labelKey: 'desktop.plugin.title',
    isI18nKey: true,
    prefix: true,
    icon: 'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z',
    order: BUILTIN_MENU_ORDERS.plugins,
  },
  {
    id: 'settings',
    path: '/settings',
    labelKey: 'desktop.sidebar.settings',
    isI18nKey: true,
    icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z',
    order: BUILTIN_MENU_ORDERS.settings,
  },
]

/** 自定义菜单项注册表（模块级单例） */
const customItems = new Map<string, SidebarMenuItem>()
/** 响应式副本，供 Vue 模板消费 */
const customItemsRef: Ref<SidebarMenuItem[]> = ref([])

/**
 * 注册自定义侧边栏菜单项（扩展点，可指定 order 插入任意位置）
 *
 * @returns dispose 注销句柄（Disposable 模式，与插件注册表一致）
 */
export function registerSidebarItem(item: SidebarMenuItemDescriptor): { dispose: () => void } {
  customItems.set(item.id, {
    id: item.id,
    path: item.path,
    labelKey: item.labelKey,
    isI18nKey: item.isI18nKey ?? false,
    icon: item.icon ?? DEFAULT_MENU_ICON,
    order: item.order,
    prefix: item.prefix,
  })
  syncCustomItems()
  return {
    dispose: () => {
      if (customItems.delete(item.id)) {
        syncCustomItems()
      }
    },
  }
}

function syncCustomItems() {
  customItemsRef.value = [...customItems.values()]
}

/** 插件视图 → 统一菜单项（sidebar 与 toolbox 面板共用同一排序空间） */
function toMenuItem(view: {
  pluginId: string
  viewId: string
  viewType: string
  title: string
  icon?: string
  order: number
}): SidebarMenuItem {
  const isSidebar = view.viewType === 'sidebar'
  return {
    id: `plugin-${view.pluginId}-${view.viewId}`,
    path: isSidebar
      ? `/plugin/sidebar/${view.pluginId}/${view.viewId}`
      : `/plugin/toolbox/${view.pluginId}/${view.viewId}`,
    labelKey: view.title,
    isI18nKey: false,
    icon: view.icon ?? DEFAULT_MENU_ICON,
    order: view.order,
    prefix: true,
  }
}

/** 贡献视图的最小形态（注册表条目与测试桩共用） */
export interface ContributionViewRef {
  pluginId: string
  viewId: string
  viewType: string
  order: number
}

/**
 * 内置入口让位判据 —— 某运行态插件贡献的侧边栏目录**占用该内置项的 order 槽位**
 * （同 order 值）即视为该域已被插件接管，内置入口让位以避免同域出现两个入口。
 *
 * 按同槽位精确匹配而非「order 区间」判定：插在两个内置项之间（如 150）的新域目录
 * 只是插入排序位置，不代表接管了「设备配对」域，否则会误摘宿主入口。
 * 接管某域的插件必须把该域的目录项落在与内置入口相同的 order 上
 * （见 D6：设备与配对 100 / 终端会话 200，同域的其余项如连接历史取 101+）。
 *
 * 让位与否与贡献目录摘除、深链兜底共用 `isContributionActiveState` 判据：
 * 插件 error / 停用后贡献目录不再是「运行态插件的贡献」，内置入口随之恢复。
 *
 * @returns 顶替该内置入口的贡献目录（无则 null）
 */
export function builtinSupersededBy(
  item: { order: number },
  views: ContributionViewRef[],
  states: Record<string, PluginState>,
): ContributionViewRef | null {
  for (const view of views) {
    if (view.viewType !== 'sidebar') continue
    if (view.order !== item.order) continue
    if (!isContributionActiveState(states[view.pluginId])) continue
    return view
  }
  return null
}

/**
 * 侧边栏菜单组合子 — 合并内置 + 自定义 + 插件视图，按 order 升序稳定排列
 *
 * sort 为稳定排序：同 order 时保持 内置 → 自定义 → 插件注册 的先后顺序。
 * 声明了 `supersedable` 的内置入口在对应槽位出现运行态插件的贡献目录时让位（不重复入口）。
 */
export function useSidebarMenu(): { menuItems: ComputedRef<SidebarMenuItem[]> } {
  const registry = getPluginRegistry()

  const menuItems = computed<SidebarMenuItem[]>(() => {
    const states = registry.pluginStatesRef.value
    // 贡献目录与设置分组、内置入口让位共用同一生效判据：插件 error / 停用后
    // 其目录随之摘除，内置入口恢复，避免「同一域两个入口」与点开空目录（D7）
    const pluginViews = [...registry.sidebarViews.value, ...registry.toolboxViews.value].filter(
      (view) => isContributionActiveState(states[view.pluginId]),
    )
    const pluginItems = pluginViews.map(toMenuItem)

    const yielded = new Set<string>()
    for (const item of builtinMenuItems) {
      if (!item.supersedable) continue
      if (builtinSupersededBy(item, pluginViews, states)) {
        yielded.add(item.id)
      }
    }

    const all = [
      ...builtinMenuItems.filter((i) => !yielded.has(i.id)),
      ...customItemsRef.value,
      ...pluginItems,
    ]
    all.sort((a, b) => a.order - b.order)
    return all
  })

  return { menuItems }
}
