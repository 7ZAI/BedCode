<template>
  <SettingsSubPage :title="$t('settings.connection.title')">
    <div class="px-4 py-4 space-y-5">
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.reconnectSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.autoReconnect') }}</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.keepAlive') }}</span>
            <Toggle v-model="settings.keepAlive" />
          </div>
        </div>
      </section>

      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.networkSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.defaultPort') }}</span>
            <div class="settings-stepper shrink-0">
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.defaultPort) <= 1"
                @click="stepDefaultPort(-1)"
                :aria-label="t('common.button.decrease')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M20 12H4" /></svg>
              </button>
              <input
                v-model.number="settings.defaultPort"
                type="number"
                inputmode="numeric"
                min="1"
                max="65535"
                class="settings-number-input"
              />
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.defaultPort) >= 65535"
                @click="stepDefaultPort(1)"
                :aria-label="t('common.button.increase')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M12 4v16m8-8H4" /></svg>
              </button>
            </div>
          </div>
        </div>
      </section>
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.linkCryptoSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkCryptoMaster') }}</span>
            <Toggle
              :model-value="linkSettings.settings.value.enabled"
              @update:model-value="onToggleLinkEncryption"
            />
          </div>
          <!-- 通道子开关：主开关关时置灰（与桌面端粒度对齐） -->
          <div
            v-for="sub in linkChannelRows"
            :key="sub.channel"
            class="settings-row"
            :class="{ 'opacity-50': !linkSettings.settings.value.enabled }"
          >
            <span class="settings-label">{{ $t(sub.labelKey) }}</span>
            <Toggle
              :model-value="sub.value"
              :disabled="!linkSettings.settings.value.enabled"
              @update:model-value="(v: boolean) => linkSettings.setChannel(sub.channel, v)"
            />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkStrictMode') }}</span>
            <Toggle
              :model-value="linkSettings.settings.value.strictMode"
              :disabled="!linkSettings.settings.value.enabled"
              @update:model-value="linkSettings.setStrictMode"
            />
          </div>
          <!-- 对端指纹：人工核对锚点；未配对时展示占位 -->
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkPeerFingerprint') }}</span>
            <span class="font-mono text-xs text-[var(--mobile-text-muted)] truncate">{{
              pinnedFingerprint ?? $t('settings.connection.linkNotPaired')
            }}</span>
          </div>
        </div>
        <p class="text-xs text-[var(--mobile-text-muted)] px-1">
          {{ $t('settings.connection.linkCryptoHint') }}
        </p>
      </section>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 连接设置二级页面 - 自动重连、保持连接、默认端口 + 链路加密（issue 08）
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 *
 * 「重连间隔」可调项已于 2026-10-04 移除：重连节奏收敛到 Rust 侧
 * `ReconnectManager`（指数退避 + 抖动 + 同因熔断 + 1s 下限），不再是前端
 * 的固定间隔循环。把退避算法参数漏给用户只会诱导他调出一个「3 秒重连一次」
 * 的打服务端配置；留着开关却不再生效则是静默失效。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'
import {
  getPinnedFingerprint,
  getPinnedKey,
  useLinkEncryptionSettings,
} from '@/composables/useLinkEncryption'
import { useToast } from '@/composables/useToast'
import { syncAutoReconnectSetting } from '@/composables/useMobileConnection'

const { t } = useI18n()
const toast = useToast()
const { settings, loadSettings } = useMobileSettings()
const linkSettings = useLinkEncryptionSettings()
// 指纹行用 ref 而非无依赖 computed：后者首次求值后永久缓存，设置页常驻时
// 配对完成（ws_link_crypto_pin 事件落地）指纹行仍显示「未配对」
const pinnedFingerprint = ref(getPinnedFingerprint())

/** 从 localStorage 刷新指纹（配对/重认证事件驱动） */
function refreshPinnedFingerprint(): void {
  pinnedFingerprint.value = getPinnedFingerprint()
}

/** HTTP 载荷加密子开关行元数据（WS 通道已随桌面端插件端点加密退役，仅剩 http） */
const linkChannelRows = computed(() => [
  {
    channel: 'http' as const,
    labelKey: 'settings.connection.linkEncryptHttp',
    value: linkSettings.settings.value.encryptHttp,
  },
])

/**
 * 主开关切换守卫：无 pin（桌面端身份公钥）时拒绝开启并引导先配对——加密协商
 * 依赖配对期下发的桌面端身份公钥作信任锚，无 pin 开关只会产生「开着但永不
 * 生效」的半启用态。判断用公钥而非指纹：指纹仅展示用途，公钥才是协商前提。
 */
function onToggleLinkEncryption(next: boolean) {
  if (next && !getPinnedKey()) {
    toast.error(t('settings.connection.linkNeedPairing'))
    return
  }
  linkSettings.setEnabled(next)
}

onMounted(async () => {
  await loadSettings()
  refreshPinnedFingerprint()
  // 首次同步：设置页是唯一入口，但 Rust 侧 flag 不随 localStorage 自动恢复，
  // 不在这里推一次，重启后开关会回到默认值
  await syncAutoReconnectSetting()
  // 配对/重认证（配对码/QR/reauth/生物认证）统一经 ws_link_crypto_pin 落地
  const { listen } = await import('@tauri-apps/api/event')
  const unlisten = await listen('ws_link_crypto_pin', refreshPinnedFingerprint)
  onUnmounted(() => unlisten())
})

// 自动重连开关递到 Rust 连接层（策略归连接层，UI 只递意图）
watch(
  () => settings.value.autoReconnect,
  () => { void syncAutoReconnectSetting() },
)

// ==================== 数字步进 ====================

/** 默认端口步进：钳制到 1-65535 */
function stepDefaultPort(delta: number) {
  const next = Number(settings.value.defaultPort) + delta
  settings.value.defaultPort = Math.max(1, Math.min(65535, Number.isFinite(next) ? next : 1))
}
</script>
