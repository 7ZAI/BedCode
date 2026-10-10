<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 返回行 ==================== -->
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
        {{ t('shell.detail.title') }}
      </span>
    </div>

    <div v-if="app" class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== Hero ==================== -->
      <div class="flex items-center gap-3.5 px-4 pt-2 pb-4">
        <ShellAppIcon :app="app" :size="60" />
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <b class="text-[var(--font-size-xl)] text-[var(--mobile-text-primary)] truncate">{{ app.name }}</b>
            <ShellChip v-if="app.official" :label="t('shell.detail.official')" tone="accent" />
          </div>
          <div class="mt-0.5 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] truncate">
            {{ app.version ? t('shell.detail.version', { version: app.version }) : '' }}
            <span v-if="app.author"> · {{ app.author }}</span>
          </div>
          <div class="mt-1 flex items-center gap-1.5 text-[var(--font-size-xs)]">
            <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" :style="{ background: stateDot }" aria-hidden="true" />
            <span :style="{ color: 'var(--mobile-text-secondary)' }">{{ stateLabel }}</span>
            <span v-if="app.error" class="truncate" :style="{ color: 'var(--mobile-error)' }"> · {{ app.error }}</span>
          </div>
        </div>
      </div>

      <!-- ==================== 连接 ==================== -->
      <ShellSection :title="t('shell.detail.connection')" :note="t('shell.detail.connectionNote')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <div class="flex items-center gap-3 px-3 py-3">
            <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" :style="{ background: stateDot }" aria-hidden="true" />
            <div class="flex-1 min-w-0">
              <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">
                {{ app.name }}
              </div>
              <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
                {{ t('shell.detail.connectionNote') }}
              </div>
            </div>
            <ShellChip :label="stateLabel" :tone="stateTone" />
          </div>
        </ShellGroupCard>
      </div>

      <!-- ==================== 权限 ==================== -->
      <ShellSection :title="t('shell.detail.permissions')" :note="permissionsNote" />
      <div class="px-4 pb-1">
        <ShellGroupCard v-if="permissionGroups.length > 0">
          <template v-for="group in permissionGroups" :key="group.id">
            <div class="px-3 pt-2.5 pb-1 text-[var(--font-size-xs)] font-semibold text-[var(--mobile-text-muted)]">
              {{ t(group.titleKey) }}
            </div>
            <div
              v-for="grant in group.items"
              :key="grant.key"
              class="flex items-center gap-3 px-3 py-2.5 min-h-[var(--mobile-touch-target-min)]"
            >
              <div class="flex-1 min-w-0">
                <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">
                  {{ permissionTitle(grant.key) }}
                  <code class="ml-1.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">{{ grant.key }}</code>
                </div>
                <div v-if="grant.locked" class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
                  {{ t('shell.detail.permissionLocked') }}
                </div>
                <div v-else-if="grant.scope" class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
                  {{ grant.scope }}
                </div>
              </div>
              <!-- 锁定项：不给开关，只标徽标——隐藏会让用户误以为应用没有该能力 -->
              <ShellChip v-if="grant.locked" :label="t('shell.detail.permissionLocked')" tone="neutral" />
              <Toggle
                v-else
                :model-value="grant.granted"
                :disabled="!permissionControlEnabled"
                @update:model-value="(next: boolean) => handleToggle(grant.key, next)"
              />
            </div>
          </template>
        </ShellGroupCard>
        <p
          v-else
          class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-4 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.permissions.empty') }}
        </p>
        <!-- 能力缺失要显性说明：把开关置灰却不解释，用户会以为是应用的问题 -->
        <p
          v-if="!permissionControlEnabled"
          class="mt-2 px-1 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.detail.permissionUnsupported') }}
        </p>
        <p v-if="toggleError" class="mt-2 px-1 text-[var(--font-size-xs)]" :style="{ color: 'var(--mobile-error)' }">
          {{ toggleError }}
        </p>
      </div>

      <!-- ==================== 存储 ==================== -->
      <ShellSection :title="t('shell.detail.storage')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <div class="flex items-center px-3 py-3 min-h-[var(--mobile-touch-target-min)]">
            <span class="flex-1 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ t('shell.detail.appData') }}
            </span>
            <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)]">{{ formatBytes(app.sizeBytes) }}</span>
          </div>
          <div class="flex items-center px-3 py-3 min-h-[var(--mobile-touch-target-min)]">
            <span class="flex-1 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ t('shell.detail.cache') }}
            </span>
            <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)]">{{ t('shell.common.na') }}</span>
          </div>
        </ShellGroupCard>
      </div>

      <!-- ==================== 授权记录 ==================== -->
      <ShellSection :title="t('shell.detail.grants')" :note="t('shell.detail.grantsNote')" />
      <div class="px-4 pb-1">
        <p
          class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-4 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.detail.grantsEmpty') }}
        </p>
      </div>

      <!-- ==================== 操作 ==================== -->
      <div class="px-4 pt-3 flex flex-col gap-2.5">
        <button
          type="button"
          class="w-full rounded-[10px] border border-[var(--mobile-border)] px-4 py-3 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
          @click="openDemoPrompt"
        >
          {{ t('shell.detail.demoPrompt') }}
        </button>
        <button
          type="button"
          class="w-full rounded-[10px] border border-[var(--mobile-border)] px-4 py-3 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
          @click="handleStop"
        >
          {{ t('shell.detail.disable') }}
        </button>
        <button
          v-if="canRemove"
          type="button"
          class="w-full rounded-[10px] px-4 py-3 text-[var(--font-size-base)] transition-colors duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
          :style="{ background: 'var(--mobile-error-muted)', color: 'var(--mobile-error)' }"
          @click="confirmRemove = true"
        >
          {{ t('shell.detail.uninstall') }}
        </button>
      </div>
    </div>

    <!-- 应用已卸载/不存在：给出原因与出口，不留白屏 -->
    <div v-else class="flex-1 flex items-center justify-center px-8 text-center">
      <p class="text-[var(--font-size-base)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.run.notFound') }}
      </p>
    </div>

    <!-- 危险操作二次确认：复用平台通用 ConfirmDialog，不用原生对话框
         （原生 confirm 是系统弹窗，视觉与平台不一致，且不可本地化按钮）。
         放在 `v-if="app"` / `v-else` 配对之外，避免打断两者的相邻关系。 -->
    <ConfirmDialog
      :model-value="confirmRemove"
      variant="danger"
      :title="t('shell.detail.uninstall')"
      :message="app ? t('shell.apps.uninstallConfirm', { name: app.name }) : ''"
      :confirm-text="t('shell.detail.uninstall')"
      :cancel-text="t('shell.common.cancel')"
      @update:model-value="confirmRemove = $event"
      @confirm="handleRemove"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 应用详情与权限
 *
 * 权限开关是 UX，裁决在 Rust 端（AGENTS.md §8）。因此：
 *   · 后端不支持逐项授予时，开关置灰并显式说明，不做「点了就算通过」的假成功
 *   · 授权记录区没有后端真源，渲染空态而不是编造几条示例
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import ConfirmDialog from '../ui/ConfirmDialog.vue'
import Toggle from '../ui/Toggle.vue'
import { useToast } from '../../composables/useToast'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellOverlays } from '../../composables/useShellOverlays'
import { groupPermissions, permissionTitleKey } from '../../permissions'
import { formatBytes } from '../../utils'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

