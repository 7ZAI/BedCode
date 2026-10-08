<template>
  <SettingsSubPage :title="$t('settings.egress.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 访问策略（per-plugin × network，ADR 0022 2026-09-28 对齐） -->
      <section v-if="!loading" class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.egress.strategySection') }}</h2>
        <p class="px-1 text-xs text-[var(--mobile-text-muted)]">{{ $t('settings.egress.strategyHint') }}</p>

        <div v-if="plugins.length === 0" class="settings-group">
          <div class="settings-row justify-center">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.egress.empty') }}</span>
          </div>
        </div>

        <div v-for="plugin in plugins" :key="plugin.pluginId" class="settings-group p-3 space-y-2.5">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-sm font-medium text-[var(--mobile-text-primary)] truncate">{{ pluginLabel(plugin.pluginId) }}</span>
          </div>
          <div class="flex gap-2">
            <button
              v-for="tier in tiers"
              :key="tier.value"
              class="flex-1 px-2 py-2.5 rounded-xl text-xs font-medium transition-opacity duration-200 active:opacity-80"
              :class="plugin.strategy === tier.value
                ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
                : 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] text-[var(--mobile-text-secondary)]'"
              @click="setStrategy(plugin, tier.value)"
            >
              {{ $t(tier.labelKey) }}
            </button>
          </div>
          <p class="px-1 text-xs text-[var(--mobile-text-muted)]">{{ $t(tierDescKey(plugin.strategy)) }}</p>
        </div>
      </section>

      <!-- 授权记录列表 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.egress.recordsSection') }}</h2>

        <div v-if="loading" class="settings-group">
          <div class="settings-row">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.egress.loading') }}</span>
          </div>
        </div>

        <div v-else-if="records.length === 0" class="settings-group">
          <div class="settings-row justify-center">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.egress.empty') }}</span>
          </div>
        </div>

        <div v-else class="settings-group">
          <div v-for="r in records" :key="recordKey(r)" class="settings-row">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 min-w-0">
                <span class="text-sm font-medium text-[var(--mobile-text-primary)] break-all">{{ r.target }}</span>
                <span
                  class="flex-shrink-0 inline-flex items-center h-5 px-2 rounded-tag text-[0.6875rem] font-medium"
                  :class="sourceBadgeClass(r)"
                >
                  {{ sourceLabel(r) }}
                </span>
              </div>
              <div class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">
                {{ pathLabel(r) }} · {{ pluginLabel(r.pluginId) }}
              </div>
            </div>
            <button
              class="flex-shrink-0 ml-2 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity duration-200"
              @click="confirmRevokeOne(r)"
            >
              {{ $t('settings.egress.revokeOne') }}
            </button>
          </div>
        </div>
      </section>

      <!-- 撤销全部授权 -->
      <section v-if="!loading && records.length > 0" class="space-y-2">
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

    <!-- 单条撤销确认 -->
    <ConfirmDialog
      v-model="showRevokeOne"
      :title="$t('settings.egress.revokeOneConfirmTitle')"
      :message="$t('settings.egress.revokeOneConfirmMessage')"
      variant="danger"
      :confirm-text="$t('common.button.confirm')"
      :cancel-text="$t('common.button.cancel')"
      :loading="revokingOne"
      @confirm="doRevokeOne"
    />
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 网络访问授权设置二级页面（Egress L3，D8 / 票 19）
 *
 * 展示已授权的外网访问记录（会话级 + 持久，来自 Rust egress.rs list_records），
 * 提供「撤销全部授权」与逐条撤销；顶部为 per-plugin 三档访问策略（总是询问 /
 * 默认 / 始终允许，ADR 0022 2026-09-28 对齐）。裁决与记忆均在 Rust 端（§8 安全红线）。
 */
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'

/** Rust egress.rs AuthRecord 形状 */
interface EgressRecord {
  plugin_id: string
  effect: string
  target: string
  path_prefix?: string | null
  source: string
  created_at: number
}

/** 单插件策略视图（plugin_id → 当前档位） */
interface PluginStrategy {
  pluginId: string
  strategy: string
}

const { t } = useI18n()

