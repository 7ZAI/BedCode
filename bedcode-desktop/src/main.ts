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
import 'vue-sonner/style.css'
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

/** 插件运行时异常统一通道（宿主检测到插件异常时主动上报，见 PLUGIN_RUNTIME_ERROR） */
interface PluginRuntimeErrorPayload {
  plugin_id: string
  plugin_name: string
  kind: 'panic' | 'trap' | 'recovery_failed'
  error: string
}

const app = createApp(App)

// 全局异常处理：Vue 组件渲染/生命周期错误与未捕获的 Promise 拒绝统一提示，
// 避免静默失败（与插件运行时异常通道互为补充，见下方 plugin:runtime-error 监听）
// 细节全量进 console，toast 只做用户可见的「发生了未知错误」提示
app.config.errorHandler = (err, _instance, info) => {
  console.error(`[GlobalError] ${info || 'render'}:`, err)
  useToast().error(i18n.global.t('desktop.plugin.runtimeUnexpected'))
}

window.addEventListener('unhandledrejection', (event) => {
  console.error('[GlobalError] unhandledrejection:', event.reason)
  useToast().error(i18n.global.t('desktop.plugin.runtimeUnexpected'))
})

app.use(createPinia())
app.use(router)
app.use(i18n)

// 初始化共享模块运行时（供插件通过 @binblink/plugin-sdk-desktop 访问）
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
// 配置失败（如 hooks 脚本拷贝失败）→ 弹窗提示，插件状态不变
listen<PluginErrorPayload>('plugin:error', (event) => {
  const { plugin_id, error } = event.payload
  const toast = useToast()
  console.error(`[Plugin] ${plugin_id} self-check failed:`, error)
  toast.error(i18n.global.t('desktop.plugin.selfCheckFailed', { plugin: plugin_id, error }))
})

// 监听插件运行时异常事件（宿主 WASM 调用 panic / trap / 自动恢复失败时发送）
// 按 kind 提示不同文案；error 细节可能很长（panic 消息/回溯），toast 截断展示，全量进 console
listen<PluginRuntimeErrorPayload>('plugin:runtime-error', (event) => {
  const { plugin_id, plugin_name, kind, error } = event.payload
  const toast = useToast()
  console.error(`[Plugin] ${plugin_name} (${plugin_id}) runtime error [${kind}]:`, error)
  const shortError = error.length > 120 ? `${error.slice(0, 120)}…` : error
  const key =
    kind === 'panic'
      ? 'desktop.plugin.runtimePanic'
      : kind === 'trap'
        ? 'desktop.plugin.runtimeTrap'
        : 'desktop.plugin.runtimeRecoveryFailed'
  toast.error(i18n.global.t(key, { name: plugin_name, error: shortError }))
})

app.mount('#app')

// 初始化插件系统（非阻塞，失败不影响主应用）
import { pluginLoader } from '@/plugin/loader'
pluginLoader.loadAll().catch(e => {
  console.error('[PluginSystem] Failed to initialize:', e)
})