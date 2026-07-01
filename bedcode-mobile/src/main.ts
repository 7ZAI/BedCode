import { createApp } from 'vue'
import { createPinia } from 'pinia'
import router from './router'
import App from './App.vue'
import i18n from './locales'
import { initPlatform } from '@/composables/usePlatform'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import './style.css'
import './styles/mobile.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(i18n)

// 预初始化：并行执行平台检测和设置加载
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
Promise.all([
  initPlatform(),
  settingsStore.loadSettings(),
]).then(() => {
  // 设置加载完成后初始化语言偏好
  i18nStore.initLanguage()
  console.log('[Init] Platform and settings pre-loaded')
})

app.mount('#app')
