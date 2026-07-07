import { createApp } from 'vue'
import { createPinia } from 'pinia'
// TODO: 插件功能暂未上线
// import { listen } from '@tauri-apps/api/event'
import router from './router'
import App from './App.vue'
import i18n from './locales'
import { initPlatform } from '@/composables/usePlatform'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useWslStore } from '@/stores/wsl'
import { useToast } from '@/composables/useToast'
import './style.css'

interface PluginSetupResult {
  success: boolean
  message: string
  token_generated: boolean
}

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(i18n)

// 预初始化：并行执行平台检测、设置加载和 WSL 信息缓存
// WSL 命令执行较慢（可能触发虚拟机启动），提前加载避免弹窗卡顿
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const wslStore = useWslStore()
Promise.all([
  initPlatform(),
  settingsStore.loadSettings(),
  wslStore.loadWslInfo(),
]).then(() => {
  // 设置加载完成后初始化语言偏好
  i18nStore.initLanguage()
  console.log('[Init] Platform, settings and WSL info pre-loaded')
})

// TODO: 插件功能暂未上线
// 监听插件配置结果事件
// listen<PluginSetupResult>('plugin-setup-result', (event) => {
//   const toast = useToast()
//   const result = event.payload
//
//   if (result.success) {
//     toast.success(result.message)
//     if (result.token_generated) {
//       setTimeout(() => toast.info(i18n.global.t('common.notification.tokenUpdated')), 1000)
//     }
//   } else {
//     toast.error(result.message, 5000)
//   }
// })

app.mount('#app')

// TODO: 插件功能暂未上线
// 初始化插件系统（非阻塞，失败不影响主应用）
// import { pluginLoader } from '@/plugin/loader'
// pluginLoader.loadAll().catch(e => {
//   console.error('[PluginSystem] Failed to initialize:', e)
// })