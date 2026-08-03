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
      <h1 class="flex-1 text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ $t('mobile.plugin.title') }}</h1>
      <!-- 安装入口 -->
      <button
        class="flex-shrink-0 w-8 h-8 flex items-center justify-center rounded-lg bg-[var(--mobile-accent)] text-white active:opacity-80 transition-opacity"
        :disabled="installing"
        @click="showInstallSheet = true"
      >
        <svg v-if="!installing" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
        <div v-else class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
      </button>
    </header>

    <!-- Plugin List -->
    <div class="flex-1 overflow-y-auto">
      <!-- Empty state -->
      <div v-if="plugins.length === 0" class="flex flex-col items-center justify-center h-full px-8 text-center">
        <svg class="w-12 h-12 text-[var(--mobile-text-disabled)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
        </svg>
        <p class="text-[var(--mobile-text-disabled)] text-sm">{{ $t('mobile.plugin.noPlugins') }}</p>
        <p class="text-[var(--mobile-text-muted)] text-xs mt-2">{{ $t('mobile.plugin.noPluginsHint') }}</p>
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
            class="flex items-center gap-3 px-4 py-3 cursor-pointer active:opacity-80 transition-colors"
            @click="expandedPlugin = expandedPlugin === plugin.id ? null : plugin.id"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium text-[var(--mobile-text-primary)] truncate">{{ plugin.name }}</span>
                <!-- 状态徽章 -->
                <span
                  class="flex-shrink-0 inline-flex items-center px-1.5 py-0.5 rounded-tag text-[10px] font-medium"
                  :class="stateBadgeClass(plugin.state)"
                >
                  {{ $t(getStateKey(plugin.state)) }}
                </span>
              </div>
              <!-- 描述或错误信息 -->
              <div class="text-xs mt-0.5 truncate text-[var(--mobile-text-muted)]">
                <template v-if="isErrorState(plugin.state)">
                  <span class="text-[var(--mobile-danger-color)]">⚠ {{ getErrorMessage(plugin.state) }}</span>
                </template>
                <template v-else>
                  {{ plugin.description || plugin.id }}
                </template>
              </div>
              <div class="text-xs text-[var(--mobile-text-muted)] mt-0.5">
                v{{ plugin.version }} · {{ getSourceLabel(plugin.source) }}
              </div>
            </div>
            <Toggle v-model="pluginEnabledStates[plugin.id]" @update:model-value="(v: boolean) => handlePluginToggle(plugin.id, v)" />
          </div>

          <!-- Expanded details -->
          <Transition name="expand">
            <div v-if="expandedPlugin === plugin.id" class="px-4 pb-3 pt-0 border-t border-[var(--mobile-border)]">
              <div class="space-y-1.5 pt-3 text-xs text-[var(--mobile-text-muted)]">
                <div>{{ $t('mobile.plugin.id') }}: <span class="font-mono text-[var(--mobile-text-secondary)]">{{ plugin.id }}</span></div>
                <div>{{ $t('mobile.plugin.source') }}: {{ getSourceLabel(plugin.source) }}</div>
                <div>{{ $t('mobile.plugin.version') }}: {{ plugin.version }}</div>
                <div>{{ $t('mobile.plugin.author') }}: {{ plugin.author || '-' }}</div>
                <div>{{ $t('mobile.plugin.permissions') }}: {{ plugin.permissions.join(', ') || '-' }}</div>
                <div>{{ $t('mobile.plugin.extensions') }}: {{ getPluginExtensions(plugin) }}</div>
              </div>
              <!-- 卸载（仅用户安装的插件） -->
              <button
                v-if="!isBuiltin(plugin.source)"
                class="mt-3 w-full py-2 rounded-lg bg-[var(--mobile-danger-bg)] text-[var(--mobile-danger-color)] text-sm font-medium active:opacity-80 transition-opacity"
                @click="requestUninstall(plugin)"
              >
                {{ $t('mobile.plugin.uninstall') }}
              </button>
            </div>
          </Transition>
        </div>
      </div>
    </div>

    <!-- 安装弹层 -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showInstallSheet" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="closeInstallSheet()"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-5 shadow-xl modal-panel">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">{{ $t('mobile.plugin.install') }}</h3>

            <!-- 从文件安装 -->
            <button
              class="w-full flex items-center gap-3 px-4 py-3 rounded-xl bg-[var(--mobile-input-bg)] text-left active:opacity-80 transition-opacity"
              :disabled="installing"
              @click="handleInstallFile"
            >
              <svg class="w-5 h-5 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
              </svg>
              <span class="text-sm text-[var(--mobile-text-primary)]">{{ $t('mobile.plugin.installFromFile') }}</span>
            </button>

            <!-- 从 URL 安装 -->
            <div class="mt-3">
              <div class="flex items-center gap-3 px-4 py-3 rounded-xl bg-[var(--mobile-input-bg)]">
                <svg class="w-5 h-5 text-[var(--mobile-accent)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5M10.172 13.828a4 4 0 010-5.656l3-3a4 4 0 115.656 5.656l-1.5 1.5" />
                </svg>
                <input
                  v-model="installUrl"
                  type="url"
                  class="flex-1 bg-transparent text-sm text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-text-disabled)] outline-none"
                  :placeholder="$t('mobile.plugin.urlPlaceholder')"
                  @keydown.enter="handleInstallUrl"
                />
              </div>
              <button
                class="mt-3 w-full py-2.5 rounded-xl bg-[var(--mobile-accent)] text-white text-sm font-medium active:opacity-80 transition-opacity disabled:opacity-50"
                :disabled="installing || !installUrl.trim()"
                @click="handleInstallUrl"
              >
                {{ installing ? $t('mobile.plugin.installing') : $t('mobile.plugin.dialog.confirm') }}
              </button>
            </div>

            <button
              class="mt-4 w-full py-2 text-sm text-[var(--mobile-text-muted)] active:opacity-80 transition-opacity"
              :disabled="installing"
              @click="closeInstallSheet()"
            >
              {{ $t('mobile.plugin.dialog.cancel') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 卸载确认 -->
    <ConfirmDialog
      v-model="showUninstallConfirm"
      :title="$t('mobile.plugin.uninstallConfirmTitle')"
      :message="uninstallTarget ? $t('mobile.plugin.uninstallConfirmMessage', { name: uninstallTarget.name }) : ''"
      :confirm-text="$t('mobile.plugin.dialog.confirm')"
      :cancel-text="$t('mobile.plugin.dialog.cancel')"
      variant="danger"
      :loading="installing"
      @confirm="confirmUninstall"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * PluginView - 插件管理页面
 *
 * 独立页面，从设置页跳转进入
 * 展示已安装插件列表（状态徽章/来源）、启用/禁用、详情展开、
 * 安装（文件/URL）与卸载（仅用户安装的插件）
 */
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import Toggle from '@/components/Toggle.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  pluginListLoaded,
  pluginSetEnabled,
  pluginIsEnabled,
  pluginInstallFromFile,
  pluginDownload,
  pluginUninstall,
} from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import type { PluginInfo, PluginState } from '@/plugin/types'

const router = useRouter()
const { t } = useI18n()
const toast = useToast()

const plugins = ref<PluginInfo[]>([])
const pluginEnabledStates = ref<Record<string, boolean>>({})
const expandedPlugin = ref<string | null>(null)
const showInstallSheet = ref(false)
const installUrl = ref('')
const installing = ref(false)
const uninstallTarget = ref<PluginInfo | null>(null)
const showUninstallConfirm = ref(false)

onMounted(loadPlugins)

/** 加载插件列表与启用状态 */
async function loadPlugins(): Promise<void> {
  try {
    plugins.value = await pluginListLoaded()
    const states: Record<string, boolean> = {}
    for (const p of plugins.value) {
      states[p.id] = await pluginIsEnabled(p.id)
    }
    pluginEnabledStates.value = states
  } catch {
    toast.error(t('mobile.plugin.loadFailed'))
  }
}

/** 切换启用/停用：持久化偏好 + 联动激活/停用 */
async function handlePluginToggle(pluginId: string, enabled: boolean): Promise<void> {
  try {
    await pluginSetEnabled(pluginId, enabled)
    if (enabled) {
      await pluginLoader.activate(pluginId)
    } else {
      await pluginLoader.deactivate(pluginId)
    }
    await loadPlugins()
  } catch (e: any) {
    toast.error(t(enabled ? 'mobile.plugin.activateFailed' : 'mobile.plugin.deactivateFailed', { error: e.message || String(e) }))
    // 恢复开关状态
    pluginEnabledStates.value[pluginId] = !enabled
  }
}

/** 从文件安装：文件选择器选 zip 插件包 */
async function handleInstallFile(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Plugin', extensions: ['zip'] }],
    })
    if (typeof selected !== 'string') return
    installing.value = true
    const pluginId = await pluginInstallFromFile(selected)
    toast.success(t('mobile.plugin.installSuccess', { name: pluginId }))
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.installFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    closeInstallSheet()
  }
}

