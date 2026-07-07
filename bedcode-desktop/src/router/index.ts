import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/sessions' },
    {
      path: '/sessions',
      name: 'sessions',
      component: () => import('@/views/SessionsConfigView.vue'),
    },
    {
      path: '/session-manager',
      name: 'session-manager',
      component: () => import('@/views/SessionManagerView.vue'),
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
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/SettingsView.vue'),
    },
    // TODO: 插件功能暂未上线
    // {
    //   path: '/plugins',
    //   name: 'plugins',
    //   component: () => import('@/views/PluginsView.vue'),
    // },
    // {
    //   path: '/plugins/:id/config',
    //   name: 'plugin-config',
    //   component: () => import('@/views/PluginConfigView.vue'),
    // },
    // {
    //   path: '/plugin/sidebar/:pluginId/:viewId',
    //   name: 'plugin-sidebar-view',
    //   component: () => import('@/plugin/components/PluginViewHost.vue'),
    //   props: true,
    // },
    // {
    //   path: '/plugin/toolbox/:pluginId/:viewId',
    //   name: 'plugin-toolbox-view',
    //   component: () => import('@/plugin/components/PluginViewHost.vue'),
    //   props: true,
    // },
    {
      path: '/terminal-window/:id',
      name: 'terminal-window',
      component: () => import('@/views/TerminalWindowView.vue'),
    },
  ],
})

export default router
