import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'mobile-home',
      component: () => import('@/modules/mobile/components/MobileSwipeContainer.vue'),
    },
    {
      path: '/mobile',
      name: 'mobile-home-alt',
      component: () => import('@/modules/mobile/components/MobileSwipeContainer.vue'),
    },
    {
      path: '/mobile/devices',
      name: 'mobile-devices',
      component: () => import('@/modules/mobile/views/DevicesView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/sessions',
      name: 'mobile-sessions',
      component: () => import('@/modules/mobile/views/SessionsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/terminal/:id',
      name: 'mobile-terminal',
      component: () => import('@/modules/mobile/views/TerminalView.vue'),
      meta: { keepAlive: true },
    },
    {
      path: '/mobile/toolbox',
      name: 'mobile-toolbox',
      component: () => import('@/modules/mobile/views/ToolboxView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/files/:id',
      name: 'mobile-files',
      component: () => import('@/modules/mobile/views/CodeExplorerView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings',
      name: 'mobile-settings',
      component: () => import('@/modules/mobile/views/SettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/scan',
      name: 'mobile-scan',
      component: () => import('@/modules/mobile/views/ScanView.vue'),
    },
  ],
})

export default router
