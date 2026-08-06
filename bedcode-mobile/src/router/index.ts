import { createRouter, createWebHistory } from 'vue-router'
import { pluginLoader } from '@/plugin/loader'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'mobile-home',
      component: () => import('@/components/MobileSwipeContainer.vue'),
    },
    {
      path: '/mobile',
      name: 'mobile-home-alt',
      component: () => import('@/components/MobileSwipeContainer.vue'),
    },
    {
      path: '/mobile/devices',
      name: 'mobile-devices',
      component: () => import('@/views/DevicesView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/sessions',
      name: 'mobile-sessions',
      component: () => import('@/views/SessionsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/terminal/:id',
      name: 'mobile-terminal',
      component: () => import('@/views/TerminalView.vue'),
    },
    {
      path: '/mobile/toolbox',
      name: 'mobile-toolbox',
      component: () => import('@/views/ToolboxView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/toolbox/preset-tasks',
      name: 'mobile-preset-tasks',
      component: () => import('@/views/PresetTasksView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/files/:id',
      name: 'mobile-files',
      component: () => import('@/views/CodeExplorerView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings',
      name: 'mobile-settings',
      component: () => import('@/views/SettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/connection',
      name: 'mobile-settings-connection',
      component: () => import('@/views/settings/ConnectionSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/notifications',
      name: 'mobile-settings-notifications',
      component: () => import('@/views/settings/NotificationSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/authentication',
      name: 'mobile-settings-authentication',
      component: () => import('@/views/settings/AuthenticationSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/appearance',
      name: 'mobile-settings-appearance',
      component: () => import('@/views/settings/AppearanceSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/about',
      name: 'mobile-settings-about',
      component: () => import('@/views/settings/AboutSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/plugins',
      name: 'mobile-plugins',
      component: () => import('@/views/PluginView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/scan',
      name: 'mobile-scan',
      component: () => import('@/views/ScanView.vue'),
    },
    {
      path: '/mobile/discover',
      name: 'mobile-discover',
      component: () => import('@/views/DiscoverView.vue'),
    },
  ],
})

// 插件动态路由守卫：深度链接/插件停用后残留导航时懒激活插件（activate 内 registerRoute 完成 addRoute）。
// 插件已激活时直接放行；激活成功需重导航命中刚注册的动态路由（vue-router 守卫中 addRoute 不影响当前导航匹配）。
const pluginRouteActivateTried = new Set<string>()
router.beforeEach(async (to) => {
  const pluginRoute = to.meta.pluginRoute as { pluginId: string } | undefined
  if (!pluginRoute?.pluginId) return
  if (pluginLoader.getActivePlugin(pluginRoute.pluginId)) return
  if (pluginRouteActivateTried.has(pluginRoute.pluginId)) return
  pluginRouteActivateTried.add(pluginRoute.pluginId)
  await pluginLoader.activate(pluginRoute.pluginId)
  // 激活成功（动态路由已注册）则重导航命中；失败放行，页面展示加载失败兜底
  if (pluginLoader.getActivePlugin(pluginRoute.pluginId)) return to.fullPath
})

export default router
