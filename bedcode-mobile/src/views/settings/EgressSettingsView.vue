<template>
  <SettingsSubPage :title="$t('settings.egress.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 授权记录列表 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.egress.grantsSection') }}</h2>

        <div v-if="loading" class="settings-group">
          <div class="settings-row">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.egress.loading') }}</span>
          </div>
        </div>

        <div v-else-if="grants.length === 0" class="settings-group">
          <div class="settings-row justify-center">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.egress.empty') }}</span>
          </div>
        </div>

        <div v-else class="settings-group">
          <div v-for="g in grants" :key="grantKey(g)" class="settings-row">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium text-[var(--mobile-text-primary)] break-all">{{ g.host }}</div>
              <div class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">
                {{ pathLabel(g) }}
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- 撤销全部授权 -->
      <section v-if="!loading && grants.length > 0" class="space-y-2">
        <button
          class="w-full flex items-center justify-center gap-2 py-3 rounded-xl text-sm font-medium text-[var(--mobile-error)] bg-[var(--mobile-error-muted)] border danger-action-btn transition-colors duration-200 active:scale-[0.98] active:opacity-80"
          @click="showConfirm = true"
        >
          <svg class="w-[1.125rem] h-[1.125rem] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          {{ $t('settings.egress.revokeAll') }}
        </button>
        <p class="px-1 text-xs text-[var(--mobile-text-muted)] leading-relaxed">
          {{ $t('settings.egress.revokeHint') }}
        </p>
      </section>
    </div>

    <!-- 撤销确认（防误触，danger 变体） -->
    <ConfirmDialog
      v-model="showConfirm"
      :title="$t('settings.egress.revokeConfirmTitle')"
      :message="$t('settings.egress.revokeConfirmMessage')"
      variant="danger"
      :confirm-text="$t('common.button.confirm')"
      :cancel-text="$t('common.button.cancel')"
      :loading="revoking"
      @confirm="doRevoke"
    />
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 网络访问授权设置二级页面（Egress L3，D8）
 *
 * 展示已授权的外网访问记录（会话级 + 持久，来自 Rust egress.rs list_grants），
 * 提供「撤销全部授权」（revoke_all_grants）：清空会话记忆 + 持久文件
 * （egress_grants.json）。裁决与记忆均在 Rust 端（§8 安全红线）。
 */
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'

/** Rust egress.rs PersistentGrant 形状 */
interface EgressGrant {
  host: string
  path_prefix?: string | null
  allowed_at: number
}

const { t } = useI18n()

const grants = ref<EgressGrant[]>([])
const loading = ref(true)
const showConfirm = ref(false)
const revoking = ref(false)

onMounted(loadGrants)

async function loadGrants() {
  loading.value = true
  try {
    grants.value = await invoke<EgressGrant[]>('egress_list_grants')
  } catch (e) {
    logger.error('[EgressSettings] list grants failed:', e)
    grants.value = []
  } finally {
    loading.value = false
  }
}

/** 列表 key：host + path 前缀（同一 host 多前缀授权时各占一行） */
function grantKey(g: EgressGrant): string {
  return `${g.host}${g.path_prefix || ''}`
}

/** 路径粒度 + 授权时间展示 */
function pathLabel(g: EgressGrant): string {
  const path = g.path_prefix && g.path_prefix !== '/' ? g.path_prefix : t('settings.egress.allPaths')
  const time = g.allowed_at ? new Date(g.allowed_at * 1000).toLocaleString() : ''
  return `${path} · ${time}`
}

async function doRevoke() {
  revoking.value = true
  try {
    await invoke('egress_revoke_grants')
    grants.value = []
  } catch (e) {
    logger.error('[EgressSettings] revoke grants failed:', e)
  } finally {
    revoking.value = false
    showConfirm.value = false
  }
}
</script>

<style scoped>
/* 与 SettingsView 撤销按钮一致的半透明描边（Tailwind 无法对 var() 应用透明度修饰符） */
.danger-action-btn {
  border-color: color-mix(in srgb, var(--mobile-error) 30%, transparent);
}
</style>
