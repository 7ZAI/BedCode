/**
 * Sidebar Menu — 侧边栏菜单统一模型 + 可配置排序的扩展点
 *
 * 内置菜单项、宿主自定义项（registerSidebarItem）与插件注册的面板
 * （ui.registerSidebarPanel / ui.registerToolboxPage）合并为单一菜单列表，
 * 全部按 order 升序稳定排列 —— 插件/自定义项可通过 order 值插入到任意内置项之间。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { getPluginRegistry } from '@/plugin/registry'

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

/** 内置菜单项排序槽位 — 区间间隔 100，供插件/自定义项插入。
 * 设备配对(100) 置于首位；插件管理(9998) 与设置(9999) 置于所有插件排序值之后，
 * 保证这两个入口永远排在菜单最末。说明：server 槽位(300) 保留不复用，防止插件排序撞位 */
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

/**
 * 侧边栏菜单组合子 — 合并内置 + 自定义 + 插件视图，按 order 升序稳定排列
 *
 * sort 为稳定排序：同 order 时保持 内置 → 自定义 → 插件注册 的先后顺序
 */
export function useSidebarMenu(): { menuItems: ComputedRef<SidebarMenuItem[]> } {
  const registry = getPluginRegistry()

  const menuItems = computed<SidebarMenuItem[]>(() => {
    const pluginItems = [...registry.sidebarViews.value, ...registry.toolboxViews.value].map(toMenuItem)
    const all = [...builtinMenuItems, ...customItemsRef.value, ...pluginItems]
    all.sort((a, b) => a.order - b.order)
    return all
  })

  return { menuItems }
}
