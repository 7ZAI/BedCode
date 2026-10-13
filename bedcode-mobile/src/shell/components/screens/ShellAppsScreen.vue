<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 页头 ==================== -->
    <header class="px-4 pt-3 pb-2">
      <h1 class="text-[var(--font-size-xl)] font-semibold text-[var(--mobile-text-primary)]">
        {{ t('shell.apps.title') }}
      </h1>
      <p class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.apps.subtitle', { installed: stats.installed, running: stats.running }) }}
      </p>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 统计条 ==================== -->
      <div class="px-4 pb-1">
        <div
          class="flex rounded-[12px] border border-[var(--mobile-group-border)] bg-[var(--mobile-bg-card)] py-3.5"
          :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
        >
          <div class="flex-1 text-center">
            <div class="text-[var(--font-size-lg)] font-bold text-[var(--mobile-text-primary)]">
              {{ stats.installed }}
            </div>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
              {{ t('shell.apps.statInstalled') }}
            </div>
          </div>
          <div
            class="flex-1 text-center border-l border-r border-[var(--mobile-group-divider)]"
          >
            <div class="text-[var(--font-size-lg)] font-bold" :style="{ color: 'var(--mobile-success)' }">
              {{ stats.running }}
            </div>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
              {{ t('shell.apps.statRunning') }}
            </div>
          </div>
          <div class="flex-1 text-center">
            <div class="text-[var(--font-size-lg)] font-bold text-[var(--mobile-text-primary)]">
              {{ totalSizeLabel }}
            </div>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
              {{ t('shell.apps.statSize') }}
            </div>
          </div>
        </div>
      </div>

      <!-- ==================== 已安装列表 ==================== -->
      <ShellSection :title="t('shell.apps.installedTitle')" :note="t('shell.apps.installedNote')" />
      <div class="px-4 pb-1">
        <ShellGroupCard v-if="sortedApps.length > 0">
          <button
            v-for="app in sortedApps"
            :key="app.id"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="nav.openDetail(app.id)"
          >
            <ShellAppIcon :app="app" :size="40" />
            <span class="flex-1 min-w-0">
              <span class="flex items-center gap-1.5">
                <b class="text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">{{ app.name }}</b>
                <ShellChip v-if="app.official" :label="t('shell.detail.official')" tone="accent" />
                <ShellChip
                  v-if="app.state === 'running'"
                  :label="t('shell.common.running')"
                  tone="success"
                />
                <ShellChip
                  v-else-if="app.state === 'disabled'"
                  :label="t('shell.common.disabled')"
                  tone="warn"
                />
                <ShellChip
                  v-else-if="app.state === 'error'"
                  :label="t('shell.common.error')"
                  tone="danger"
                />
              </span>
              <span class="block text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
                {{ appMeta(app) }}
              </span>
            </span>
            <svg class="flex-shrink-0" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path d="M9 6l6 6-6 6" />
            </svg>
          </button>
        </ShellGroupCard>
        <p
          v-else
          class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-5 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.apps.empty') }}
        </p>
      </div>

      <!-- ==================== 添加应用（按数据源能力渲染） ==================== -->
      <ShellSection v-if="canInstall" :title="t('shell.apps.addTitle')" />
      <div v-if="canInstall" class="px-4 pb-2">
        <ShellGroupCard>
          <button
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            :disabled="installing"
            @click="handleInstall"
          >
            <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path d="M12 3v12" />
              <path d="M7 10l5 5 5-5" />
              <path d="M4 21h16" />
            </svg>
            <span class="flex-1 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ t('shell.apps.installLocal') }}
            </span>
            <span class="flex-shrink-0 text-[var(--font-size-xs)]" :style="{ color: 'var(--mobile-warning)' }">
              {{ installing ? t('shell.common.loading') : t('shell.apps.installLocalHint') }}
            </span>
          </button>
        </ShellGroupCard>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * WASM 应用管理
 *
 * 列表只展示平台掌握的事实（版本 / 权限项数 / 数据占用），不猜测应用内部状态。
 * 「添加应用」按数据源能力渲染：没有安装能力的形态不会出现点了没反应的按钮。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '../../composables/useToast'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import type { ShellApp } from '../../types'
import { formatBytes } from '../../utils'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

const { t } = useI18n()
const toast = useToast()
const nav = useShellNavigation()
const { sortedApps, stats, supportsInstall, installFromLocalPackage, refresh } = useShellApps()

const installing = ref(false)

/**
 * 安装能力是「是否有数据源支持」的派生值，做成 computed 而不是在模板里
 * 直接放函数引用——放函数引用的 v-if 恒为真（函数本身永远存在）。
 */
const canInstall = computed(() => supportsInstall())

/** 总占用：任一应用未统计时显示「—」，不把缺失值当 0 累加成假数字 */
const totalSizeLabel = computed(() =>
  stats.value.totalBytes === undefined ? t('shell.common.na') : formatBytes(stats.value.totalBytes),
)

/** 列表副标题：版本 · 权限项数 · 数据占用 */
function appMeta(app: ShellApp): string {
  const parts = [app.version ? `v${app.version}` : '']
  parts.push(t('shell.apps.permissionCount', { count: app.permissions.length }))
  parts.push(formatBytes(app.sizeBytes))
  return parts.filter(Boolean).join(' · ')
}

async function handleInstall(): Promise<void> {
  installing.value = true
  try {
    const ok = await installFromLocalPackage()
    // 取消选包不是失败，不弹错误
    if (ok) {
      toast.success(t('shell.apps.installSuccess'))
      await refresh()
    }
  } catch (e) {
    toast.error(t('shell.apps.installFailed', { error: e instanceof Error ? e.message : String(e) }))
  } finally {
    installing.value = false
  }
}
</script>
