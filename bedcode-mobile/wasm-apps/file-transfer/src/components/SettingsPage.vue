<script setup lang="ts">
/**
 * SettingsPage — 设置页（Mobile，宿主设置二级页形态）
 *
 * 页面骨架交给宿主 `SettingsSubPage`（PluginRouteView 的 `header: true` 模式）：
 * 页头（返回 + 标题）、平台底色、顶部/底部安全区都由平台提供——插件自绘页头时
 * 拿不到 Android 的环境变量安全区（`env(safe-area-inset-*)` 在 Android WebView
 * 恒为 0，宿主用 JS 注入值），标题会压在系统状态栏下；底色的蓝灰 app 主题也会
 * 与宿主设置流跳色（2026-10-15 真机反馈）。
 *
 * 内容仍套 `.ft-app.ft-host-page`：保留应用强调色（--ft-accent*），结构色重绑到
 * 宿主 token（见 styles.css 的「平台设置页变体」段），与宿主设置页视觉一致。
 *
 * 宿主 PluginRouteView 以无 props 方式渲染组件，本组件负责实例化 useSettings
 * 并把 settingsApi + t 注入 SettingsSection。入口：FileTransferView 齿轮与
 * 宿主壳设置的「文件传输」项，都经 context.ui.openPage('settings') 整体路由跳转。
 */
import { inject, onMounted } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { useSettings } from '../composables/useSettings'
import SettingsSection from './SettingsSection.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)
const settingsApi = useSettings(context)

onMounted(() => {
  void settingsApi.load()
})
</script>

<template>
  <div class="ft-app ft-host-page">
    <SettingsSection :settings-api="settingsApi" :t="t" />
  </div>
</template>