/** 从 URL 安装：下载 zip 插件包 */
async function handleInstallUrl(): Promise<void> {
  const url = installUrl.value.trim()
  if (!url) return
  installing.value = true
  try {
    const pluginId = await pluginDownload(url)
    toast.success(t('mobile.plugin.installSuccess', { name: pluginId }))
    installUrl.value = ''
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.installFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    closeInstallSheet()
  }
}

/** 请求卸载（弹确认框） */
function requestUninstall(plugin: PluginInfo): void {
  uninstallTarget.value = plugin
  showUninstallConfirm.value = true
}

/** 确认卸载 */
async function confirmUninstall(): Promise<void> {
  const plugin = uninstallTarget.value
  if (!plugin) return
  installing.value = true
  try {
    await pluginUninstall(plugin.id)
    toast.success(t('mobile.plugin.uninstallSuccess', { name: plugin.name }))
    expandedPlugin.value = null
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.uninstallFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    uninstallTarget.value = null
    showUninstallConfirm.value = false
  }
}

function closeInstallSheet(): void {
  if (!installing.value) {
    showInstallSheet.value = false
    installUrl.value = ''
  }
}

// ==================== 展示辅助 ====================

/** 状态徽章样式 */
function stateBadgeClass(state: PluginState): string {
  if (isErrorState(state)) {
    return 'bg-[var(--mobile-danger-bg)] text-[var(--mobile-danger-color)]'
  }
  if (state.state === 'Activated') {
    return 'bg-[var(--mobile-success-muted)] text-[var(--mobile-success)]'
  }
  return 'bg-[var(--mobile-input-bg)] text-[var(--mobile-text-muted)]'
}

/** 状态文本 key */
function getStateKey(state: PluginState): string {
  if (state.state === 'Error') return 'mobile.plugin.stateError'
  if (state.state === 'Activated') return 'mobile.plugin.stateActivated'
  if (state.state === 'Deactivated') return 'mobile.plugin.stateDeactivated'
  return 'mobile.plugin.stateLoaded'
}

function isErrorState(state: PluginState): boolean {
  return state.state === 'Error'
}

function getErrorMessage(state: PluginState): string {
  return state.state === 'Error' ? state.error : ''
}

/** 来源标签 */
function getSourceLabel(source: string): string {
  return source === 'apk-asset'
    ? t('mobile.plugin.sourceBuiltin')
    : t('mobile.plugin.sourceInstalled')
}

function isBuiltin(source: string): boolean {
  return source === 'apk-asset'
}

/** 扩展点摘要 */
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
  max-height: 260px;
}
</style>
