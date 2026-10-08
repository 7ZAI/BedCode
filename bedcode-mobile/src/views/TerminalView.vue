<template>
  <div class="h-full">
    <component
      :is="terminalView.component"
      v-if="terminalView"
      :key="sessionId"
      :session-id="sessionId"
    />
    <div
      v-else
      class="flex items-center justify-center h-full text-sm text-[var(--mobile-text-disabled)]"
    >
      {{ t('mobile.plugin.loadFailed') }}
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * TerminalView（宿主薄壳；票 15：终端 UI 域整体下沉插件）
 *
 * 终端消费 UI（渲染/输入/订阅/设置/引导等约 9.6k 行）已迁入内置 app
 * `com.bedcode.terminal-session` 前端（plugins/terminal-session/src/terminal/**）；
 * 宿主只保留三件无业务语义的事：
 * - 路由壳：`/mobile/terminal/:id` URL 形状与返回/底部导航语义零变更
 *   （旧壳将被宿主壳 `/mobile/shell` 替换，本薄壳随之退役）
 * - 宿主机制 provide：通用组件（FileSidebar / PluginTerminalBar 与 safeArea
 *   （App.vue 既有 provide））——插件经 inject('bedcodeHostComponents') 消费；
 *   跨插件注册表渲染与代码浏览域组件留在宿主，不随终端域迁移
 * - 插件激活：直达深链（插件未激活）时先激活，注册完成后自动重渲染
 *
 * 会话 id 经 props.sessionId 透传（插件内缺省回落宿主活动会话，壳内运行面路径一致）。
 */
import { computed, onMounted, provide, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { getPluginRegistry, pluginLoader } from '@/plugin'
import FileSidebar from '@/components/FileSidebar.vue'
import PluginTerminalBar from '@/plugin/components/PluginTerminalBar.vue'

const TERMINAL_PLUGIN_ID = 'com.bedcode.terminal-session'

const route = useRoute()
const { t } = useI18n()
const registry = getPluginRegistry()

const sessionId = computed(() => (route.params.id as string) || '')

/** 终端主视图（插件注册；插件 id 校验防其他插件占位） */
const terminalView = ref<{ pluginId: string; component: any } | null>(null)

function syncTerminalView(): void {
  const v = registry.terminalView.value
  terminalView.value = v && v.pluginId === TERMINAL_PLUGIN_ID ? v : null
}
syncTerminalView()

// 宿主机制 provide（插件经 inject('bedcodeHostComponents') 消费）
provide('bedcodeHostComponents', { FileSidebar, PluginTerminalBar })

async function ensurePluginActive(): Promise<void> {
  if (terminalView.value || pluginLoader.getActivePlugin(TERMINAL_PLUGIN_ID)) return
  await pluginLoader.activate(TERMINAL_PLUGIN_ID)
}

onMounted(() => {
  void ensurePluginActive()
})

watch(
  () => registry.terminalView.value,
  () => syncTerminalView(),
)
</script>
