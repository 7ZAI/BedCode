import { createRouter, createWebHistory } from 'vue-router'
import { initPlatform } from '@/composables/usePlatform'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'root',
      // 不设置固定重定向，由路由守卫动态决定
      component: () => import('@/views/LoadingView.vue'),
    },
    {
      path: '/sessions',
      name: 'sessions',
      component: () => import('@/views/desktop/SessionsView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('@/views/desktop/DevicesView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/desktop/SettingsView.vue'),
      meta: { platform: 'desktop' },
    },
    {
      path: '/mobile/devices',
      name: 'mobile-devices',
      component: () => import('@/views/mobile/DevicesView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/sessions',
      name: 'mobile-sessions',
      component: () => import('@/views/mobile/SessionsView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/terminal/:id',
      name: 'mobile-terminal',
      component: () => import('@/views/mobile/TerminalView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/quick-actions',
      name: 'mobile-quick-actions',
      component: () => import('@/views/mobile/QuickActionsView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/history',
      name: 'mobile-history',
      component: () => import('@/views/mobile/HistoryView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/settings',
      name: 'mobile-settings',
      component: () => import('@/views/mobile/SettingsView.vue'),
      meta: { platform: 'mobile' },
    },
    {
      path: '/mobile/scan',
      name: 'mobile-scan',
      component: () => import('@/views/mobile/ScanView.vue'),
      meta: { platform: 'mobile' },
    },
  ],
})

/**
 * 全局路由守卫：根据平台自动重定向
 *
 * 首次进入应用时，检测平台并跳转到对应的默认页面：
 * - 桌面端：/sessions
 * - 移动端：/mobile/devices
 */
router.beforeEach(async (to, from, next) => {
  // 根路由需要动态重定向
  if (to.name === 'root') {
    const platformInfo = await initPlatform()

    if (platformInfo.isMobile) {
      // 移动端跳转到设备页面
      next({ name: 'mobile-devices', replace: true })
    } else {
      // 桌面端跳转到会话页面
      next({ name: 'sessions', replace: true })
    }
    return
  }

  // 其他路由正常导航
  next()
})

export default router
