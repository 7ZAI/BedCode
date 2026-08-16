<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左返回+名称，右版本号 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5 min-w-0">
        <button
          class="h-7 w-7 rounded-[6px] border border-[var(--border)] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0"
          :title="$t('desktop.plugin.backToList')"
          @click="goBack"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate">
          {{ plugin ? plugin.name : '...' }}
        </h1>
        <span v-if="plugin" class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0">v{{ plugin.version }}</span>
      </div>
    </div>

    <!-- ==================== 内容区 ==================== -->
    <div class="flex-1 overflow-auto p-5">
      <div class="max-w-3xl mx-auto">
        <!-- 加载态 -->
        <div v-if="loading" class="py-16 text-center">
          <div class="w-8 h-8 border-2 border-[var(--color-primary)] border-t-transparent rounded-full animate-spin mx-auto mb-3"></div>
          <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]">Loading...</p>
        </div>

        <!-- 插件未找到 -->
        <div v-else-if="!plugin" class="py-16 text-center">
          <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">Plugin not found</p>
          <button class="mt-3 text-[calc(12px*var(--ui-scale))] text-[var(--color-primary)] hover:underline" @click="goBack">
            {{ $t('desktop.plugin.backToList') }}
          </button>
        </div>

        <template v-else>
          <!-- ==================== Hero：图标 + 名称 + 作者·版本 + 状态徽章 + 操作列 ==================== -->
          <div class="flex items-center gap-4 pb-5 border-b border-[var(--border)]">
            <PluginIcon
              :icon="plugin.icon"
              :name="plugin.name"
              :plugin-id="plugin.id"
              :extension-path="plugin.extensionPath"
              size="lg"
            />
            <div class="flex-1 min-w-0">
              <h2 class="text-[calc(16px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate">{{ plugin.name }}</h2>
              <p class="text-[calc(12px*var(--ui-scale))] mt-0.5 text-[var(--text-secondary)]">
                {{ plugin.author || '—' }} · v{{ plugin.version }}
              </p>
              <span
                class="inline-flex items-center gap-1.5 mt-2 px-2 py-0.5 rounded-md text-[calc(11px*var(--ui-scale))] font-medium"
                :class="stateBadgeClass(plugin.state)"
              >
                <span v-if="isActivated(plugin.state)" class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
                {{ $t(getStateKey(plugin.state)) }}
              </span>
            </div>
            <!-- 操作列：启停 + 配置，上下并排 -->
            <div class="flex flex-col gap-2 shrink-0">
              <!-- 启停按钮 -->
              <button
                v-if="plugin.pluginType !== 'rust'"
                class="w-[76px] h-8 rounded-[6px] text-[calc(12px*var(--ui-scale))] font-medium transition-colors flex items-center justify-center"
                :class="isActivated(plugin.state)
                  ? 'bg-red-50 dark:bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-500/20'
                  : 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)] hover:opacity-90'"
                :disabled="!!togglingId"
                @click="handleToggle(plugin.id, !isActivated(plugin.state))"
              >
                {{ isActivated(plugin.state) ? $t('desktop.plugin.disable') : $t('desktop.plugin.enabled') }}
              </button>
              <span v-else class="w-[76px] h-8 rounded-[6px] text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] bg-[var(--bg-hover)] flex items-center justify-center">
                {{ $t('desktop.plugin.alwaysOn') }}
              </span>

              <!-- 配置按钮（仅激活 + 有 configuration 时可点） -->
              <router-link
                v-if="hasConfiguration(plugin)"
                :to="`/plugins/${plugin.id}/config`"
                class="w-[76px] h-8 rounded-[6px] border border-[var(--border)] text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors flex items-center justify-center"
              >
                {{ $t('desktop.plugin.goConfig') }}
              </router-link>
              <span v-else class="w-[76px] h-8 rounded-[6px] border border-[var(--border)] text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] flex items-center justify-center cursor-not-allowed">
                {{ $t('desktop.plugin.goConfig') }}
              </span>
            </div>
          </div>

          <!-- ==================== 统计条 ==================== -->
          <div class="grid grid-cols-3 gap-0 border-b border-[var(--border)]">
            <div class="py-3 text-center border-r border-[var(--border)]">
              <div class="text-[calc(14px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ getContributionChips(plugin).length }}</div>
              <div class="text-[calc(11px*var(--ui-scale))] mt-0.5 text-[var(--text-tertiary)]">{{ $t('desktop.plugin.stat.extensions') }}</div>
            </div>
            <div class="py-3 text-center border-r border-[var(--border)]">
              <div class="text-[calc(14px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ plugin.permissions.length }}</div>
              <div class="text-[calc(11px*var(--ui-scale))] mt-0.5 text-[var(--text-tertiary)]">{{ $t('desktop.plugin.stat.permissions') }}</div>
            </div>
            <div class="py-3 text-center">
              <div class="text-[calc(14px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ formatBytes(plugin.sizeBytes) }}</div>
              <div class="text-[calc(11px*var(--ui-scale))] mt-0.5 text-[var(--text-tertiary)]">{{ $t('desktop.plugin.stat.size') }}</div>
            </div>
          </div>

          <!-- ==================== 折叠区域 ==================== -->
          <div class="mt-4 bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-4 divide-y-0">
            <!-- 简介（默认展开） -->
            <CollapseSection :title="$t('desktop.plugin.section.intro')" emoji="📄" :default-open="true">
              <p class="px-1 pb-4 text-[calc(12px*var(--ui-scale))] leading-relaxed text-[var(--text-secondary)]">
                {{ plugin.description || $t('desktop.plugin.noDescription') }}
              </p>
            </CollapseSection>

            <!-- 扩展点（默认折叠） -->
            <CollapseSection :title="$t('desktop.plugin.section.contributes')" emoji="🧩" :badge="getContributionChips(plugin).length" :default-open="false">
              <div class="px-1 pb-4 flex flex-wrap gap-2">
                <span
                  v-for="chip in getContributionChips(plugin)"
                  :key="chip.key"
                  class="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[calc(11px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-secondary)]"
                >
                  {{ chip.emoji }} {{ $t(chip.labelKey, chip.params ?? {}) }}
                </span>
                <span v-if="getContributionChips(plugin).length === 0" class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]">—</span>
              </div>
            </CollapseSection>

            <!-- 权限（默认折叠） -->
            <CollapseSection :title="$t('desktop.plugin.section.permissions')" emoji="🛡️" :badge="plugin.permissions.length" :default-open="false">
              <div class="px-1 pb-3 divide-y divide-[var(--border)]">
                <div v-for="perm in plugin.permissions" :key="perm" class="flex items-center gap-3 py-2.5">
                  <span class="w-4 h-4 flex items-center justify-center text-xs shrink-0">{{ getPermissionMeta(perm).emoji }}</span>
                  <div class="flex-1 min-w-0">
                    <div class="text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)]">{{ getPermissionMeta(perm).title }}</div>
                    <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ getPermissionMeta(perm).desc }}</div>
                  </div>
                  <span class="wb-mono text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0">{{ perm }}</span>
                </div>
                <div v-if="plugin.permissions.length === 0" class="py-2.5 text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]">—</div>
              </div>
            </CollapseSection>

            <!-- 详细信息（默认折叠） -->
            <CollapseSection :title="$t('desktop.plugin.section.details')" emoji="ℹ️" :default-open="false">
              <div class="px-1 pb-3 text-[calc(12px*var(--ui-scale))]">
                <div
                  v-for="row in getDetailRows(plugin)"
                  :key="row.key"
                  class="grid grid-cols-[80px_1fr] py-2 border-b border-[var(--border)] last:border-b-0"
                >
                  <span class="text-[var(--text-tertiary)]">{{ row.label }}</span>
                  <span class="break-all text-[var(--text-secondary)]" :class="row.mono ? 'wb-mono' : ''">{{ row.value }}</span>
                </div>
                <!-- 扩展路径 + 复制 -->
                <div class="grid grid-cols-[80px_1fr] py-2">
                  <span class="text-[var(--text-tertiary)]">{{ $t('desktop.plugin.copyPath') }}</span>
                  <div class="flex items-center gap-2 min-w-0">
                    <code class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] bg-[var(--bg-hover)] px-2 py-1 rounded-[4px] truncate">{{ plugin.extensionPath }}</code>
                    <button
                      class="text-[calc(11px*var(--ui-scale))] text-[var(--color-primary)] hover:underline shrink-0"
                      @click="copyPath(plugin.extensionPath)"
                    >
                      {{ $t('desktop.plugin.copyPath') }}
                    </button>
                  </div>
                </div>
              </div>
            </CollapseSection>
          </div>
        </template>
      </div>
    </div>

    <!-- ==================== 启停遮罩弹窗 ==================== -->
    <Teleport to="body">
      <Transition name="overlay">
        <div
          v-if="togglingPluginInfo"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
        >
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-xl px-8 py-6 shadow-xl flex flex-col items-center gap-4">
            <div class="w-8 h-8 border-3 border-[var(--color-primary)] border-t-transparent rounded-full animate-spin"></div>
            <p class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">{{ togglingPluginInfo.message }}</p>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginDetailView - 插件详情页
 *
 * Hero + 操作行 + 统计条 + 四折叠区（简介/扩展点/权限/详细信息）。
 * 从插件列表进入，返回直接回列表页。
 */
