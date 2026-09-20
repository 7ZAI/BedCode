import { createRouter, createWebHistory } from 'vue-router'
import { pluginLoader } from '@/plugin/loader'
import { getPluginRegistry } from '@/plugin/registry'
import { builtinMenuItems, builtinSupersededBy } from '@/composables/useSidebarMenu'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/sessions' },
    {
      // 兜底壳：会话页由 com.bedcode.session 插件贡献目录接管（本路由在插件
      // 激活时经 beforeEach 重定向到 `/plugin/sidebar/<pluginId>/<viewId>`），
      // 未激活 / error / 停用时渲染本页——不白屏且保留基本启停删除（票 13）
      path: '/sessions',
      name: 'session',
      component: () => import('@/views/SessionsFallbackView.vue'),
    },
    {
      path: '/server',
      name: 'server',
      component: () => import('@/views/ServerView.vue'),
    },
    {
      // 兜底壳：设备与配对 / 连接历史由 com.bedcode.session 插件贡献目录接管
      // （本路由在插件激活时经 beforeEach 重定向到 `/plugin/sidebar/<pluginId>/<viewId>`），
      // 未激活 / error / 停用时渲染本页——不白屏且保留基本设备管理（票 14）
      path: '/devices',
      name: 'devices',
      component: () => import('@/views/DevicesFallbackView.vue'),
    },
    {
      // 连接历史深链兜底：插件接管时同样被重定向；未接管时落到同一兜底壳（不 404）
      path: '/devices/:id/history',
      name: 'device-history',
      component: () => import('@/views/DevicesFallbackView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/SettingsView.vue'),
    },
    {
      path: '/plugins',
      name: 'plugins',
      component: () => import('@/views/PluginsView.vue'),
    },
    {
      path: '/plugins/:id',
      name: 'plugin-detail',
      component: () => import('@/views/PluginDetailView.vue'),
    },
    {
      path: '/plugins/:id/config',
      name: 'plugin-config',
      component: () => import('@/views/PluginConfigView.vue'),
    },
    {
      path: '/plugin/sidebar/:pluginId/:viewId',
      name: 'plugin-sidebar-view',
      component: () => import('@/plugin/components/PluginViewHost.vue'),
      props: true,
    },
    {
      path: '/plugin/toolbox/:pluginId/:viewId',
      name: 'plugin-toolbox-view',
      component: () => import('@/plugin/components/PluginViewHost.vue'),
      props: true,
    },
    {
      path: '/terminal-window/:id',
      name: 'terminal-window',
      component: () => import('@/views/TerminalWindowView.vue'),
    },
  ],
})

// 插件视图路由守卫：确保懒激活插件在直接访问 URL 时被激活
router.beforeEach(async (to) => {
  if (to.name === 'plugin-sidebar-view' || to.name === 'plugin-toolbox-view') {
    const pluginId = to.params.pluginId as string
    if (pluginId && !pluginLoader.getActivePlugin(pluginId)) {
      await pluginLoader.activate(pluginId)
    }
    return
  }

  // 内置入口让位后的深链兜底：原路由仍可达（不 404），重定向到接管该域的贡献目录。
  // 判据与侧边栏让位、设置分组摘除共用 builtinSupersededBy —— 插件未激活 / error 时
  // 不重定向，宿主页照常渲染（兜底壳）
  const builtin = builtinMenuItems.find((i) => i.supersedable && i.path === to.path)
  if (builtin) {
    const registry = getPluginRegistry()
    const view = builtinSupersededBy(
      builtin,
      [...registry.sidebarViews.value, ...registry.toolboxViews.value],
      registry.pluginStatesRef.value,
    )
    if (view) {
      return {
        name: 'plugin-sidebar-view',
        params: { pluginId: view.pluginId, viewId: view.viewId },
      }
    }
  }
})

export default router
