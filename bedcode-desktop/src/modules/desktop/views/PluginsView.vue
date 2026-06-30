<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-slate-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center justify-between shadow-sm dark:shadow-none">
      <h2 class="text-lg font-semibold">{{ $t('desktop.plugin.title') }}</h2>
      <button
        @click="loadPlugins()"
        :disabled="loading"
        class="flex items-center gap-1.5 px-3 py-1.5 text-sm text-slate-600 dark:text-dark-300 hover:bg-slate-100 dark:hover:bg-dark-700 rounded-lg transition-colors disabled:opacity-50"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        {{ $t('desktop.plugin.refresh') }}
      </button>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-3xl mx-auto">
        <!-- Empty State -->
        <EmptyState
          v-if="!loading && plugins.length === 0"
          :title="$t('desktop.plugin.noPlugins')"
          :description="$t('desktop.plugin.noPluginsHint')"
        />

        <!-- Plugin Table -->
        <div v-else class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 shadow-sm dark:shadow-none overflow-hidden">
          <!-- Table Header -->
          <div class="grid grid-cols-[2fr_80px_80px_72px_56px] gap-2 px-4 py-2 bg-slate-50 dark:bg-dark-700/50 text-xs font-semibold text-slate-500 dark:text-dark-400 border-b border-slate-200 dark:border-dark-700 items-center">
            <span>{{ $t('desktop.plugin.title') }}</span>
            <span>{{ $t('desktop.plugin.version') }}</span>
            <span>{{ $t('desktop.plugin.state') }}</span>
            <span></span>
            <span>{{ $t('desktop.plugin.enabled') }}</span>
          </div>

          <!-- Plugin Rows -->
          <div v-for="plugin in plugins" :key="plugin.id">
            <!-- Row -->
            <div
              class="grid grid-cols-[2fr_80px_80px_72px_56px] gap-2 px-4 py-3 text-sm items-center cursor-pointer transition-colors border-b border-slate-100 dark:border-dark-700/50 last:border-b-0"
              :class="[
                isErrorState(plugin.state) ? 'bg-red-50 dark:bg-red-900/10' : '',
                expandedId === plugin.id ? 'bg-indigo-50/50 dark:bg-indigo-900/10' : 'hover:bg-slate-50 dark:hover:bg-dark-700/30'
              ]"
              @click="toggleExpand(plugin.id)"
            >
              <!-- Plugin Name + Description -->
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <svg class="w-3 h-3 text-slate-400 dark:text-dark-500 shrink-0 transition-transform" :class="{ 'rotate-90': expandedId === plugin.id }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                  <span class="font-medium truncate" :class="isErrorState(plugin.state) ? 'text-red-700 dark:text-red-400' : 'text-slate-900 dark:text-white'">
                    {{ plugin.name }}
                  </span>
                </div>
                <div class="text-xs mt-0.5 pl-4.5 truncate" :class="isErrorState(plugin.state) ? 'text-red-600 dark:text-red-400' : 'text-slate-500 dark:text-dark-400'">
                  <template v-if="isErrorState(plugin.state)">
                    ⚠ {{ getErrorMessage(plugin.state) }}
                  </template>
                  <template v-else>
                    {{ plugin.description }}
                  </template>
                </div>
              </div>

              <!-- Version -->
              <span class="text-slate-500 dark:text-dark-400 text-xs">{{ plugin.version }}</span>

              <!-- State Badge -->
              <span
                class="text-xs px-2 py-0.5 rounded text-center"
                :class="stateBadgeClass(plugin.state)"
              >
                {{ $t(getStateKey(plugin.state)) }}
              </span>

              <!-- Config Link -->
              <router-link
                v-if="isActivated(plugin.state)"
                :to="`/plugins/${plugin.id}/config`"
                class="text-xs text-primary-600 dark:text-primary-400 hover:underline"
                @click.stop
              >
                {{ $t('desktop.plugin.config') }}
              </router-link>
              <span v-else class="text-xs text-slate-400 dark:text-dark-500 cursor-default">
                {{ $t('desktop.plugin.config') }}
              </span>

              <!-- Toggle -->
              <div class="flex justify-center" @click.stop>
                <Toggle
                  :modelValue="isActivated(plugin.state)"
                  @update:modelValue="(val: boolean) => handleToggle(plugin.id, val)"
                />
              </div>
            </div>

            <!-- Expanded Detail -->
            <div
              v-if="expandedId === plugin.id"
              class="px-4 py-3 bg-slate-50/50 dark:bg-dark-700/20 border-b border-slate-100 dark:border-dark-700/50"
            >
              <div class="grid grid-cols-2 gap-4">
                <!-- Left Column -->
                <div class="space-y-3">
                  <div>
                    <div class="text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">ID</div>
                    <div class="text-xs text-slate-600 dark:text-dark-300 font-mono">{{ plugin.id }}</div>
                  </div>
                  <div v-if="plugin.author">
                    <div class="text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">Author</div>
                    <div class="text-xs text-slate-600 dark:text-dark-300">{{ plugin.author }}</div>
                  </div>
                  <div>
                    <div class="text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">{{ $t('desktop.plugin.copyPath') }}</div>
                    <div class="flex items-center gap-2">
                      <code class="text-xs text-slate-600 dark:text-dark-300 bg-slate-100 dark:bg-dark-700 px-2 py-1 rounded truncate max-w-[280px]">{{ plugin.extensionPath }}</code>
                      <button
                        @click="copyPath(plugin.extensionPath)"
                        class="text-xs text-primary-600 dark:text-primary-400 hover:underline shrink-0"
                      >
                        {{ $t('desktop.plugin.copyPath') }}
                      </button>
                    </div>
                  </div>
                </div>
                <!-- Right Column -->
                <div class="space-y-3">
                  <div>
                    <div class="text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">Permissions</div>
                    <div class="flex flex-wrap gap-1">
                      <span
                        v-for="perm in plugin.permissions"
                        :key="perm"
                        class="text-[10px] bg-indigo-100 dark:bg-indigo-900/30 text-indigo-700 dark:text-indigo-300 px-1.5 py-0.5 rounded"
                      >
                        {{ perm }}
                      </span>
                      <span v-if="plugin.permissions.length === 0" class="text-xs text-slate-400">—</span>
                    </div>
                  </div>
                  <div>
                    <div class="text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">Contributes</div>
                    <div class="text-xs text-slate-600 dark:text-dark-300">{{ getContributesSummary(plugin) }}</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 插件视图 - 桌面端插件管理页面
 * 显示所有已安装插件，支持启用/停用切换和详情展开
 */
import { onMounted } from 'vue'
import Toggle from '@/modules/shared/components/Toggle.vue'
import EmptyState from '@/modules/shared/components/EmptyState.vue'
import { usePluginManager } from '@/modules/desktop/composables/usePluginManager'
import type { PluginState } from '@/modules/shared/plugin/types'

const {
  plugins,
  loading,
  expandedId,
  loadPlugins,
  togglePlugin,
  toggleExpand,
  copyPath,
  getStateKey,
  isActivated,
  isErrorState,
  getErrorMessage,
  getContributesSummary,
} = usePluginManager()

/** 状态徽章样式 */
function stateBadgeClass(state: PluginState): string {
  if (isActivated(state)) {
    return 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300'
  }
  if (isErrorState(state)) {
    return 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300'
  }
  return 'bg-slate-100 dark:bg-dark-600 text-slate-600 dark:text-dark-300'
}

/** 处理切换，失败时恢复 UI 状态由 composable 内部处理 */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  await togglePlugin(id, enable)
}

onMounted(() => {
  loadPlugins()
})
</script>
