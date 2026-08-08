<script setup lang="ts">
/**
 * PluginsView — 插件管理（骨架）：状态徽章、激活/停用、设置区、路由、
 * 终端工具栏、文件服务挂载一览（全部由 dev-shell mock 驱动）。
 */
import { useI18n } from 'vue-i18n'
import {
  mounts,
  openActiveView,
  plugins,
  routes,
  settingsSections,
  terminalToolbarItems,
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
  activated: 'bg-[var(--mobile-success-muted)] text-[var(--mobile-success)]',
  loaded: 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]',
  deactivated: 'bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-muted)]',
  error: 'bg-[var(--mobile-error-muted)] text-[var(--mobile-error)]',
}

function openSettings(pluginId: string, section: string) {
  const entry = settingsSections.value.find(
    (s) => s.pluginId === pluginId && s.section.section === section,
  )
  if (!entry) return
  openActiveView({
    kind: 'settings',
    pluginId,
    title: entry.section.section,
    component: entry.section.component,
  })
}

function openRoute(pluginId: string, routeId: string) {
  const entry = routes.value.find((r) => r.pluginId === pluginId && r.route.id === routeId)
  if (!entry) return
  openActiveView({
    kind: 'route',
    pluginId,
    title: entry.route.title,
    header: entry.route.header ?? true,
    component: entry.route.component,
  })
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
  <div class="p-4 space-y-4">
    <h2 class="text-base font-semibold text-[var(--mobile-text-primary)]">{{ t('devshell.plugins.title') }}</h2>

    <div v-if="plugins.length === 0" class="py-10 text-center text-sm text-[var(--mobile-text-muted)]">
      {{ t('devshell.toolbox.emptyHint') }}
    </div>

    <div v-for="record in plugins" :key="record.id" class="rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] p-4 space-y-3">
      <!-- 头部：图标 + 名称 + 状态 -->
      <div class="flex items-center gap-3">
        <span v-if="isSvgIcon(record.manifest.icon)" class="w-8 h-8 flex items-center justify-center text-[var(--mobile-accent)]">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-8 h-8">
            <path :d="record.manifest.icon" />
          </svg>
        </span>
        <span v-else class="text-2xl leading-none">{{ record.manifest.icon || '🧩' }}</span>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-semibold text-[var(--mobile-text-primary)] truncate">{{ record.name }}</p>
          <p class="text-[11px] text-[var(--mobile-text-muted)] truncate">{{ record.id }} · v{{ record.manifest.version || '-' }}</p>
        </div>
        <span class="px-2 py-0.5 rounded-full text-[11px] flex-shrink-0" :class="stateClass[record.state]">
          {{ t(stateLabel[record.state]) }}
        </span>
      </div>

      <!-- 错误信息 -->
      <p v-if="record.state === 'error' && record.error" class="text-xs text-[var(--mobile-error)] bg-[var(--mobile-error-muted)] rounded-lg px-3 py-2 break-all">
        {{ record.error }}
      </p>

      <!-- 操作 -->
      <div class="flex items-center gap-2">
        <button
          class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors duration-200"
          :class="
            record.state === 'deactivated'
              ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
              : 'bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)]'
          "
          @click="toggle(record.id)"
        >
          {{ record.state === 'deactivated' ? t('devshell.plugins.activate') : t('devshell.plugins.deactivate') }}
        </button>
      </div>

      <!-- 注册项一览 -->
      <div class="space-y-2 text-xs">
        <div v-if="settingsSections.filter((s) => s.pluginId === record.id).length">
          <p class="text-[var(--mobile-text-muted)] mb-1">{{ t('devshell.plugins.settings') }}</p>
          <div class="flex flex-wrap gap-1.5">
            <button
              v-for="s in settingsSections.filter((x) => x.pluginId === record.id)"
              :key="s.section.section"
              class="px-2 py-1 rounded-md bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] transition-colors duration-200"
              @click="openSettings(record.id, s.section.section)"
            >
              {{ s.section.section }}
            </button>
          </div>
        </div>

        <div v-if="routes.filter((r) => r.pluginId === record.id).length">
          <p class="text-[var(--mobile-text-muted)] mb-1">{{ t('devshell.plugins.routes') }}</p>
          <div class="flex flex-wrap gap-1.5">
            <button
              v-for="r in routes.filter((x) => x.pluginId === record.id)"
              :key="r.route.id"
              class="px-2 py-1 rounded-md bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] transition-colors duration-200"
              @click="openRoute(record.id, r.route.id)"
            >
              /{{ r.route.id }}
            </button>
          </div>
        </div>

        <div v-if="terminalToolbarItems.filter((x) => x.pluginId === record.id).length">
          <p class="text-[var(--mobile-text-muted)] mb-1">{{ t('devshell.plugins.toolbar') }}</p>
          <div class="flex flex-wrap gap-1.5">
            <span
              v-for="item in terminalToolbarItems.filter((x) => x.pluginId === record.id)"
              :key="item.item.id"
              class="px-2 py-1 rounded-md bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)]"
            >
              {{ item.item.label }}
            </span>
          </div>
        </div>

        <div v-if="mounts.filter((m) => m.pluginId === record.id).length">
          <p class="text-[var(--mobile-text-muted)] mb-1">{{ t('devshell.plugins.mounts') }}</p>
          <div
            v-for="m in mounts.filter((x) => x.pluginId === record.id)"
            :key="m.mountPath"
            class="px-2 py-1.5 rounded-md bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] font-mono text-[11px] break-all"
          >
            {{ m.mountPath }} → [{{ m.roots.join(', ') }}] ({{ m.operations.join(', ') }})
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
