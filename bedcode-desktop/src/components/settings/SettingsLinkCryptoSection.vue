<template>
  <!-- ==================== LINK CRYPTO ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.linkCrypto.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 主开关：默认关（opt-in），关时全服务明文与现状一致 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div class="min-w-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.linkCrypto.master')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.linkCrypto.masterDesc') }}
          </p>
        </div>
        <button
          class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
          :class="
            linkCryptoConfig?.enabled
              ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
              : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
          "
          role="switch"
          :aria-checked="linkCryptoConfig?.enabled ?? false"
          @click="updateLinkCrypto({ enabled: !linkCryptoConfig?.enabled })"
        >
          <span
            class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
            :class="
              linkCryptoConfig?.enabled
                ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                : 'left-[3px] bg-[var(--border-strong)]'
            "
          />
        </button>
      </div>

      <!-- 通道子开关：主开关关时置灰不可点（粒度收窄是显式动作） -->
      <div
        v-for="channel in linkCryptoChannels"
        :key="channel.field"
        class="px-5 py-3.5 flex items-center justify-between gap-4"
        :class="{ 'opacity-50': !linkCryptoConfig?.enabled }"
      >
        <div class="min-w-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t(channel.labelKey)
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t(channel.descKey) }}
          </p>
        </div>
        <button
          class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0 disabled:cursor-not-allowed"
          :class="
            linkCryptoConfig?.[channel.field]
              ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
              : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
          "
          role="switch"
          :aria-checked="linkCryptoConfig?.[channel.field] ?? false"
          :disabled="!linkCryptoConfig?.enabled"
          @click="updateLinkCrypto({ [channel.field]: !linkCryptoConfig?.[channel.field] })"
        >
          <span
            class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
            :class="
              linkCryptoConfig?.[channel.field]
                ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                : 'left-[3px] bg-[var(--border-strong)]'
            "
          />
        </button>
      </div>

      <!-- 明文回退：关掉后非环回未协商请求一律拒绝（强加密模式） -->
      <div
        class="px-5 py-3.5 flex items-center justify-between gap-4"
        :class="{ 'opacity-50': !linkCryptoConfig?.enabled }"
      >
        <div class="min-w-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.linkCrypto.plaintextFallback')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.linkCrypto.plaintextFallbackDesc') }}
          </p>
        </div>
        <button
          class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0 disabled:cursor-not-allowed"
          :class="
            linkCryptoConfig?.allow_plaintext_fallback
              ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
              : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
          "
          role="switch"
          :aria-checked="linkCryptoConfig?.allow_plaintext_fallback ?? false"
          :disabled="!linkCryptoConfig?.enabled"
          @click="
            updateLinkCrypto({
              allow_plaintext_fallback: !linkCryptoConfig?.allow_plaintext_fallback,
            })
          "
        >
          <span
            class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
            :class="
              linkCryptoConfig?.allow_plaintext_fallback
                ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                : 'left-[3px] bg-[var(--border-strong)]'
            "
          />
        </button>
      </div>

      <!-- 本机指纹：供双端人工核对（移动端 pin 展示比对） -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div class="min-w-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.linkCrypto.fingerprint')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.linkCrypto.fingerprintDesc') }}
          </p>
        </div>
        <span
          class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0 truncate"
          >{{ linkCryptoFingerprint ?? '—' }}</span
        >
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 链路加密分组（SettingsView 拆分产物）
 *
 * 主开关 + 通道子开关 + 明文回退 + 本机指纹展示；乐观更新失败回滚
 * （不产生「UI 已开/后端未生效」的半启用状态）。挂载时加载配置。
 */
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import {
  useLinkCryptoConfig,
  type LinkCryptoConfig as LinkCryptoCfg,
} from '@/composables/useLinkCrypto'

const { t } = useI18n()
const toast = useToast()

const {
  config: linkCryptoConfig,
  fingerprint: linkCryptoFingerprint,
  loadLinkCryptoConfig,
  saveLinkCryptoConfig,
} = useLinkCryptoConfig()

/** 通道子开关元数据：字段名与 Rust serde snake_case 一致（deny_unknown_fields） */
const linkCryptoChannels: Array<{
  field: keyof Omit<LinkCryptoCfg, 'enabled' | 'allow_plaintext_fallback'>
  labelKey: string
  descKey: string
}> = [
  {
    field: 'encrypt_http',
    labelKey: 'settings.linkCrypto.encryptHttp',
    descKey: 'settings.linkCrypto.encryptHttpDesc',
  },
  {
    field: 'encrypt_ws_terminal',
    labelKey: 'settings.linkCrypto.encryptWsTerminal',
    descKey: 'settings.linkCrypto.encryptWsTerminalDesc',
  },
  {
    field: 'encrypt_ws_event',
    labelKey: 'settings.linkCrypto.encryptWsEvent',
    descKey: 'settings.linkCrypto.encryptWsEventDesc',
  },
]

/**
 * 乐观更新 + 失败回滚：单次切换整体保存，落库失败恢复原值并提示，
 * 不产生「UI 已开/后端未生效」的半启用状态
 */
async function updateLinkCrypto(patch: Partial<LinkCryptoCfg>) {
  if (!linkCryptoConfig.value) return
  const prev = linkCryptoConfig.value
  const next = { ...prev, ...patch }
  linkCryptoConfig.value = next
  try {
    await saveLinkCryptoConfig(next)
  } catch {
    linkCryptoConfig.value = prev
    toast.error(t('settings.linkCrypto.saveFailed'))
  }
}

onMounted(() => {
  loadLinkCryptoConfig().catch(() => {
    /* 配置域加载失败不阻断设置页其余部分；指纹/开关展示占位符 */
  })
})
</script>
