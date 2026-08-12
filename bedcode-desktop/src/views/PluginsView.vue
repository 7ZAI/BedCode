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
                  <!-- 启停开关（v-bind 改造：CSS 变量驱动样式，状态切换集中在 <style>） -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="plugin-toggle"
                    :style="{
                      '--toggle-on': isActivated(plugin.state) ? 1 : 0,
                      '--toggle-locked': togglingId === plugin.id ? 1 : 0,
                    }"
                    :title="$t('desktop.plugin.disable')"
                    :aria-label="$t('desktop.plugin.disable')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, false)"
                  >
                    <span class="plugin-toggle__knob" />
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
                  <!-- 启停开关（v-bind 改造：同一组件，状态由 CSS 变量驱动） -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="plugin-toggle"
                    :style="{
                      '--toggle-on': isActivated(plugin.state) ? 1 : 0,
                      '--toggle-locked': togglingId === plugin.id ? 1 : 0,
                    }"
                    :title="$t('desktop.plugin.enabled')"
                    :aria-label="$t('desktop.plugin.enabled')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, true)"
                  >
                    <span class="plugin-toggle__knob" />
                  </button>
                  <span v-else class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center">{{ $t('desktop.plugin.alwaysOn') }}</span>
                </div>
              </div>
            </div>
          </section>
        </template>
      </div>
    </div>

    <!-- ==================== 启停遮罩弹窗（通用 LoadingOverlay 组件） ==================== -->
    <LoadingOverlay :visible="!!togglingPluginInfo" :message="togglingPluginInfo?.message" />
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
import LoadingOverlay from '@/components/LoadingOverlay.vue'
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

/* ==================== 启停开关 ==================== */
/*
 * v-bind 改造要点：
 * - 状态（on / locked）由父组件通过 :style 注入 CSS 变量（数值：0 或 1）
 * - 视觉表现（背景、边框、圆点位置、opacity）完全由 <style> 集中定义
 * - 切换 on 时只需改变 --toggle-on 数值，圆点 transform 自动位移（calc 参与）
 * - 锁定态（--toggle-locked: 1）通过 calc 乘 0.5 影响 opacity，零额外 DOM
 * - 子组件或子元素可通过 var(--toggle-on) 读取状态，无需 props 透传
 */
.plugin-toggle {
  /* 数值由父组件 :style 注入；这里给默认值保 SSR/初次渲染正确 */
  --toggle-on: 0;
  --toggle-locked: 0;
  --toggle-shift: 19;       /* 圆点位移：按钮 40 - 圆点 12 - 左右内边距 3*2 - 边框 1*2 = 19px */

  position: relative;
  width: 2.5rem;
  height: 1.25rem;
  border-radius: 4px;
  border: 1px solid;
  flex-shrink: 0;
  cursor: pointer;
  transition: background-color 0.2s, border-color 0.2s, opacity 0.2s;

  background: var(--color-primary);
  border-color: var(--color-primary);
  /* 锁定 1 时 opacity 0.5，0 时 1；calc(1 - var(--toggle-locked) * 0.5) */
  opacity: calc(1 - var(--toggle-locked) * 0.5);
}

/* 关闭态：覆盖 background / border */
.plugin-toggle[style*="--toggle-on: 0"] {
  background: var(--bg-page);
  border-color: var(--border-strong);
}

.plugin-toggle__knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 2px;
  background: var(--color-primary-contrast);
  transition: transform 0.2s, background-color 0.2s;
  /* calc 直接用 --toggle-on（数字），不用字符串 */
  transform: translateX(calc(var(--toggle-on) * var(--toggle-shift) * 1px));
}

.plugin-toggle[style*="--toggle-on: 0"] .plugin-toggle__knob {
  background: var(--border-strong);
}
</style>
