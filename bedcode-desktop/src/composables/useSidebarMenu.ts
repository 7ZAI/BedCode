/**
 * Sidebar Menu — 侧边栏菜单统一模型 + 可配置排序的扩展点
 *
 * 内置菜单项、宿主自定义项（registerSidebarItem）与插件注册的面板
 * （ui.registerSidebarPanel / ui.registerToolboxPage）合并为单一菜单列表，
 * 全部按 order 升序稳定排列 —— 插件/自定义项可通过 order 值插入到任意内置项之间。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { getPluginRegistry, isContributionActiveState } from '@/plugin/registry'

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

/** 内置菜单项排序槽位 — 插件管理与设置恒在最末（9998/9999）。
 * 业务域槽位（设备配对 100 / 终端会话 200）已随票 13/14 下沉 com.bedcode.terminal-session
 * 插件，宿主不再占用；插件贡献目录按自身 order 排布（同域约定仍为 100/200，
 * 见插件侧目录注册，宿主不感知） */
export const BUILTIN_MENU_ORDERS = {
  plugins: 9998,
  settings: 9999,
} as const

/** 插件/自定义项未指定 icon 时的兜底图标 */
const DEFAULT_MENU_ICON = 'M4 6h16M4 12h16M4 18h7'

/**
 * 内置菜单项（与插件共用 Heroicons outline 图标体系）
 *
 * 会话 / 设备配对入口已随票 13/14 下沉 com.bedcode.terminal-session 插件：宿主不再提供
 * 兜底菜单与页面（路由 /sessions、/devices 及其兜底壳已删除），插件未激活时
 * 侧边栏不显示这两个入口；插件激活后由其贡献目录接管菜单排序槽位。
 *
 * 服务器管理页面（/server）已从导航中移除入口（产品决策：服务器常驻，
 * 用户不可开关，见 ServerSupervisor）。路由与页面代码保留，调试者可直接
 * 访问 /server URL 预览，未来 CLI 开发工具可复用此页面。
 */
export const builtinMenuItems: SidebarMenuItem[] = [
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

/**
 * 侧边栏菜单组合子 — 合并内置 + 自定义 + 插件视图，按 order 升序稳定排列
 *
 * sort 为稳定排序：同 order 时保持 内置 → 自定义 → 插件注册 的先后顺序。
 * 插件贡献目录经 `isContributionActiveState` 过滤：插件 error / 停用后目录随之
 * 摘除，避免点开空目录（D7）；宿主不再提供业务域兜底入口（票 13/14 已下沉插件），
 * 故无让位逻辑。
 */
export function useSidebarMenu(): { menuItems: ComputedRef<SidebarMenuItem[]> } {
  const registry = getPluginRegistry()

  const menuItems = computed<SidebarMenuItem[]>(() => {
    const states = registry.pluginStatesRef.value
    const pluginViews = [...registry.sidebarViews.value, ...registry.toolboxViews.value].filter(
      (view) => isContributionActiveState(states[view.pluginId]),
    )
    const pluginItems = pluginViews.map(toMenuItem)

    const all = [...builtinMenuItems, ...customItemsRef.value, ...pluginItems]
    all.sort((a, b) => a.order - b.order)
    return all
  })

  return { menuItems }
}
