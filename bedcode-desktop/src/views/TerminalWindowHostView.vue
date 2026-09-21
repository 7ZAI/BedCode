<template>
  <PluginViewHost
    :plugin-id="SESSION_PLUGIN_ID"
    :view-id="SESSION_TERMINAL_WINDOW_VIEW_ID"
  />
</template>

<script setup lang="ts">
/**
 * 终端窗口宿主壳（票 03a；票 05 收口）— 宿主 /terminal-window/:id 路由组件
 *
 * 终端窗口内容已整体下沉 session 插件（方案 1）：本组件只做两件事——
 * 1. 把「终端宿主能力」注入插件视图（settings accessor / 背景图命令桥 /
 *    插件扩展点 registry），经 provide 传给 PluginViewHost 渲染的
 *    插件 `session.terminal-window` 视图；
 * 2. 渲染 PluginViewHost（插件视图宿主，视图组件注册在插件 activate）。
 *
 * 能力桥设计（ADR 0022 裁剪线）：终端渲染/写入/IME 下沉插件，但宿主存储面
 * （settingsStore 持久化、set_terminal_bg_image 文件复制命令）属宿主引擎
 * 原语，经注入桥接——与 context.session.openTerminal 原语同构（宿主留原语，
 * 插件持编排）。输出面不设桥（票 05 摘除）：输出改经插件 WASM 命令面
 * `session.output.pull` 轮询拉取 `host-session.output-ring-fetch` 原语，
 * 插件前端不依赖宿主注入；`sessionId` 由插件视图经路由参数自取。
 *
 * 注入契约类型真源：`plugins/terminal-session/src/components/terminal/
 * terminalHostCapabilities.ts`（宿主不 import 插件，就地定义同构结构）。
 */
import { provide, reactive, computed } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import PluginViewHost from '@/plugin/components/PluginViewHost.vue'
import { getPluginRegistry } from '@/plugin/registry'
import { useSettingsStore } from '@/stores/settings'
import { logger } from '@/utils/frontendLogger'
import { TERMINAL_HOST_CAPABILITIES_KEY } from '@/plugin/terminal-host-capabilities-contract'
import type { TerminalSettingsAccessor } from '@/plugin/terminal-host-capabilities-contract'

// 会话插件常量（与 plugins/terminal-session/plugin.json 一致；插件 id 改名票 06 集中化）
const SESSION_PLUGIN_ID = 'com.bedcode.terminal-session'
const SESSION_TERMINAL_WINDOW_VIEW_ID = 'session.terminal-window'

const settingsStore = useSettingsStore()
const registry = getPluginRegistry()

// ==================== 终端设置桥（TerminalSettingsAccessor） ====================

/** 背景图片选择允许的图片扩展名（与宿主旧 TerminalWindowView 一致） */
const BG_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico']

const settings: TerminalSettingsAccessor = {
  getFontSize: () => settingsStore.settings.ui.terminal_font_size ?? 12,
  getTheme: () => settingsStore.settings.ui.terminal_theme || 'dracula',
  getBgImage: () => settingsStore.settings.ui.terminal_bg_image || '',
  getBgOpacity: () => settingsStore.settings.ui.terminal_bg_opacity ?? 30,
  getServerPort: () => 8080, // 背景图 URL 端口：宿主本地服务器静态端点（插件侧不感知实际端口）
  save: (patch) => {
    void settingsStore.saveSettings({
      ui: {
        ...settingsStore.settings.ui,
        ...(patch.fontSize != null ? { terminal_font_size: patch.fontSize } : {}),
        ...(patch.theme != null ? { terminal_theme: patch.theme } : {}),
        ...(patch.bgImage != null ? { terminal_bg_image: patch.bgImage } : {}),
        ...(patch.bgOpacity != null ? { terminal_bg_opacity: patch.bgOpacity } : {}),
      },
    })
  },
  onChange: () => () => {}, // 终端设置只由终端窗口设置面板读写（单写者），无外部变化源
}

// ==================== 背景图命令桥 ====================

/** 选择系统图片文件并设为终端背景（复制到应用数据目录，避免原图移动/删除后失效） */
async function pickAndSetBgImage(): Promise<boolean> {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Background Image', extensions: BG_IMAGE_EXTENSIONS }],
    })
    if (!selected || typeof selected !== 'string') return false
    const fileName = await invoke<string | null>('set_terminal_bg_image', { sourcePath: selected })
    if (fileName) {
      // 设置中存原始文件名用于回显，实际复制文件由后端统一命名为 terminal_bg.<ext>
      const displayName = selected.split(/[\\/]/).pop() || fileName
      await settingsStore.saveSettings({
        ui: { ...settingsStore.settings.ui, terminal_bg_image: displayName },
      })
    }
    return true
  } catch (e) {
    logger.error('[TerminalWindowHost] Failed to set background image:', e)
    return false
  }
}

async function removeBgImage(): Promise<void> {
  try {
    await invoke('set_terminal_bg_image', { sourcePath: null })
    await settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_bg_image: '' },
    })
  } catch (e) {
    logger.error('[TerminalWindowHost] Failed to remove background image:', e)
  }
}

const bgImageName = computed(() => {
  const v = settingsStore.settings.ui.terminal_bg_image
  if (!v) return ''
  return v.split(/[\\/]/).pop() || v
})

// bgImage 桥整体包 reactive：imageName/hasImage 是 computed 属性（reactive 自动
// 解包），插件壳模板访问时响应式依赖跟随 settingsStore 变化，不依赖 provide 时点快照
const bgImageBridge = reactive({
  pickAndSet: pickAndSetBgImage,
  remove: removeBgImage,
  imageName: computed(() => bgImageName.value),
  hasImage: computed(() => !!settingsStore.settings.ui.terminal_bg_image),
})

// ==================== 注入 ====================

// 插件扩展点：宿主 registry 响应式数组直接注入（插件壳复刻渲染按钮；
// TerminalExtensionItem 结构镜像——id/label/icon/pluginId/onClick 字段宿主侧超集）
provide(TERMINAL_HOST_CAPABILITIES_KEY, {
  settings,
  bgImage: bgImageBridge,
  extensions: {
    terminalToolbarItems: registry.terminalToolbarItems,
    titleBarItems: registry.titleBarItems,
    pageToolbarItems: registry.pageToolbarItems,
  },
})
</script>
