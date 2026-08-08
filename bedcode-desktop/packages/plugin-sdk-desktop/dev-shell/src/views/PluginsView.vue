<script setup lang="ts">
/**
 * PluginsView — 插件管理（桌面端）：状态徽章、激活/停用、全部注册项一览
 * （侧边栏面板 / 工具箱 / 状态栏 / 输入扩展 / 终端工具栏 / 标题栏 / 页面工具栏 /
 * 文件处理器 / HTTP 端点 / 文件服务挂载）
 */
import { useI18n } from 'vue-i18n'
import {
  endpoints,
  fileHandlers,
  inputExtensions,
  mounts,
  pageToolbarItems,
  plugins,
  sidebarPanels,
  statusBarItems,
  terminalToolbarItems,
  titleBarItems,
  toolboxPages,
} from '../registry'
import { deactivatePlugin, loadPlugins } from '../loader'
import { isSvgIcon } from '../utils/icon'

const { t } = useI18n()

const stateLabel: Record<string, string> = {
  activated: 'devshell.plugins.state.activated',
  loaded: 'devshell.plugins.state.loaded',
  deactivated: 'devshell.plugins.state.deactivated',
  error: 'devshell.plugins.state.error',
}

const stateClass: Record<string, string> = {
  activated: 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]',
  loaded: 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]',
  deactivated: 'bg-[var(--bg-hover)] text-[var(--text-tertiary)]',
  error: 'bg-red-500/10 text-red-500',
}

async function toggle(pluginId: string) {
  const record = plugins.value.find((p) => p.id === pluginId)
  if (!record) return
  if (record.state === 'deactivated') {
    await loadPlugins()
  } else {
    await deactivatePlugin(pluginId)
  }
}
</script>

<template>
  <div class="p-6 space-y-4">
    <h2 class="text-lg font-semibold">{{ t('devshell.nav.plugins') }}</h2>

    <div v-if="plugins.length === 0" class="py-16 text-center text-sm text-[var(--text-tertiary)]">
      {{ t('devshell.toolbox.emptyHint') }}
    </div>

    <div
      v-for="record in plugins"
      :key="record.id"
      class="bg-card border border-[var(--border)] rounded-card p-4 space-y-3 shadow-card"
    >
      <div class="flex items-center gap-3">
        <span v-if="isSvgIcon(record.manifest.icon)" class="w-8 h-8 flex items-center justify-center text-[var(--color-primary)]">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-8 h-8">
            <path :d="record.manifest.icon" />
          </svg>
        </span>
        <span v-else class="text-2xl leading-none">{{ record.manifest.icon || '🧩' }}</span>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-semibold truncate">{{ record.name }}</p>
          <p class="text-[11px] text-[var(--text-tertiary)] truncate">{{ record.id }} · v{{ record.manifest.version || '-' }}</p>
        </div>
        <span class="px-2 py-0.5 rounded-tag text-[11px] flex-shrink-0" :class="stateClass[record.state]">
          {{ t(stateLabel[record.state]) }}
        </span>
      </div>

      <p v-if="record.state === 'error' && record.error" class="text-xs text-red-500 bg-red-500/10 rounded-lg px-3 py-2 break-all">
        {{ record.error }}
      </p>

      <div class="flex items-center gap-2">
        <button
          class="px-3 py-1.5 rounded-btn text-xs font-medium transition-colors duration-200"
          :class="
            record.state === 'deactivated'
              ? 'bg-[var(--color-primary)] text-white'
              : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
          "
          @click="toggle(record.id)"
        >
          {{ record.state === 'deactivated' ? t('devshell.plugins.activate') : t('devshell.plugins.deactivate') }}
        </button>
      </div>

      <!-- 注册项一览 -->
      <div class="grid gap-2 text-xs md:grid-cols-2">
        <div v-if="sidebarPanels.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.sidebar') }}</p>
          <p v-for="x in sidebarPanels.filter((y) => y.pluginId === record.id)" :key="x.panel.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.panel.title }}
          </p>
        </div>
        <div v-if="toolboxPages.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.nav.toolbox') }}</p>
          <p v-for="x in toolboxPages.filter((y) => y.pluginId === record.id)" :key="x.page.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.page.title }}
          </p>
        </div>
        <div v-if="fileHandlers.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.fileHandlers') }}</p>
          <p v-for="x in fileHandlers.filter((y) => y.pluginId === record.id)" :key="x.handler.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)] font-mono">
            {{ x.handler.id }}（{{ x.handler.extensions.join(' ') }}）
          </p>
        </div>
        <div v-if="endpoints.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.endpoints') }}</p>
          <p v-for="x in endpoints.filter((y) => y.pluginId === record.id)" :key="x.path" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)] font-mono">
            {{ x.path }}
          </p>
        </div>
        <div v-if="mounts.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.mounts') }}</p>
          <p v-for="x in mounts.filter((y) => y.pluginId === record.id)" :key="x.mountPath" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)] font-mono break-all">
            {{ x.mountPath }} → [{{ x.roots.join(', ') }}]
          </p>
        </div>
        <div v-if="titleBarItems.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.titlebar') }}</p>
          <p v-for="x in titleBarItems.filter((y) => y.pluginId === record.id)" :key="x.item.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.item.label }}
          </p>
        </div>
        <div v-if="statusBarItems.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.statusbar') }}</p>
          <p v-for="x in statusBarItems.filter((y) => y.pluginId === record.id)" :key="x.item.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.item.label }}
          </p>
        </div>
        <div v-if="pageToolbarItems.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.pageToolbar') }}</p>
          <p v-for="x in pageToolbarItems.filter((y) => y.pluginId === record.id)" :key="x.item.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.item.label }} → {{ x.item.target }}
          </p>
        </div>
        <div v-if="inputExtensions.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.inputExtensions') }}</p>
          <p v-for="x in inputExtensions.filter((y) => y.pluginId === record.id)" :key="x.ext.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.ext.label }}
          </p>
        </div>
        <div v-if="terminalToolbarItems.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--text-tertiary)] mb-1">{{ t('devshell.plugins.toolbar') }}</p>
          <p v-for="x in terminalToolbarItems.filter((y) => y.pluginId === record.id)" :key="x.item.id" class="px-2 py-1 rounded-tag bg-[var(--bg-hover)]">
            {{ x.item.label }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
