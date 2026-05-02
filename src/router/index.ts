import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/sessions',
    },
    {
      path: '/sessions',
      name: 'sessions',
      component: () => import('@/views/desktop/SessionsView.vue'),
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('@/views/desktop/DevicesView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/desktop/SettingsView.vue'),
    },
    {
      path: '/mobile/devices',
      name: 'mobile-devices',
      component: () => import('@/views/mobile/DevicesView.vue'),
    },
    {
      path: '/mobile/terminal/:id',
      name: 'mobile-terminal',
      component: () => import('@/views/mobile/TerminalView.vue'),
    },
    {
      path: '/mobile/quick-actions',
      name: 'mobile-quick-actions',
      component: () => import('@/views/mobile/QuickActionsView.vue'),
    },
    {
      path: '/mobile/history',
      name: 'mobile-history',
      component: () => import('@/views/mobile/HistoryView.vue'),
    },
    {
      path: '/mobile/settings',
      name: 'mobile-settings',
      component: () => import('@/views/mobile/SettingsView.vue'),
    },
  ],
})

export default router
