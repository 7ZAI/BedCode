import { createRouter, createWebHistory } from 'vue-router'
import { pluginLoader } from '@/plugin/loader'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/sessions' },
    {
      path: '/sessions',
      name: 'session',
      component: () => import('@/views/SessionsConfigView.vue'),
    },
    {
      path: '/server',
      name: 'server',
      component: () => import('@/views/ServerView.vue'),
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('@/views/DevicesView.vue'),
    },
    {
      path: '/devices/:id/history',
      name: 'device-history',
      component: () => import('@/views/ConnectionHistoryView.vue'),
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
  }
})

export default router
