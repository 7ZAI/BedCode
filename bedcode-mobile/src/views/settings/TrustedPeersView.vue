<template>
  <SettingsSubPage :title="$t('settings.peer.title')">
    <div class="px-4 py-4 space-y-3">
      <!-- 空态 -->
      <div v-if="trustedPeers.length === 0" class="settings-group py-8 text-center">
        <p class="text-sm text-[var(--mobile-text-muted)]">{{ $t('settings.peer.empty') }}</p>
      </div>

      <!-- 对端卡片：名称/短指纹/完整 ID/加入时间 + 撤销 -->
      <div
        v-for="peer in trustedPeers"
        :key="peer.nodeId"
        class="settings-group p-4"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-baseline gap-2 min-w-0">
              <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate">
                {{ peer.displayName || $t('settings.peer.unknownDevice') }}
              </span>
              <span class="text-xs flex-shrink-0" style="color: var(--mobile-row-sub)">
                {{ peer.fingerprintShort }}
              </span>
            </div>
            <p class="text-xs mt-1 break-all" style="color: var(--mobile-row-sub)">
              {{ peer.nodeId }}
            </p>
            <p class="text-xs mt-1.5" style="color: var(--mobile-row-sub)">
              {{ $t('settings.peer.addedAt') }}{{ formatDateTime(peer.addedAt) }}
            </p>
          </div>
          <button
            class="flex-shrink-0 min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors duration-200 active:opacity-80"
            style="
              color: var(--mobile-error);
              background: color-mix(in srgb, var(--mobile-error) 12%, transparent);
            "
            @click="openRevokeConfirm(peer)"
          >
            {{ $t('settings.peer.revoke') }}
          </button>
        </div>
      </div>
    </div>

    <!-- 撤销确认 -->
    <ConfirmDialog
      v-model="showRevokeConfirm"
      :title="$t('settings.peer.revokeConfirmTitle')"
      :message="$t('settings.peer.revokeConfirmMsg', { name: revokeTargetName })"
      variant="danger"
      :confirm-text="$t('settings.peer.revoke')"
      :cancel-text="$t('common.button.cancel')"
      @confirm="handleRevoke"
    />
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 可信对端管理二级页面 — 对等网络信任层（issue 04）
 *
 * 列出本机全部可信对端（名称/指纹/加入时间），支持逐项撤销；
 * 撤销后对端重连将重新走首连确认弹窗。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import {
  listTrustedPeers,
  revokeTrustedPeer,
  type TrustedPeer,
} from '@/composables/useTrustedPeers'

const { locale } = useI18n()

const trustedPeers = ref<TrustedPeer[]>([])
const showRevokeConfirm = ref(false)
const revokeTarget = ref<TrustedPeer | null>(null)

const revokeTargetName = computed(() =>
  revokeTarget.value ? revokeTarget.value.displayName || revokeTarget.value.fingerprintShort : '',
)

/** 跟随当前 i18n locale 格式化加入时间 */
function formatDateTime(dateStr: string): string {
  if (!dateStr) return ''
  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return dateStr
  return new Intl.DateTimeFormat(locale.value === 'en' ? 'en-US' : 'zh-CN', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date)
}

async function load() {
  try {
    trustedPeers.value = await listTrustedPeers()
  } catch (error) {
    console.error('[TrustedPeers] load failed:', error)
    trustedPeers.value = []
  }
}

function openRevokeConfirm(peer: TrustedPeer) {
  revokeTarget.value = peer
  showRevokeConfirm.value = true
}

async function handleRevoke() {
  const target = revokeTarget.value
  if (!target) return
  showRevokeConfirm.value = false
  try {
    await revokeTrustedPeer(target.nodeId)
  } catch (error) {
    console.error('[TrustedPeers] revoke failed:', error)
  }
  revokeTarget.value = null
  await load()
}

onMounted(load)
</script>
