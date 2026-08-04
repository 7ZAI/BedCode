import { createRouter, createWebHistory } from 'vue-router'

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

export default router
