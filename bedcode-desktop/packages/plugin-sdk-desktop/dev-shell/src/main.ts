/**
 * Dev Shell 入口（桌面端）
 *
 * 初始化顺序：pinia → router → i18n → 共享运行时（必须在导入插件前）→ 加载插件 → 挂载。
 */
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createI18n } from 'vue-i18n'
import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'
import { initSharedRuntime } from './shared-runtime'
import { loadPlugins } from './loader'
import { zhCN, en } from './locales'
import { readSavedLocale } from './locale'
import './styles/style.css'
import './styles/dev.css'

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

// 宿主在设置中动态调整 --ui-scale；dev-shell 固定 1（骨架无设置联动）
document.documentElement.style.setProperty('--ui-scale', '1')

const router = createRouter({
  history: createWebHashHistory(),
  routes: [{ path: '/', redirect: '/toolbox' }],
})

app.use(pinia)
app.use(router)
app.use(i18n)

await initSharedRuntime({ pinia, router, i18n })

await loadPlugins()

app.mount('#app')
