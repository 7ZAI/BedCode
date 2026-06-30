import { createRouter, createWebHistory } from 'vue-router'
import { initPlatform } from '@/modules/shared/composables/usePlatform'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'root',
      component: () => import('@/modules/shared/views/LoadingView.vue'),
    },
    {
      path: '/sessions',
      name: 'sessions',
      component: () => import('@/modules/desktop/views/SessionsConfigView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/session-manager',
      name: 'session-manager',
      component: () => import('@/modules/desktop/views/SessionManagerView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('@/modules/desktop/views/DevicesView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/modules/desktop/views/SettingsView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/mobile',
      name: 'mobile-home',
      component: () => import('@/modules/mobile/components/MobileSwipeContainer.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/devices',
      name: 'mobile-devices',
      component: () => import('@/modules/mobile/views/DevicesView.vue'),
      meta: { platform: 'mobile', standAlone: true },
    },
    {
      path: '/mobile/sessions',
      name: 'mobile-sessions',
      component: () => import('@/modules/mobile/views/SessionsView.vue'),
      meta: { platform: 'mobile', standAlone: true },
    },
    {
      path: '/mobile/terminal/:id',
      name: 'mobile-terminal',
      component: () => import('@/modules/mobile/views/TerminalView.vue'),
      meta: { platform: 'mobile', keepAlive: true },
    },
    {
      path: '/mobile/toolbox',
      name: 'mobile-toolbox',
      component: () => import('@/modules/mobile/views/ToolboxView.vue'),
      meta: { platform: 'mobile', standAlone: true },
    },
    {
      path: '/mobile/files/:id',
      name: 'mobile-files',
      component: () => import('@/modules/mobile/views/CodeExplorerView.vue'),
      meta: { platform: 'mobile', standAlone: true },
    },
    {
      path: '/mobile/settings',
      name: 'mobile-settings',
      component: () => import('@/modules/mobile/views/SettingsView.vue'),
      meta: { platform: 'mobile', standAlone: true },
    },
    {
      path: '/mobile/scan',
      name: 'mobile-scan',
      component: () => import('@/modules/mobile/views/ScanView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/terminal-window/:id',
      name: 'terminal-window',
      component: () => import('@/modules/desktop/views/TerminalWindowView.vue'),
      meta: { platform: 'desktop' },
    },
  ],
})

router.beforeEach(async (to, from, next) => {
  if (to.name === 'root') {
    const platformInfo = await initPlatform()

    if (platformInfo.isMobile) {
      next({ name: 'mobile-home', replace: true })
    } else {
      next({ name: 'sessions', replace: true })
    }
    return
  }

  next()
})

export default router