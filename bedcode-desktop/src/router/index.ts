import { createRouter, createWebHistory } from 'vue-router'
import { pluginLoader } from '@/plugin/loader'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    // 默认落地插件页：会话 / 设备配对入口已下沉插件（票 13/14），宿主不再有
    // 业务页；服务器页（/server）按产品决策「常驻不可开关」已从导航移除入口
    // （仍可经 URL 直达做诊断），故落地页取插件管理
    { path: '/', redirect: '/plugins' },
    {
      path: '/server',
      name: 'server',
      component: () => import('@/views/ServerView.vue'),
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
      // 通用插件窗口：任何插件注册的 `kind: 'page'` 视图都能独占一个独立窗口。
      // `bareWindow` = 宿主不套桌面外壳（侧边栏 / 标题栏），内容全部由插件视图给
      path: '/plugin/window/:pluginId/:viewId',
      name: 'plugin-window-view',
      component: () => import('@/views/PluginWindowHostView.vue'),
      meta: { bareWindow: true },
    },
    {
      path: '/terminal-window/:id',
      name: 'terminal-window',
      // 票 03a / 票 05：终端窗口内容整体下沉会话插件——本路由走通用插件窗口宿主，
      // 缺省目标即会话插件的 `session.terminal-window` 视图；宿主不留终端兜底实现
      component: () => import('@/views/PluginWindowHostView.vue'),
      meta: { bareWindow: true },
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
})

export default router
