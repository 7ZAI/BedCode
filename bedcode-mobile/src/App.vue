<template>
  <div :class="themeClasses.container">
    <!-- 路由出口：旧宿主布局（MobileLayout + MobileNav）随阶段 B 退役，
         壳自行承担布局框与安全区（见 src/shell/views/ShellView.vue） -->
    <router-view />

    <!-- Global Toast Container（vue-sonner，richColors 区分等级） -->
    <Toaster
      :theme="toasterTheme"
      position="top-center"
      rich-colors
      :mobile-offset="{ top: safeArea.top, bottom: safeArea.bottom }"
    />

    <!-- Plugin Dialog Host -->
    <PluginDialogHost />

    <!-- 插件全局弹窗（宿主统一渲染，文件传输等插件经 context.ui.showDialog 触发） -->
    <PluginGlobalDialog />

    <!-- 文件系统授权弹窗（插件目录授权，全局挂载） -->
    <FsAuthDialog />

    <!-- 外网访问授权弹窗（Egress L3，请求时懒触发，全局挂载） -->
    <EgressConsentDialog />

    <!-- 开屏组件已禁用（2026-09-13 排查：启动期存在两层前端开屏——index.html 静态首屏
         与本处的 Vue 开屏组件，现仅保留静态首屏 + Android 原生纯色开屏底）。
         恢复方式：取消下方两个组件标签的注释，并同步恢复 script 中的
         SplashScreen / SplashScreenNative / ACTIVE_SPLASH_CANDIDATE 导入、
         showSplash / splashIsNative 声明及 vue 导入里的 ref；
         渲染候选页仍由 config/splash.ts 的 ACTIVE_SPLASH_CANDIDATE 决定。
    <SplashScreen v-if="!splashIsNative && showSplash" @closed="showSplash = false" />
    <SplashScreenNative v-else-if="showSplash" @closed="showSplash = false" />
    -->
  </div>
</template>

<script setup lang="ts">
import { provide, computed, onMounted, onUnmounted } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { Toaster } from 'vue-sonner'
// 开屏组件已禁用（恢复说明见模板注释块）
// import SplashScreen from '@/components/SplashScreen.vue'
// import SplashScreenNative from '@/components/SplashScreenNative.vue'
// import { ACTIVE_SPLASH_CANDIDATE } from '@/config/splash'
import { usePlatform } from '@/composables/usePlatform'
import { useOrientation } from '@/composables/useOrientation'
import { useEdgeToEdge } from '@/composables/useEdgeToEdge'
import PluginDialogHost from '@/plugin/components/PluginDialogHost.vue'
import PluginGlobalDialog from '@binblink/bedcode-plugin-sdk-mobile/ui/plugin-global-dialog'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
import EgressConsentDialog from '@/components/EgressConsentDialog.vue'
// 宿主公共组件面：文件浏览器留宿主作跨应用通用能力（spec §2 裁决），
// 经 bedcodeHostComponents 注入给应用侧的既有挂载位 —— 见文件末尾 provide 说明
import FileExplorer from '@/components/FileExplorer.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import { useTheme } from '@/composables/useTheme'
import { syncLinkCryptoContextToNative, initLinkCryptoPinSync } from '@/composables/useLinkEncryption'
import { useFontSize } from '@/composables/useFontSize'
import { useSettingsStore } from '@/stores/settings'
import { completeStartupTask } from '@/composables/useAppStartup'
// mDNS 广播暂时禁用：移动端目前不需要被发现，避免扫描到自身
// import { useMdnsAdvertiser } from '@/composables/useMdnsAdvertiser'

const { platformInfo } = usePlatform()
const { isLandscape, orientation } = useOrientation()
const { safeArea, keyboardInfo, isReady } = useEdgeToEdge()

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

// Toaster 主题跟随应用设置（'system' 时由 sonner 自身监听系统偏好）
const settingsStore = useSettingsStore()
const toasterTheme = computed(() => settingsStore.settings.ui.theme as 'light' | 'dark' | 'system')

// 开屏动画状态已随组件一并禁用（恢复说明见模板注释块）
// const showSplash = ref(true)
// /** 当前候选是否为原生系统开屏样式复刻(候选 2);false 即终端叙事动画(候选 1) */
// const splashIsNative = ACTIVE_SPLASH_CANDIDATE === 'native'
// const { startAdvertise, stopAdvertise } = useMdnsAdvertiser()

onMounted(async () => {
  setupTheme()
  setupFontSize()
  // 开屏启动任务打点:主题/字体/安全区等 UI 子系统就绪
  completeStartupTask('ui')
  // 链路加密上下文启动同步（issue 09）：Rust 侧事件 WS 建连前需要拿到
  // 当前开关与 pin；失败静默（默认全关，不影响明文现状）
  void syncLinkCryptoContextToNative()
  // 监听认证成功时 Rust 广播的 pin（配对码/QR/reauth/生物认证统一出口）：
  // 写入 localStorage 供 HTTP/终端 WS 通道与设置页指纹展示（修复 pin 断链）
  try {
    const unlisten = await initLinkCryptoPinSync()
    unlistenQueue.push(unlisten)
  } catch (e) {
    logger.error('[App] initLinkCryptoPinSync failed:', e)
  }
  // mDNS 广播暂时禁用
  // try {
  //   const deviceName = `BedCode-Mobile-${Math.random().toString(36).slice(2, 6)}`
  //   await startAdvertise(0, deviceName)
  // } catch (e) {
  //   logger.warn('[App] mDNS advertise failed:', e)
  // }
})

// pin 事件监听器句柄（onUnmounted 释放，避免 HMR/测试重复注册泄漏）
const unlistenQueue: Array<() => void> = []

onUnmounted(async () => {
  cleanupTheme()
  for (const unlisten of unlistenQueue.splice(0)) {
    try {
      unlisten()
    } catch (e) {
      logger.error('[App] unlisten failed:', e)
    }
  }
  // await stopAdvertise()
})

// Provide to child components
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)
provide('safeAreaReady', isReady)

// ==================== 宿主公共组件面（票 2026-10-10 批次 C5） ====================
//
// 文件浏览器按 spec §2 裁决**留在宿主**作公共组件，不下沉到 wasm app——它是跨应用
// 的通用能力（任何应用都可能要浏览远程会话的文件），下沉会让每个应用各写一份。
//
// 应用侧通过 inject('bedcodeHostComponents') 取用：wasm app 的终端域已经写好了
// `hostComponents.FileSidebar` / `hostComponents.FileExplorer` 两个挂载位，此前因为
// 无人 provide 一直是死代码（v-if 恒假）。这里补上 provide，两件事同时成立：
// ① 文件浏览器从宿主可达（它随应用运行面一起进入宿主组件树，inject 沿树可见）；
// ② 全屏路由 `/mobile/files/:id` 之外多了一条不需要占宿主路由的入口。
//
// 为什么在 App.vue 而不是壳里：插件组件可能被非壳路径渲染（如 dev 调试入口），
// 挂到应用根才能保证凡在宿主组件树内都取得到；这层是**通用注册**不是业务编排，
// 不违反 §5.1（无业务类型 / 无业务状态 / 无业务默认值）。
provide('bedcodeHostComponents', { FileExplorer, FileSidebar })
</script>
