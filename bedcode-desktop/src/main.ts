// WDIO E2E 测试探针：暴露 window.wdioTauri（execute/mock/log），无 Rust wdio 插件配合时静默空转
import '@wdio/tauri-plugin'
import { logger } from '@/utils/frontendLogger'
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import router from './router'
import App from './App.vue'
import i18n from './locales'
import { initPlatform } from '@/composables/usePlatform'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useWslStore } from '@/stores/wsl'
import { useToast } from '@/composables/useToast'
import { setupSharedRuntime } from '@/plugin/shared-runtime'
import { setupPluginRuntimeListeners } from '@/plugin/runtime-listeners'
import { initFrontendLogger } from '@/utils/frontendLogger'
import 'vue-sonner/style.css'
import './style.css'

const app = createApp(App)

// 尽早初始化（dev 专属）：把前端日志经 frontendLogger 批量转发到 Rust 落盘
// （runtime.*.log，target=frontend）；release 构建 logger 为空函数，零开销零输出
initFrontendLogger()

// 全局异常处理：Vue 组件渲染/生命周期错误与未捕获的 Promise 拒绝统一提示，
// 避免静默失败（与插件运行时异常通道互为补充，见 plugin/runtime-listeners.ts）
// 细节全量进 console，toast 只做用户可见的「发生了未知错误」提示
app.config.errorHandler = (err, _instance, info) => {
  logger.error(`[GlobalError] ${info || 'render'}:`, err)
  useToast().error(i18n.global.t('desktop.plugin.runtimeUnexpected'))
}

window.addEventListener('unhandledrejection', (event) => {
  logger.error('[GlobalError] unhandledrejection:', event.reason)
  useToast().error(i18n.global.t('desktop.plugin.runtimeUnexpected'))
})

// 正式版（release）禁用右键默认菜单：Windows WebView2 / macOS WKWebView 可由 JS
// preventDefault 抑制；Linux 由 Rust 端 GTK context-menu 信号处理（此处无效果但无害）。
// dev 构建保留右键菜单，便于开发调试（检查元素等）。capture 阶段确保先于页面内
// 任意处理器（含 xterm.js）执行；preventDefault 不阻断事件继续传播，不影响既有行为。
if (!import.meta.env.DEV) {
  window.addEventListener('contextmenu', (event) => event.preventDefault(), { capture: true })
}

app.use(createPinia())
app.use(router)
app.use(i18n)

// 初始化共享模块运行时（供插件通过 @binblink/bedcode-plugin-sdk-desktop 访问）
setupSharedRuntime(i18n, router)

// 注册插件事件监听（notify / self-check error / runtime-error，实现见 plugin/runtime-listeners.ts）
setupPluginRuntimeListeners()

// 预初始化：并行执行平台检测、设置加载和 WSL 信息缓存
// WSL 命令执行较慢（可能触发虚拟机启动），提前加载避免弹窗卡顿
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const wslStore = useWslStore()
Promise.all([initPlatform(), settingsStore.loadSettings(), wslStore.loadWslInfo()]).then(
  ([platformInfo]) => {
    // 平台标记：<html> 上加平台专属 class，供 CSS 做平台条件样式。
    // - platform-desktop / platform-mobile：通用桌面/移动区分
    // - platform-linux：仅 Linux，启用 font-size:115% + --ui-scale:1.15 体系修正
    //   WebKitGTK 下字号偏小（issue 06 已移除 zoom，避免破坏 xterm 鼠标坐标系）
    if (platformInfo.isDesktop) {
      document.documentElement.classList.add('platform-desktop')
    } else if (platformInfo.isMobile) {
      document.documentElement.classList.add('platform-mobile')
    }
    if (platformInfo.isLinux) {
      document.documentElement.classList.add('platform-linux')
    }

    // 设置加载完成后初始化语言偏好
    i18nStore.initLanguage()
    logger.log('[Init] Platform, settings and WSL info pre-loaded')
  },
)

app.mount('#app')

// 初始化插件系统（非阻塞，失败不影响主应用）
import { pluginLoader } from '@/plugin/loader'
pluginLoader.loadAll().catch((e) => {
  logger.error('[PluginSystem] Failed to initialize:', e)
})
