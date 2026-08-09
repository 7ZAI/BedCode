/**
 * Dev Shell 入口
 *
 * 初始化顺序与宿主一致：pinia → router → i18n → 共享运行时（必须在导入插件前）→
 * 加载插件（async）→ 挂载应用。mount 前先渲染 loading 骨架，插件就绪后展示真实内容。
 */
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createI18n } from 'vue-i18n'
import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'
import { initSharedRuntime } from './shared-runtime'
import { loadPlugins } from './loader'
import { initTheme } from './theme'
import { presetTasksApi } from './mock/preset-tasks'
import { mobileApi } from './mock/mobile-api'
import { dialogService } from './mock/dialog-service'
import { zhCN, en } from './locales'
import { readSavedLocale } from './locale'
import './styles/main.css'
import './styles/mobile.css'

const app = createApp(App)
const pinia = createPinia()

const i18n = createI18n({
  legacy: false,
  locale: readSavedLocale(),
  fallbackLocale: 'en',
  messages: { 'zh-CN': zhCN, en },
  missingWarn: false,
  fallbackWarn: false,
})

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/toolbox' },
    { path: '/toolbox', component: () => import('./views/ToolboxView.vue') },
    { path: '/terminal', component: () => import('./views/MockTerminalView.vue') },
    { path: '/plugins', component: () => import('./views/PluginsView.vue') },
  ],
})

app.use(pinia)
app.use(router)
app.use(i18n)

// 主题初始化（index.html 内联脚本已消除首帧闪烁，这里建立监听/持久化）
initTheme()

// 共享运行时必须在插件模块导入之前就位（插件模块顶层可能调用 getVue() 等）
await initSharedRuntime({
  pinia,
  router,
  i18n,
  presetTasks: presetTasksApi,
  dialogs: dialogService,
  mobileApi,
})

// 插件激活完成后挂载（ToolboxView 依赖注册结果）
await loadPlugins()

app.mount('#app')
