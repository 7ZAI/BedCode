<template>
  <ShellSheet
    :open="request !== null"
    :title="request ? t('shell.permissionPrompt.title', { name: appName }) : ''"
    :subtitle="t('shell.permissionPrompt.subtitle')"
    @close="deny"
  >
    <div v-if="request && app" class="flex items-center gap-3 pb-3">
      <ShellAppIcon :app="app" :size="44" />
      <div class="min-w-0 flex-1">
        <p class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate">
          {{ appName }}
        </p>
        <p class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
          {{ app.version ? t('shell.detail.version', { version: app.version }) : app.id }}
        </p>
      </div>
    </div>

    <ShellGroupCard v-if="request">
      <div v-for="key in request.keys" :key="key" class="flex items-start gap-3 px-3 py-3">
        <svg class="flex-shrink-0 mt-0.5" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
          <path d="M12 21a9 9 0 100-18 9 9 0 000 18z M3 12h18 M12 3c2.5 2.6 3.8 5.7 3.8 9S14.5 18.4 12 21c-2.5-2.6-3.8-5.7-3.8-9S9.5 5.6 12 3z" />
        </svg>
        <div class="min-w-0 flex-1">
          <div class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">
            {{ permissionTitle(key) }}
            <code class="ml-1.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">{{ key }}</code>
          </div>
          <div v-if="request.targets?.[key]" class="mt-0.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] break-all">
            {{ request.targets[key] }}
          </div>
          <div class="mt-0.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
            {{ request.reasons?.[key] ? t('shell.permissionPrompt.purpose', { reason: request.reasons[key] }) : t('shell.permissionPrompt.purposeUnknown') }}
          </div>
        </div>
      </div>
    </ShellGroupCard>

    <p v-if="request" class="pt-2.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      {{ t('shell.permissionPrompt.fineprint', { name: appName }) }}
    </p>
    <p v-if="resultNote" class="pt-2 text-[var(--font-size-xs)]" :style="{ color: 'var(--mobile-warning)' }">
      {{ resultNote }}
    </p>

    <template #footer>
      <div class="flex gap-3">
        <button
          type="button"
          class="flex-1 rounded-[10px] border border-[var(--mobile-border)] px-4 py-3 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
          @click="deny"
        >
          {{ t('shell.permissionPrompt.deny') }}
        </button>
        <button
          type="button"
          class="flex-1 rounded-[10px] px-4 py-3 text-[var(--font-size-base)] transition-colors duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
          :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
          @click="allow"
        >
          {{ t('shell.permissionPrompt.allow') }}
        </button>
      </div>
    </template>
  </ShellSheet>
</template>

<script setup lang="ts">
/**
 * 运行时授权弹窗（平台统一）
 *
 * 裁决口径写死在这里而不是交给应用：拒绝 = 该功能不可用（fail-closed），
 * 且允许后必须真的落到数据源；数据源不支持时保留弹窗并说明，不做「点了就通过」。
 *
 * 真正的权限裁决在 Rust 端执行，本弹窗只是 UX（AGENTS.md §8）。
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../composables/useShellApps'
import { useShellOverlays } from '../composables/useShellOverlays'
import { permissionTitleKey } from '../permissions'
import ShellAppIcon from './ShellAppIcon.vue'
import ShellGroupCard from './ShellGroupCard.vue'
import ShellSheet from './ShellSheet.vue'

const { t } = useI18n()
const overlays = useShellOverlays()
const { getApp, setPermissionGrant } = useShellApps()

const request = computed(() => overlays.permissionRequest.value)
const app = computed(() => (request.value ? getApp(request.value.appId) : undefined))
const appName = computed(() => app.value?.name ?? request.value?.appId ?? '')

/** 裁决结果提示（数据源不支持时要显性说明，不能默默关掉） */
const resultNote = ref<string | null>(null)
watch(request, () => {
  resultNote.value = null
})

function permissionTitle(key: string): string {
  const i18nKey = permissionTitleKey(key)
  return i18nKey ? t(i18nKey) : key
}

/** 拒绝：直接关闭，不写任何状态（未授予即不可用，fail-closed） */
function deny(): void {
  overlays.closePermissionRequest()
}

async function allow(): Promise<void> {
  const current = request.value
  if (!current) return

  let unsupported = false
  for (const key of current.keys) {
    const ok = await setPermissionGrant(current.appId, key, true)
    if (!ok) unsupported = true
  }

  if (unsupported) {
    // 保留弹窗并说明：静默关闭会让用户以为授权成功了
    resultNote.value = t('shell.detail.permissionUnsupported')
    return
  }
  overlays.closePermissionRequest()
}
</script>
