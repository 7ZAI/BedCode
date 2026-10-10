<template>
  <SettingsSubPage :title="$t('settings.connection.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 票 2026-10-10 批次 C4：本页只保留**平台项**（链路加密，ADR 0022 ②类安全闸门）。
           自动重连 / 保持连接 / 默认端口是业务设置，票 2026-10-10 起真源与 UI 归
           terminal-session 的应用内设置页（spec §2 设置切分口径），此处不再重复实现——
           两处各写一份必然漂移，且会让用户不知道改哪个才作数。 -->
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

      <!-- 业务设置去处说明：不留死胡同，用户找得到「自动重连在哪」 -->
      <p class="text-xs text-[var(--mobile-text-muted)] px-1">
        {{ $t('settings.connection.businessElsewhere') }}
      </p>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 链路加密设置二级页面（issue 08）
 *
 * 票 2026-10-10 批次 C4 起本页**只管平台项**：链路加密是 ADR 0022 ②类安全闸门
 * （fail-closed，属宿主机制面），故留壳。原先同页的自动重连 / 保持连接 / 默认端口
 * 是业务设置，真源与 UI 已下沉 terminal-session，此处删除以免两处实现漂移。
 *
 * 「重连间隔」可调项已于 2026-10-04 移除：重连节奏收敛到 Rust 侧
 * `ReconnectManager`（指数退避 + 抖动 + 同因熔断 + 1s 下限），不再是前端
 * 的固定间隔循环。把退避算法参数漏给用户只会诱导他调出一个「3 秒重连一次」
 * 的打服务端配置；留着开关却不再生效则是静默失效。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import {
  getPinnedFingerprint,
  getPinnedKey,
  useLinkEncryptionSettings,
} from '@/composables/useLinkEncryption'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()
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
  refreshPinnedFingerprint()
  // 配对/重认证（配对码/QR/reauth/生物认证）统一经 ws_link_crypto_pin 落地
  const { listen } = await import('@tauri-apps/api/event')
  const unlisten = await listen('ws_link_crypto_pin', refreshPinnedFingerprint)
  onUnmounted(() => unlisten())
})
</script>
