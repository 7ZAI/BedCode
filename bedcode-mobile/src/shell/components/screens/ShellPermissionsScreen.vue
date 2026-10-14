<template>
  <div class="flex flex-col h-full min-h-0">
    <div class="flex items-center gap-2 px-2 pt-2 pb-1">
      <button
        type="button"
        class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.detail.back')"
        @click="nav.back()"
      >
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M15 6l-6 6 6 6" />
        </svg>
      </button>
      <span class="text-[var(--font-size-base)] font-semibold text-[var(--mobile-text-primary)]">
        {{ t('shell.settings.permissions') }}
      </span>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 待审批（需要用户动作，置顶） ==================== -->
      <!-- 审批是整单的（后端 plugin_approve：全部声明权限 + 内容哈希钉扎），
           因此这里不做逐项勾选；批不批是用户的判断，平台不替他预设 -->
      <template v-if="pendingApps.length > 0">
        <ShellSection :title="t('shell.permissions.pending')" :note="t('shell.permissions.pendingNote')" />
        <div class="px-4 pb-1 flex flex-col gap-2">
          <div
            v-for="app in pendingApps"
            :key="app.id"
            class="flex items-center gap-3 rounded-[12px] border px-3 py-3"
            :style="{
              background: 'var(--mobile-warning-muted)',
              borderColor: 'var(--mobile-warning)',
            }"
          >
            <ShellAppIcon :app="app" :size="34" />
            <div class="flex-1 min-w-0">
              <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate">
                {{ app.name }}
              </div>
              <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
                {{ t('shell.permissions.pendingBody', { count: app.permissions.length }) }}
              </div>
            </div>
            <ShellChip
              :label="t('shell.permissions.approveStart')"
              tone="accent"
              clickable
              @click="approveAndStart(app.id)"
            />
          </div>
        </div>
      </template>

      <template v-if="permissionRows.length > 0">
        <div v-for="row in permissionRows" :key="row.key">
          <ShellSection :title="row.title" :note="t('shell.apps.permissionCount', { count: row.apps.length })" />
          <div class="px-4">
            <ShellGroupCard>
              <button
                v-for="app in row.apps"
                :key="app.id"
                type="button"
                class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
                @click="nav.openDetail(app.id)"
              >
                <ShellAppIcon :app="app" :size="28" />
                <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
                  {{ app.name }}
                </span>
                <svg class="flex-shrink-0" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
                  <path d="M9 6l6 6-6 6" />
                </svg>
              </button>
            </ShellGroupCard>
          </div>
        </div>
      </template>

      <div v-else class="px-4 pt-6">
        <p class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-6 text-center text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)]">
          {{ t('mobile.plugin.noPermissions') }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 权限总览（按权限看应用）
 *
 * 与详情页的「按应用看权限」共用同一份权限授予真源，只是投影方向相反。
 * 不新增任何权限状态——反查视图如果自己维护一份状态，两处迟早不一致。
 * 待审批区是新增的：审批是权限管理里唯一需要用户动作的事，必须置顶。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '../../composables/useToast'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { permissionTitleKey } from '../../permissions'
import type { ShellApp } from '../../types'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

interface PermissionRow {
  key: string
  title: string
  apps: ShellApp[]
}

const { t } = useI18n()
const toast = useToast()
const nav = useShellNavigation()
const { apps, approveApp, launch } = useShellApps()

/** 待审批应用：审批入口的置顶分区（批准后状态变化，卡片自然消失） */
const pendingApps = computed(() => apps.value.filter((a) => a.state === 'approval'))

/**
 * 批准并启动
 *
 * 批准失败（后端拒绝）时不继续启动——半途而废的「已批准」假象比直接报错更危险。
 */
async function approveAndStart(appId: string): Promise<void> {
  const approved = await approveApp(appId)
  if (!approved) {
    toast.error(t('shell.permissions.approveFailed'))
    return
  }
  const started = await launch(appId)
  if (started) nav.openApp(appId)
}

/** 权限 → 持有该权限的应用（按权限词聚合，应用按名称排序） */
const permissionRows = computed<PermissionRow[]>(() => {
  const byKey = new Map<string, ShellApp[]>()
  for (const app of apps.value) {
    for (const grant of app.permissions) {
      const list = byKey.get(grant.key) ?? []
      list.push(app)
      byKey.set(grant.key, list)
    }
  }
  return [...byKey.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([key, list]) => {
      const i18nKey = permissionTitleKey(key)
      return {
        key,
        title: i18nKey ? `${t(i18nKey)} · ${key}` : key,
        apps: [...list].sort((x, y) => x.name.localeCompare(y.name, 'zh-Hans-CN')),
      }
    })
})
</script>
