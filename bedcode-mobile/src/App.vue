<template>
  <div :class="themeClasses.container">
    <MobileLayout />

    <!-- Global Toast Container（vue-sonner，richColors 区分等级） -->
    <Toaster
      :theme="toasterTheme"
      position="top-center"
      rich-colors
      :mobile-offset="{ top: safeArea.top, bottom: safeArea.bottom }"
    />

    <!-- Plugin Dialog Host -->
    <PluginDialogHost />

    <!-- 文件系统授权弹窗（插件目录授权，全局挂载） -->
    <FsAuthDialog />

    <!-- 开屏动画（启动就绪/兜底时长后淡出并卸载）
      SplashScreen 暂时下线：移动端开屏页暂停展示，实现完整保留
      （components/SplashScreen.vue + config/splash.ts + composables/useAppStartup.ts）。
      恢复方式：取消下方这行注释，并同步取消 script 里 SplashScreen import 与
      showSplash ref 的注释，把 'ref' 加回 vue import。 -->
    <!-- <SplashScreen v-if="showSplash" @closed="showSplash = false" /> -->
  </div>
</template>

<script setup lang="ts">
// SplashScreen 下线后 'ref' 无使用点，已暂时从 import 移除（恢复开屏时补回）
import { provide, computed, onMounted, onUnmounted } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { Toaster } from 'vue-sonner'
import MobileLayout from '@/components/MobileLayout.vue'
// SplashScreen 暂时下线：开屏页暂停挂载，恢复时取消注释
// import SplashScreen from '@/components/SplashScreen.vue'
import { usePlatform } from '@/composables/usePlatform'
import { useOrientation } from '@/composables/useOrientation'
import { useEdgeToEdge } from '@/composables/useEdgeToEdge'
import PluginDialogHost from '@/plugin/components/PluginDialogHost.vue'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
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

// 开屏动画:淡出动画结束后卸载（SplashScreen 暂时下线，恢复时取消注释）
// const showSplash = ref(true)
// const { startAdvertise, stopAdvertise } = useMdnsAdvertiser()

onMounted(async () => {
  setupTheme()
  setupFontSize()
  // 开屏启动任务打点:主题/字体/安全区等 UI 子系统就绪
  // SplashScreen 下线期间此打点只写注册表、无 UI 消费方,保留以保证恢复开屏
  // 时 startupReady 语义完整
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
</script>
