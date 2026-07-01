import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/sessions' },
    {
      path: '/sessions',
      name: 'sessions',
      component: () => import('@/modules/desktop/views/SessionsConfigView.vue'),
    },
    {
      path: '/session-manager',
      name: 'session-manager',
      component: () => import('@/modules/desktop/views/SessionManagerView.vue'),
    },
    {
      path: '/server',
      name: 'server',
      component: () => import('@/modules/desktop/views/ServerView.vue'),
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('@/modules/desktop/views/DevicesView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/modules/desktop/views/SettingsView.vue'),
    },
    {
      path: '/plugins',
      name: 'plugins',
      component: () => import('@/modules/desktop/views/PluginsView.vue'),
    },
    {
      path: '/plugins/:id/config',
      name: 'plugin-config',
      component: () => import('@/modules/desktop/views/PluginConfigView.vue'),
    },
    {
      path: '/plugin/sidebar/:pluginId/:viewId',
      name: 'plugin-sidebar-view',
      component: () => import('@/modules/shared/plugin/components/PluginViewHost.vue'),
      props: true,
    },
    {
      path: '/plugin/toolbox/:pluginId/:viewId',
      name: 'plugin-toolbox-view',
      component: () => import('@/modules/shared/plugin/components/PluginViewHost.vue'),
      props: true,
    },
    {
      path: '/terminal-window/:id',
      name: 'terminal-window',
      component: () => import('@/modules/desktop/views/TerminalWindowView.vue'),
    },
  ],
})

export default router
