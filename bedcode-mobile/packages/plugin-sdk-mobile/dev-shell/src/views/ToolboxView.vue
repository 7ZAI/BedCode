<script setup lang="ts">
/**
 * ToolboxView — 已注册运行面的应用入口列表
 *
 * 票 2026-10-10 批次 C2：`registerToolboxPage` 退役后，dev-shell 的应用入口
 * 改由 `registerSurface` 一处提供——与宿主壳「只认 surface 一种运行面形态」同口径。
 * 点击打开应用运行面（activeView → PluginView 渲染）。
 */
import { useI18n } from 'vue-i18n'
import { openActiveView, surfaces } from '../registry'

const { t } = useI18n()

function openSurface(pluginId: string) {
  const entry = surfaces.value.find((s) => s.pluginId === pluginId)
  if (!entry) return
  openActiveView({
    kind: 'surface',
    pluginId,
    title: pluginId,
    component: entry.surface.component,
    // header: false — 运行面自持完整界面（含自己的头部/底部导航），
    // 由 AppShell 全局页头统一提供 back + 标题，避免 PluginView 再渲染一个页头造成重复
    header: false,
  })
}
</script>

<template>
  <div class="px-4 py-3">
    <!-- 空态：无任何应用注册运行面时展示 -->
    <div v-if="surfaces.length === 0" class="py-16 flex flex-col items-center gap-2 text-center">
      <span class="text-3xl">🧰</span>
      <p class="text-sm text-[var(--mobile-text-secondary)]">{{ t('devshell.toolbox.empty') }}</p>
      <p class="text-xs text-[var(--mobile-text-muted)] px-8">{{ t('devshell.toolbox.emptyHint') }}</p>
    </div>

    <div v-else class="space-y-3">
      <button
        v-for="entry in surfaces"
        :key="entry.pluginId"
        class="w-full flex items-center gap-3 p-4 text-left rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] cursor-pointer transition-[border-color,opacity] duration-300 hover:border-[var(--mobile-border-hover)] active:opacity-90"
        @click="openSurface(entry.pluginId)"
      >
        <span class="icon-chip chip-cyan flex-shrink-0">
          <span class="text-xl">🧩</span>
        </span>
        <span class="flex-1 min-w-0">
          <span class="block text-sm font-medium text-[var(--mobile-text-primary)] truncate">
            {{ entry.pluginId }}
          </span>
          <span class="block mt-0.5 text-xs text-[var(--mobile-text-muted)] truncate">
            {{ t('devshell.toolbox.surface') }}
          </span>
        </span>
        <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </div>
  </div>
</template>
