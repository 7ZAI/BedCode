import { createApp } from 'vue'
import { createPinia } from 'pinia'
import router from './router'
import App from './App.vue'
import i18n from './locales'
import { initPlatform } from '@/composables/usePlatform'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { initPluginSystem } from '@/plugin'
import './style.css'
import './styles/mobile.css'
import 'vue-sonner/style.css'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
app.use(router)
app.use(i18n)

// 预初始化：并行执行平台检测和设置加载
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()

// 异步初始化流程：设置加载 → 语言初始化 → 插件系统初始化 → 挂载应用
// 插件系统必须在设置加载后初始化，因为插件的启用状态依赖设置
Promise.all([
  initPlatform(),
  settingsStore.loadSettings(),
]).then(async () => {
  // 设置加载完成后初始化语言偏好
  i18nStore.initLanguage()
  console.log('[Init] Platform and settings pre-loaded')

  // 设置就绪后初始化插件系统
  try {
    await initPluginSystem(app, pinia, router, i18n)
    console.log('[Init] Plugin system initialized')
  } catch (e) {
    console.error('[Init] Plugin system init failed:', e)
  }

  app.mount('#app')
})