import { ref, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { pluginListLoaded } from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import PluginIcon from '@/components/PluginIcon.vue'
import CollapseSection from '@/components/CollapseSection.vue'
import {
  getContributionChips,
  getPermissionMeta,
  getDetailRows,
  getStateKey,
  isActivated,
  isErrorState,
  getErrorMessage,
  hasConfiguration,
  formatBytes,
} from '@/plugin/contributionKinds'
import { pluginLoader } from '@/plugin/loader'
import type { PluginInfo, PluginState } from '@/plugin/types'

const route = useRoute()
const router = useRouter()
const toast = useToast()
const t = i18n.global.t

const plugin = ref<PluginInfo | null>(null)
const loading = ref(true)
const togglingId = ref<string | null>(null)
const togglingPluginInfo = ref<{ id: string; name: string; message: string } | null>(null)

const TOGGLE_TIMEOUT_MS = 30000

/** 加载插件信息 */
async function loadPlugin(): Promise<void> {
  loading.value = true
  try {
    const pluginId = route.params.id as string
    const list = await pluginListLoaded()
    const found = list.find(p => p.id === pluginId)
    if (!found) {
      plugin.value = null
    } else {
      plugin.value = found
    }
  } catch {
    toast.error(t('desktop.plugin.loadFailed'))
    plugin.value = null
  } finally {
    loading.value = false
  }
}

/** 返回列表页 */
function goBack(): void {
  router.push({ path: '/plugins', query: { ...route.query } })
}

/** 状态徽章样式 */
function stateBadgeClass(state: PluginState): string {
  if (isErrorState(state)) return 'bg-red-50 dark:bg-red-500/10 text-red-600 dark:text-red-400'
  if (isActivated(state)) return 'bg-green-50 dark:bg-green-500/10 text-green-600 dark:text-green-400'
  return 'bg-[var(--bg-hover)] text-[var(--text-tertiary)]'
}

/** 切换启停（带遮罩） */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  if (togglingId.value) return
  togglingId.value = id
  const name = plugin.value?.name || id
  const key = enable ? 'desktop.plugin.togglingEnable' : 'desktop.plugin.togglingDisable'
  togglingPluginInfo.value = { id, name, message: t(key, { name }) }

  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    const op = enable ? pluginLoader.activate(id) : pluginLoader.deactivate(id)
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(t('desktop.plugin.toggleTimeout'))), TOGGLE_TIMEOUT_MS)
    })
    await Promise.race([op, timeout])
    await loadPlugin()
    const resultKey = enable ? 'desktop.plugin.enabledSuccess' : 'desktop.plugin.disabledSuccess'
    toast.success(t(resultKey, { name }))
  } catch (e: any) {
    const errKey = enable ? 'desktop.plugin.activateFailed' : 'desktop.plugin.deactivateFailed'
    toast.error(t(errKey, { error: e.message || 'Unknown error' }))
  } finally {
    clearTimeout(timer)
    togglingId.value = null
    togglingPluginInfo.value = null
  }
}

/** 复制路径 */
async function copyPath(path: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(path)
    toast.success(t('desktop.plugin.pathCopied'))
  } catch {
    toast.error(t('desktop.plugin.copyFailed'))
  }
}

onMounted(() => { loadPlugin() })

// 路由参数变化时重新加载（同路由不同 params）
watch(() => route.params.id, () => { loadPlugin() })
</script>

<style scoped>
.overlay-enter-active,
.overlay-leave-active {
  transition: opacity 0.2s ease;
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}
</style>