const { t } = useI18n()
const toast = useToast()
const nav = useShellNavigation()
const overlays = useShellOverlays()
const {
  getApp,
  supportsPermissionControl,
  setPermissionGrant,
  supportsRemove,
  removeApp,
  stop,
} = useShellApps()

/** 应用 id 来自导航参数（壳内导航不落 URL） */
const appId = computed(() => nav.params.value.appId ?? '')
const app = computed(() => getApp(appId.value))

const permissionControlEnabled = computed(() =>
  appId.value ? supportsPermissionControl(appId.value) : false,
)
const canRemove = computed(() => (appId.value ? supportsRemove(appId.value) : false))

/** 权限按能力域分组；未归类的新权限进「其他」组，保证不会因未归类而消失 */
const permissionGroups = computed(() =>
  groupPermissions(app.value?.permissions ?? [], 'shell.permission.group.other'),
)

const permissionsNote = computed(() => {
  const list = app.value?.permissions ?? []
  const granted = list.filter((p) => p.granted).length
  const locked = list.filter((p) => p.locked).length
  return t('shell.detail.permissionsNote', { granted, locked })
})

/** 权限标题：复用既有本地化文案，未知权限回退原始权限词（不显示为空白） */
function permissionTitle(key: string): string {
  const i18nKey = permissionTitleKey(key)
  return i18nKey ? t(i18nKey) : key
}

const stateLabel = computed(() => {
  switch (app.value?.state) {
    case 'running':
      return t('shell.common.running')
    case 'disabled':
      return t('shell.common.disabled')
    case 'error':
      return t('shell.common.error')
    default:
      return t('shell.common.stopped')
  }
})

const stateTone = computed<'success' | 'warn' | 'danger' | 'neutral'>(() => {
  switch (app.value?.state) {
    case 'running':
      return 'success'
    case 'disabled':
      return 'warn'
    case 'error':
      return 'danger'
    default:
      return 'neutral'
  }
})

const stateDot = computed(() =>
  app.value?.state === 'running' ? 'var(--mobile-success)' : 'var(--mobile-text-disabled)',
)

const toggleError = ref<string | null>(null)
/** 卸载二次确认（复用平台 ConfirmDialog，见模板） */
const confirmRemove = ref(false)
// 切换应用时清掉上一条的失败提示，避免张冠李戴
watch(appId, () => {
  toggleError.value = null
})

async function handleToggle(key: string, next: boolean): Promise<void> {
  if (!appId.value) return
  const ok = await setPermissionGrant(appId.value, key, next)
  if (!ok) {
    toggleError.value = t('shell.detail.permissionToggleFailed', {
      reason: t('shell.detail.permissionUnsupported'),
    })
  }
}

/** 演示：用应用真实的权限清单发起一次平台统一授权弹窗（不伪造用途） */
function openDemoPrompt(): void {
  if (!app.value) return
  const keys = app.value.permissions.filter((p) => !p.locked).map((p) => p.key)
  if (keys.length === 0) return
  overlays.requestPermissions({ appId: app.value.id, keys })
}

async function handleStop(): Promise<void> {
  if (!appId.value) return
  await stop(appId.value)
}

async function handleRemove(): Promise<void> {
  confirmRemove.value = false
  if (!app.value) return
  const ok = await removeApp(app.value.id)
  if (ok) {
    toast.success(t('shell.apps.uninstallSuccess', { name: app.value.name }))
    nav.back()
  } else {
    toast.error(t('shell.apps.uninstallFailed', { error: t('shell.run.notFound') }))
  }
}
</script>
