import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import router from './router'
import App from './App.vue'
import i18n from './locales'
import { initPlatform } from '@/composables/usePlatform'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useWslStore } from '@/stores/wsl'
import { useToast } from '@/composables/useToast'
import { setupSharedRuntime } from '@/plugin/shared-runtime'
import './style.css'

interface PluginNotifyPayload {
  plugin_id: string
  title: string
  body: string
}

interface PluginErrorPayload {
  plugin_id: string
  error: string
}

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(i18n)

// 初始化共享模块运行时（供插件通过 @bedcode/plugin-sdk-desktop 访问）
setupSharedRuntime(i18n, router)

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

// 监听插件通知事件（由 host_notify Host Function 发送）
listen<PluginNotifyPayload>('plugin:notify', (event) => {
  const { title, body } = event.payload
  const toast = useToast()
  if (body) {
    toast.info(`${title}: ${body}`)
  } else {
    toast.info(title)
  }
})

// 监听插件自检失败事件（由 host_mark_plugin_error Host Function 发送）
// 插件启动配置失败（如 hooks 脚本拷贝失败）→ 弹窗提示，启用状态由插件管理页刷新
listen<PluginErrorPayload>('plugin:error', (event) => {
  const { plugin_id, error } = event.payload
  const toast = useToast()
  console.error(`[Plugin] ${plugin_id} self-check failed:`, error)
  toast.error(i18n.global.t('desktop.plugin.selfCheckFailed', { plugin: plugin_id, error }))
})

app.mount('#app')

// 初始化插件系统（非阻塞，失败不影响主应用）
import { pluginLoader } from '@/plugin/loader'
pluginLoader.loadAll().catch(e => {
  console.error('[PluginSystem] Failed to initialize:', e)
})