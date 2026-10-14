import { createRouter, createWebHistory } from 'vue-router'
import { pluginLoader } from '@/plugin/loader'

const router = createRouter({
  history: createWebHistory(),
  // 阶段 B（票 2026-10-09）：旧宿主四页容器与其页面已退役删除，
  // 唯一入口 = 宿主壳 /mobile/shell；设置子页与文件浏览器沿用（壳/插件尚未承接的过渡面）。
  // 退役面不得回接 —— 见 src/__tests__/views/retiredMobileHostUiFace.test.ts
  routes: [
    {
      path: '/',
      redirect: '/mobile/shell',
    },
    {
      path: '/mobile/files/:id',
      name: 'mobile-files',
      component: () => import('@/views/CodeExplorerView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/connection',
      name: 'mobile-settings-connection',
      component: () => import('@/views/settings/ConnectionSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/authentication',
      name: 'mobile-settings-authentication',
      component: () => import('@/views/settings/AuthenticationSettingsView.vue'),
      meta: { standAlone: true },
    },
    {
      path: '/mobile/settings/egress',
      name: 'mobile-settings-egress',
      component: () => import('@/views/settings/EgressSettingsView.vue'),
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
      // 宿主壳（WASM 应用运行平台）：应用默认入口，
      // 内部流转由壳自己的屏幕栈管理（src/shell/），不占用路由表
      path: '/mobile/shell',
      name: 'mobile-shell',
      component: () => import('@/shell/views/ShellView.vue'),
      meta: { standAlone: true },
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
