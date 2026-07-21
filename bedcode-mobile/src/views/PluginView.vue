<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="flex-shrink-0 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center gap-3">
      <button
        class="flex-shrink-0 p-1 -ml-1 text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] active:opacity-80 transition-colors"
        @click="router.back()"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ $t('mobile.plugin.title') }}</h1>
    </header>

    <!-- Plugin List -->
    <div class="flex-1 overflow-y-auto">
      <!-- Empty state -->
      <div v-if="plugins.length === 0" class="flex flex-col items-center justify-center h-full px-4">
        <svg class="w-12 h-12 text-[var(--mobile-text-disabled)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
        </svg>
        <p class="text-[var(--mobile-text-disabled)] text-sm">{{ $t('mobile.plugin.noPlugins') }}</p>
      </div>

      <!-- Plugin cards -->
      <div v-else class="p-4 space-y-3">
        <div
          v-for="plugin in plugins"
          :key="plugin.id"
          class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl overflow-hidden"
        >
          <!-- Plugin row -->
          <div
            class="flex items-center justify-between px-4 py-3 cursor-pointer active:opacity-80 transition-colors"
            @click="expandedPlugin = expandedPlugin === plugin.id ? null : plugin.id"
          >
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium text-[var(--mobile-text-primary)] truncate">{{ plugin.name }}</div>
              <div class="text-xs text-[var(--mobile-text-muted)] mt-0.5">v{{ plugin.version }} · {{ plugin.author }}</div>
            </div>
            <Toggle v-model="pluginEnabledStates[plugin.id]" @update:model-value="(v: boolean) => handlePluginToggle(plugin.id, v)" />
          </div>

          <!-- Expanded details -->
          <Transition name="expand">
            <div v-if="expandedPlugin === plugin.id" class="px-4 pb-3 pt-0 border-t border-[var(--mobile-border)]">
              <div class="space-y-1.5 pt-3 text-xs text-[var(--mobile-text-muted)]">
                <div>{{ $t('mobile.plugin.version') }}: {{ plugin.version }}</div>
                <div>{{ $t('mobile.plugin.author') }}: {{ plugin.author }}</div>
                <div>{{ $t('mobile.plugin.permissions') }}: {{ plugin.permissions.join(', ') || '-' }}</div>
                <div>{{ $t('mobile.plugin.extensions') }}: {{ getPluginExtensions(plugin) }}</div>
              </div>
            </div>
          </Transition>
        </div>
      </div>

      <!-- Plugin Settings Sections -->
      <div
        v-for="section in pluginRegistry.settingsSections.value"
        :key="section.id"
        class="px-4 py-3 border-b border-[var(--mobile-border)]"
      >
        <h3 class="text-[var(--mobile-accent)]/80 text-[0.9375rem] font-semibold mb-3 tracking-wider uppercase">{{ section.section }}</h3>
        <PluginSettingsHost :plugin-id="section.pluginId" :component="section.component" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginView - 插件管理页面
 *
 * 独立页面，从设置页跳转进入
 * 展示已安装插件列表、启用/禁用、详情展开、插件自定义设置区
 */
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import Toggle from '@/components/Toggle.vue'
import { pluginListLoaded, pluginSetEnabled, pluginIsEnabled } from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { getPluginRegistry } from '@/plugin/registry'
import PluginSettingsHost from '@/plugin/components/PluginSettingsHost.vue'
import type { PluginInfo } from '@/plugin/types'

const router = useRouter()
const { t } = useI18n()
const toast = useToast()
const pluginRegistry = getPluginRegistry()

const plugins = ref<PluginInfo[]>([])
const pluginEnabledStates = ref<Record<string, boolean>>({})
const expandedPlugin = ref<string | null>(null)

onMounted(async () => {
  try {
    plugins.value = await pluginListLoaded()
    for (const p of plugins.value) {
      pluginEnabledStates.value[p.id] = await pluginIsEnabled(p.id)
    }
  } catch {
    // 插件系统可能未就绪
  }
})

async function handlePluginToggle(pluginId: string, enabled: boolean) {
  try {
    await pluginSetEnabled(pluginId, enabled)
    if (enabled) {
      await pluginLoader.activate(pluginId)
    } else {
      await pluginLoader.deactivate(pluginId)
    }
  } catch (e: any) {
    toast.error(t(enabled ? 'mobile.plugin.activateFailed' : 'mobile.plugin.deactivateFailed', { error: e.message || String(e) }))
    // 恢复开关状态
    pluginEnabledStates.value[pluginId] = !enabled
  }
}

function getPluginExtensions(plugin: PluginInfo): string {
  const parts: string[] = []
  if (plugin.contributes.views.length > 0) parts.push('toolbox')
  if (plugin.contributes.navTab) parts.push('navTab')
  if (plugin.contributes.terminal) parts.push('terminal')
  if (plugin.contributes.settings) parts.push('settings')
  return parts.join(', ') || '-'
}
</script>

<style scoped>
.expand-enter-active,
.expand-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  opacity: 0;
  max-height: 0;
}

.expand-enter-to,
.expand-leave-from {
  opacity: 1;
  max-height: 200px;
}
</style>