/** 三档策略（与 Rust AuthStrategy.as_str() 取值一致；顺序即界面顺序） */
const tiers = [
  { value: 'always_ask', labelKey: 'settings.egress.alwaysAsk' },
  { value: 'default', labelKey: 'settings.egress.defaultStrategy' },
  { value: 'always_allow', labelKey: 'settings.egress.alwaysAllow' },
]

const records = ref<EgressRecord[]>([])
const plugins = ref<PluginStrategy[]>([])
const loading = ref(true)
const showConfirm = ref(false)
const revoking = ref(false)
const showRevokeOne = ref(false)
const revokingOne = ref(false)
const pendingRevoke = ref<EgressRecord | null>(null)

onMounted(load)

async function load() {
  loading.value = true
  try {
    records.value = await invoke<EgressRecord[]>('egress_list_records')
    // 有记录/策略的插件去重（策略缺省 = default，无需展示全部插件）
    const ids = [...new Set(records.value.map((r) => r.pluginId))]
    plugins.value = await Promise.all(
      ids.map(async (pluginId) => ({
        pluginId,
        strategy: await invoke<string>('egress_get_strategy', { pluginId }),
      })),
    )
  } catch (e) {
    logger.error('[EgressSettings] load failed:', e)
    records.value = []
    plugins.value = []
  } finally {
    loading.value = false
  }
}

async function setStrategy(plugin: PluginStrategy, strategy: string) {
  try {
    await invoke('egress_set_strategy', { pluginId: plugin.pluginId, strategy })
    plugin.strategy = strategy
  } catch (e) {
    logger.error('[EgressSettings] set strategy failed:', e)
  }
}

/** 插件展示名：插件 id 去掉 `plugin:` 前缀（宿主全局记录显示为 host） */
function pluginLabel(pluginId: string): string {
  return pluginId.startsWith('plugin:') ? pluginId.slice('plugin:'.length) : pluginId
}

/** 档位描述文案 key（每档一行说明） */
function tierDescKey(strategy: string): string {
  switch (strategy) {
    case 'always_ask':
      return 'settings.egress.alwaysAskDesc'
    case 'always_allow':
      return 'settings.egress.alwaysAllowDesc'
    default:
      return 'settings.egress.defaultStrategyDesc'
  }
}

/** 来源徽章（已确认 / 未经确认 / 已拒绝） */
function sourceLabel(r: EgressRecord): string {
  switch (r.source) {
    case 'always_allow':
      return t('settings.egress.sourceAlwaysAllow')
    case 'user_deny':
      return t('settings.egress.sourceUserDeny')
    default:
      return t('settings.egress.sourceUser')
  }
}

function sourceBadgeClass(r: EgressRecord): string {
  switch (r.source) {
    case 'always_allow':
      return 'bg-[var(--mobile-warning-muted)] text-[var(--mobile-warning)]'
    case 'user_deny':
      return 'bg-[var(--mobile-error-muted)] text-[var(--mobile-error)]'
    default:
      return 'bg-[var(--mobile-success-muted)] text-[var(--mobile-success)]'
  }
}

/** 列表 key：插件 + 目标 + 来源（同 host 多来源记录各占一行） */
function recordKey(r: EgressRecord): string {
  return `${r.pluginId}|${r.target}|${r.source}`
}

/** 路径粒度 + 授权时间展示 */
function pathLabel(r: EgressRecord): string {
  const path = r.path_prefix && r.path_prefix !== '/' ? r.path_prefix : t('settings.egress.allPaths')
  const time = r.created_at ? new Date(r.created_at * 1000).toLocaleString() : ''
  return `${path} · ${time}`
}

function confirmRevokeOne(r: EgressRecord) {
  pendingRevoke.value = r
  showRevokeOne.value = true
}

async function doRevokeOne() {
  const r = pendingRevoke.value
  if (!r) return
  revokingOne.value = true
  try {
    await invoke('egress_revoke_record', { host: r.target, pluginId: r.pluginId })
    records.value = records.value.filter((x) => !(x.target === r.target && x.pluginId === r.pluginId))
  } catch (e) {
    logger.error('[EgressSettings] revoke record failed:', e)
  } finally {
    revokingOne.value = false
    showRevokeOne.value = false
    pendingRevoke.value = null
  }
}

async function doRevoke() {
  revoking.value = true
  try {
    await invoke('egress_revoke_grants')
    records.value = []
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