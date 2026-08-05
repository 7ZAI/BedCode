<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+计数，右刷新 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ $t('desktop.plugin.title') }}</h1>
        <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ enabledPlugins.length }}/{{ plugins.length }} {{ $t('desktop.plugin.enabled') }}</span>
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
          <section v-if="enabledPlugins.length > 0" class="mb-6">
            <h2 class="wb-section-title">ENABLED · {{ enabledPlugins.length }}</h2>
            <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden">
              <div
                v-for="plugin in enabledPlugins"
                :key="plugin.id"
                class="px-4 py-3 flex items-center gap-3"
              >
                <span class="w-2 h-2 rounded-full bg-green-500 shrink-0"></span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate cursor-pointer hover:underline" @click="toggleExpand(plugin.id)">{{ plugin.name }}</span>
                    <span class="wb-mono text-[var(--text-tertiary)] shrink-0">v{{ plugin.version }}</span>
                  </div>
                  <div class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] truncate mt-0.5">{{ plugin.description }}</div>
                </div>
                <router-link
                  v-if="isActivated(plugin.state)"
                  :to="`/plugins/${plugin.id}/config`"
                  class="h-7 px-3 text-xs font-medium rounded-[6px] border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0 flex items-center"
                >
                  {{ $t('desktop.plugin.config') }}
                </router-link>
                <!-- 工作台风格开关：点击关闭插件；rust 插件始终启用不可关闭 -->
                <button
                  v-if="plugin.pluginType !== 'rust'"
                  class="relative w-10 h-5 rounded-full transition-colors shrink-0 bg-[var(--color-primary)]"
                  :class="{ 'opacity-50 cursor-not-allowed': togglingId === plugin.id }"
                  :disabled="togglingId === plugin.id"
                  @click="handleToggle(plugin.id, false)"
                >
                  <span class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full translate-x-5 transition-transform"></span>
                </button>
                <span v-else class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0">{{ $t('desktop.plugin.alwaysOn') }}</span>
              </div>
            </div>
          </section>

          <section v-if="disabledPlugins.length > 0">
            <h2 class="wb-section-title">DISABLED · {{ disabledPlugins.length }}</h2>
            <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden">
              <div
                v-for="plugin in disabledPlugins"
                :key="plugin.id"
                class="px-4 py-3 flex items-center gap-3"
              >
                <span class="w-2 h-2 rounded-full shrink-0" :class="isErrorState(plugin.state) ? 'bg-red-500' : 'bg-[var(--text-tertiary)]'"></span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate cursor-pointer hover:underline" @click="toggleExpand(plugin.id)">{{ plugin.name }}</span>
                    <span class="wb-mono text-[var(--text-tertiary)] shrink-0">v{{ plugin.version }}</span>
                    <span class="wb-mono text-[calc(11px*var(--ui-scale))] shrink-0" :class="isErrorState(plugin.state) ? 'text-red-600 dark:text-red-400' : 'text-[var(--text-tertiary)]'">
                      {{ isErrorState(plugin.state) ? getErrorMessage(plugin.state) : $t(getStateKey(plugin.state)) }}
                    </span>
                  </div>
                  <div class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] truncate mt-0.5">{{ plugin.description }}</div>
                </div>
                <!-- DISABLED 分区内插件均未激活，配置入口不可用 -->
                <span class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.config') }}</span>
                <button
                  v-if="plugin.pluginType !== 'rust'"
                  class="relative w-10 h-5 rounded-full transition-colors shrink-0 bg-[var(--border)]"
                  :class="{ 'opacity-50 cursor-not-allowed': togglingId === plugin.id }"
                  :disabled="togglingId === plugin.id"
                  @click="handleToggle(plugin.id, true)"
                >
                  <span class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full translate-x-0 transition-transform"></span>
                </button>
                <span v-else class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.alwaysOn') }}</span>
              </div>
            </div>
          </section>
        </template>

        <!-- ==================== 展开详情（点击插件名展开） ==================== -->
        <div
          v-if="expandedPlugin"
          class="mt-6 bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] p-4"
        >
          <div class="grid grid-cols-2 gap-4">
            <div class="space-y-3">
              <div>
                <div class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)] mb-1">ID</div>
                <div class="wb-mono text-[var(--text-primary)]">{{ expandedPlugin.id }}</div>
              </div>
              <div v-if="expandedPlugin.author">
                <div class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)] mb-1">Author</div>
                <div class="text-[calc(12px*var(--ui-scale))] text-[var(--text-primary)]">{{ expandedPlugin.author }}</div>
              </div>
              <div>
                <div class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)] mb-1">{{ $t('desktop.plugin.copyPath') }}</div>
                <div class="flex items-center gap-2">
                  <code class="wb-mono text-[var(--text-primary)] bg-[var(--bg-hover)] px-2 py-1 rounded-[6px] truncate max-w-[280px]">{{ expandedPlugin.extensionPath }}</code>
                  <button
                    @click="copyPath(expandedPlugin.extensionPath)"
                    class="text-[calc(12px*var(--ui-scale))] text-[var(--color-primary)] hover:underline shrink-0"
                  >
                    {{ $t('desktop.plugin.copyPath') }}
                  </button>
                </div>
              </div>
            </div>
            <div class="space-y-3">
              <div>
                <div class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)] mb-1">Permissions</div>
                <div class="flex flex-wrap gap-1">
                  <span
                    v-for="perm in expandedPlugin.permissions"
                    :key="perm"
                    class="wb-mono text-[calc(10.5px*var(--ui-scale))] px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)]"
                  >
                    {{ perm }}
                  </span>
                  <span v-if="expandedPlugin.permissions.length === 0" class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]">—</span>
                </div>
              </div>
              <div>
                <div class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)] mb-1">Contributes</div>
                <div class="text-[calc(12px*var(--ui-scale))] text-[var(--text-primary)]">{{ getContributesSummary(expandedPlugin) }}</div>
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
 * 插件视图 — 桌面端插件管理页面
 * Warm Workbench 风格：工具栏页头 + ENABLED/DISABLED 分区列表，展开详情保留
 */
import { computed, onMounted } from 'vue'
import { usePluginManager } from '@/composables/usePluginManager'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'

const {
  plugins,
  loading,
  expandedId,
  togglingId,
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

/** 已激活 → ENABLED 分区；其余（已加载/已停用/错误）→ DISABLED 分区 */
const enabledPlugins = computed(() => plugins.value.filter(p => isActivated(p.state)))
const disabledPlugins = computed(() => plugins.value.filter(p => !isActivated(p.state)))

/** 当前展开详情的插件 */
const expandedPlugin = computed(() => plugins.value.find(p => p.id === expandedId.value) ?? null)

/** 处理切换，失败时恢复 UI 状态由 composable 内部处理 */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  await togglePlugin(id, enable)
}

onMounted(() => {
  loadPlugins()
})
</script>
