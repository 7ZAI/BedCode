<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+计数，右刷新 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ $t('desktop.plugin.title') }}</h1>
        <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ enabledPlugins.length }}/{{ plugins.length }} {{ $t('desktop.plugin.enabledSection') }}</span>
      </div>
      <button class="wb-btn-ghost" :disabled="loading" @click="loadPlugins()">
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        {{ $t('desktop.plugin.refresh') }}
      </button>
      <PluginPageToolbar target="plugins" />
    </div>

    <div class="flex-1 overflow-auto p-5">
      <div class="max-w-4xl mx-auto">
        <!-- ==================== 加载态：骨架 ==================== -->
        <div v-if="loading && plugins.length === 0" class="space-y-6">
          <div v-for="i in 2" :key="i">
            <div class="h-3 w-32 rounded animate-pulse bg-[var(--bg-hover)] mb-2"></div>
            <div class="h-16 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"></div>
          </div>
        </div>

        <!-- ==================== 空态 ==================== -->
        <div v-else-if="!loading && plugins.length === 0" class="py-16 text-center">
          <p class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">{{ $t('desktop.plugin.noPlugins') }}</p>
          <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] mt-1">{{ $t('desktop.plugin.noPluginsHint') }}</p>
        </div>

        <!-- ==================== ENABLED / DISABLED 分区 ==================== -->
        <template v-else>
          <!-- ENABLED 分区 -->
          <section v-if="enabledPlugins.length > 0" class="mb-6">
            <h2 class="wb-section-title">{{ $t('desktop.plugin.enabledSection') }} · {{ enabledPlugins.length }}</h2>
            <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden">
              <div
                v-for="plugin in enabledPlugins"
                :key="plugin.id"
                class="plugin-row"
              >
                <!-- 主体：图标 + 信息 + 操作 -->
                <div class="flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]" @click="goDetail(plugin.id)">
                  <PluginIcon
                    :icon="plugin.icon"
                    :name="plugin.name"
                    :plugin-id="plugin.id"
                    :extension-path="plugin.extensionPath"
                  />
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">{{ plugin.name }}</span>
                      <span class="wb-mono text-[var(--text-tertiary)] shrink-0">v{{ plugin.version }}</span>
                      <!-- 扩展点 chips -->
                      <span
                        v-for="chip in getContributionChips(plugin)"
                        :key="chip.key"
                        class="shrink-0 inline-flex items-center gap-0.5 px-1 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
                        :title="$t(chip.labelKey, chip.params ?? {})"
                      >
                        {{ chip.emoji }}
                      </span>
                    </div>
                    <!-- 简介折叠区 -->
                    <div class="mt-0.5">
                      <div
                        class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] transition-all duration-200"
                        :class="descExpanded[plugin.id] ? 'whitespace-pre-wrap' : 'truncate'"
                      >
                        {{ plugin.description || $t('desktop.plugin.noDescription') }}
                      </div>
                    </div>
                  </div>
                  <!-- 简介展开 chevron -->
                  <button
                    class="w-5 h-5 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
                    :title="$t('desktop.plugin.openDetail')"
                    @click.stop="descExpanded[plugin.id] = !descExpanded[plugin.id]"
                  >
                    <svg
                      class="w-3.5 h-3.5 transition-transform duration-200"
                      :class="{ 'rotate-90': descExpanded[plugin.id] }"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                    </svg>
                  </button>
                  <!-- 配置入口（仅激活 + 有配置时可点） -->
                  <router-link
                    v-if="hasConfiguration(plugin)"
                    :to="`/plugins/${plugin.id}/config`"
                    class="h-7 px-3 text-xs font-medium rounded-[6px] border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0 flex items-center"
                    @click.stop
                  >
                    {{ $t('desktop.plugin.goConfig') }}
                  </router-link>
                  <span v-else class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.config') }}</span>
                  <!-- 启停开关 -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="relative w-10 h-5 rounded-[4px] border transition-colors shrink-0 bg-[var(--color-primary)] border-[var(--color-primary)]"
                    :class="{ 'opacity-50 cursor-not-allowed': togglingId === plugin.id }"
                    :title="$t('desktop.plugin.disable')"
                    :aria-label="$t('desktop.plugin.disable')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, false)"
                  >
                    <span class="absolute top-[3px] left-[22px] w-3 h-3 rounded-[2px] bg-[var(--color-primary-contrast)] transition-all"></span>
                  </button>
                  <span v-else class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0">{{ $t('desktop.plugin.alwaysOn') }}</span>
                </div>
              </div>
            </div>
          </section>

          <!-- DISABLED 分区 -->
          <section v-if="disabledPlugins.length > 0">
            <h2 class="wb-section-title">{{ $t('desktop.plugin.disabledSection') }} · {{ disabledPlugins.length }}</h2>
            <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden">
              <div
                v-for="plugin in disabledPlugins"
                :key="plugin.id"
                class="plugin-row"
              >
                <div class="flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]" @click="goDetail(plugin.id)">
                  <PluginIcon
                    :icon="plugin.icon"
                    :name="plugin.name"
                    :plugin-id="plugin.id"
                    :extension-path="plugin.extensionPath"
                  />
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">{{ plugin.name }}</span>
                      <span class="wb-mono text-[var(--text-tertiary)] shrink-0">v{{ plugin.version }}</span>
                      <span class="wb-mono text-[calc(11px*var(--ui-scale))] shrink-0" :class="isErrorState(plugin.state) ? 'text-red-600 dark:text-red-400' : 'text-[var(--text-tertiary)]'">
                        {{ isErrorState(plugin.state) ? getErrorMessage(plugin.state) : $t(getStateKey(plugin.state)) }}
                      </span>
                    </div>
                    <div class="mt-0.5">
                      <div
                        class="text-[calc(12px*var(--ui-scale))] transition-all duration-200"
                        :class="[
                          descExpanded[plugin.id] ? 'whitespace-pre-wrap' : 'truncate',
                          isErrorState(plugin.state) ? 'text-red-600/70 dark:text-red-400/70' : 'text-[var(--text-secondary)]'
                        ]"
                      >
                        {{ isErrorState(plugin.state) ? ('⚠ ' + getErrorMessage(plugin.state)) : (plugin.description || $t('desktop.plugin.noDescription')) }}
                      </div>
                    </div>
                  </div>
                  <!-- 简介展开 chevron -->
                  <button
                    class="w-5 h-5 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
                    :title="$t('desktop.plugin.openDetail')"
                    @click.stop="descExpanded[plugin.id] = !descExpanded[plugin.id]"
                  >
                    <svg
                      class="w-3.5 h-3.5 transition-transform duration-200"
                      :class="{ 'rotate-90': descExpanded[plugin.id] }"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                    </svg>
                  </button>
                  <!-- 配置入口不可用 -->
                  <span class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.config') }}</span>
                  <!-- 启停开关 -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="relative w-10 h-5 rounded-[4px] border transition-colors shrink-0 bg-[var(--bg-page)] border-[var(--border-strong)]"
                    :class="{ 'opacity-50 cursor-not-allowed': togglingId === plugin.id }"
                    :title="$t('desktop.plugin.enabled')"
                    :aria-label="$t('desktop.plugin.enabled')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, true)"
                  >
                    <span class="absolute top-[3px] left-[3px] w-3 h-3 rounded-[2px] bg-[var(--border-strong)] transition-all"></span>
                  </button>
                  <span v-else class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.alwaysOn') }}</span>
                </div>
              </div>
            </div>
          </section>
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
 * PluginsView - 桌面端插件管理页面
 *
 * Warm Workbench 风格：工具栏页头 + ENABLED/DISABLED 分区列表。
 * 卡片主体点击进入详情页，chevron 展开简介折叠区。
 * 启停操作显示全页遮罩弹窗，防反复点击。
 */
import { computed, onMounted, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { usePluginManager } from '@/composables/usePluginManager'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import PluginIcon from '@/components/PluginIcon.vue'
import {
  getContributionChips,
  getStateKey,
  isActivated,
  isErrorState,
  getErrorMessage,
  hasConfiguration,
} from '@/plugin/contributionKinds'

const router = useRouter()
const {
  plugins,
  loading,
  togglingId,
  togglingPluginInfo,
  loadPlugins,
  togglePlugin,
} = usePluginManager()

/** 简介折叠状态（每个插件独立控制） */
const descExpanded = reactive<Record<string, boolean>>({})

/** 已激活 → ENABLED 分区；其余 → DISABLED 分区 */
const enabledPlugins = computed(() => plugins.value.filter(p => isActivated(p.state)))
const disabledPlugins = computed(() => plugins.value.filter(p => !isActivated(p.state)))

/** 跳转到插件详情页 */
function goDetail(pluginId: string): void {
  router.push({ name: 'plugin-detail', params: { id: pluginId } })
}

/** 处理切换 */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  await togglePlugin(id, enable)
}

onMounted(() => {
  loadPlugins()
})
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
