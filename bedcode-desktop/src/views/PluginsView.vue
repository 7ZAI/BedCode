<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center justify-between">
      <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">{{ $t('desktop.plugin.title') }}</h2>
      <button
        @click="loadPlugins()"
        :disabled="loading"
        class="flex items-center gap-1.5 px-3 py-1.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] rounded-btn transition-all duration-200 disabled:opacity-50"
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
        <div v-else class="bg-card rounded-card shadow-card overflow-hidden animate-fade-slide-up">
          <!-- Table Header -->
          <div class="grid grid-cols-[2fr_80px_80px_72px_56px] gap-2 px-6 py-3 bg-[var(--bg-hover)]/50 text-xs font-semibold text-[var(--text-secondary)] border-b border-[var(--border)] items-center">
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
              class="grid grid-cols-[2fr_80px_80px_72px_56px] gap-2 px-6 py-3.5 text-sm items-center cursor-pointer transition-all duration-200 border-b border-[var(--border)] last:border-b-0"
              :class="[
                isErrorState(plugin.state) ? 'bg-[var(--color-danger-light)]' : '',
                expandedId === plugin.id ? 'bg-brand-light/30' : 'hover:bg-[var(--bg-hover)]'
              ]"
              @click="toggleExpand(plugin.id)"
            >
              <!-- Plugin Name + Description -->
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <svg class="w-3 h-3 text-[var(--text-tertiary)] shrink-0 transition-transform" :class="{ 'rotate-90': expandedId === plugin.id }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                  <span class="font-medium truncate text-[var(--text-primary)]">
                    {{ plugin.name }}
                  </span>
                </div>
                <div class="text-xs mt-0.5 pl-4.5 truncate text-[var(--text-secondary)]">
                  <template v-if="isErrorState(plugin.state)">
                    ⚠ {{ getErrorMessage(plugin.state) }}
                  </template>
                  <template v-else>
                    {{ plugin.description }}
                  </template>
                </div>
              </div>

              <!-- Version -->
              <span class="text-[var(--text-secondary)] text-xs">{{ plugin.version }}</span>

              <!-- State Badge -->
              <span
                class="inline-flex items-center h-6 px-2.5 rounded-tag text-[11px] font-medium"
                :class="stateBadgeClass(plugin.state)"
              >
                {{ $t(getStateKey(plugin.state)) }}
              </span>

              <!-- Config Link -->
              <router-link
                v-if="isActivated(plugin.state)"
                :to="`/plugins/${plugin.id}/config`"
                class="text-xs text-brand hover:underline"
                @click.stop
              >
                {{ $t('desktop.plugin.config') }}
              </router-link>
              <span v-else class="text-xs text-[var(--text-tertiary)] cursor-default">
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
              class="px-6 py-3 bg-[var(--bg-hover)]/30 border-b border-[var(--border)]"
            >
              <div class="grid grid-cols-2 gap-4">
                <!-- Left Column -->
                <div class="space-y-3">
                  <div>
                    <div class="text-xs font-medium text-[var(--text-secondary)] mb-1">ID</div>
                    <div class="text-xs text-[var(--text-primary)] font-mono">{{ plugin.id }}</div>
                  </div>
                  <div v-if="plugin.author">
                    <div class="text-xs font-medium text-[var(--text-secondary)] mb-1">Author</div>
                    <div class="text-xs text-[var(--text-primary)]">{{ plugin.author }}</div>
                  </div>
                  <div>
                    <div class="text-xs font-medium text-[var(--text-secondary)] mb-1">{{ $t('desktop.plugin.copyPath') }}</div>
                    <div class="flex items-center gap-2">
                      <code class="text-xs text-[var(--text-primary)] bg-[var(--bg-hover)] px-2 py-1 rounded-input truncate max-w-[280px]">{{ plugin.extensionPath }}</code>
                      <button
                        @click="copyPath(plugin.extensionPath)"
                        class="text-xs text-brand hover:underline shrink-0"
                      >
                        {{ $t('desktop.plugin.copyPath') }}
                      </button>
                    </div>
                  </div>
                </div>
                <!-- Right Column -->
                <div class="space-y-3">
                  <div>
                    <div class="text-xs font-medium text-[var(--text-secondary)] mb-1">Permissions</div>
                    <div class="flex flex-wrap gap-1">
                      <span
                        v-for="perm in plugin.permissions"
                        :key="perm"
                        class="text-[10px] bg-indigo-100 dark:bg-indigo-900/30 text-indigo-700 dark:text-indigo-300 px-1.5 py-0.5 rounded"
                      >
                        {{ perm }}
                      </span>
                      <span v-if="plugin.permissions.length === 0" class="text-xs text-[var(--text-tertiary)]">—</span>
                    </div>
                  </div>
                  <div>
                    <div class="text-xs font-medium text-[var(--text-secondary)] mb-1">Contributes</div>
                    <div class="text-xs text-[var(--text-primary)]">{{ getContributesSummary(plugin) }}</div>
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
import Toggle from '@/components/Toggle.vue'
import EmptyState from '@/components/EmptyState.vue'
import { usePluginManager } from '@/composables/usePluginManager'
import type { PluginState } from '@/plugin/types'

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

/** 状态徽章样式 — pill tag */
function stateBadgeClass(state: PluginState): string {
  if (isActivated(state)) {
    return 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
  }
  if (isErrorState(state)) {
    return 'bg-[var(--color-danger-light)] text-red-600 dark:text-red-400'
  }
  return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
}

/** 处理切换，失败时恢复 UI 状态由 composable 内部处理 */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  await togglePlugin(id, enable)
}

onMounted(() => {
  loadPlugins()
})
</script>
