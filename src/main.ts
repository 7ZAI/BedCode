import { createApp } from 'vue'
import { createPinia } from 'pinia'
import router from './router'
import App from './App.vue'
import { initPlatform } from '@/modules/shared/composables/usePlatform'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import './style.css'
import './styles/mobile.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)

// 预初始化：并行执行平台检测和设置加载
// 这样可以避免路由守卫中的阻塞等待
const settingsStore = useSettingsStore()
Promise.all([
  initPlatform(),
  settingsStore.loadSettings(),
]).then(() => {
  console.log('[Init] Platform and settings pre-loaded')
})

app.mount('#app')