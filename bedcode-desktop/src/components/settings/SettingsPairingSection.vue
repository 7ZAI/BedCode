<template>
  <!-- ==================== PAIRING ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.pairing.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 默认端口：服务器启动时使用的端口 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.pairing.defaultPort')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.pairing.defaultPortDesc') }}
          </p>
        </div>
        <input
          type="number"
          :value="settingsStore.settings.network.port"
          class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="
            settingsStore.settings.network.port = Number(($event.target as HTMLInputElement).value)
          "
        />
      </div>

      <!-- 二维码有效期：配对时展示的二维码有效时间 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.pairing.qrValidity')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.pairing.qrValidityDesc') }}
          </p>
        </div>
        <input
          type="number"
          :value="qrTokenTtl"
          class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="qrTokenTtl = Number(($event.target as HTMLInputElement).value)"
          @blur="saveQrTokenTtl"
        />
      </div>

      <!-- 配对码有效期：手动输入的配对码有效时间 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.pairing.pairingCodeTtl')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.pairing.pairingCodeTtlDesc') }}
          </p>
        </div>
        <input
          type="number"
          :value="pairingCodeTtl"
          class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="pairingCodeTtl = Number(($event.target as HTMLInputElement).value)"
          @blur="savePairingCodeTtl"
        />
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 配对分组（SettingsView 拆分产物）
 *
 * 默认端口（存 settings）与 QR / 配对码有效期（独立后端配置，挂载时加载、
 * 失焦时保存，与后端 TTL 配置一致）。
 */
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useQrCodeApi } from '@/composables/useTauri'
import { getPairingCodeTtl, setPairingCodeTtl } from '@/composables/useDesktopCommands'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const qrApi = useQrCodeApi()

const qrTokenTtl = ref(300)
const pairingCodeTtl = ref(60)

async function loadQrTokenTtl() {
  qrTokenTtl.value = await qrApi.getQrTokenTtl()
}

async function saveQrTokenTtl() {
  const val = Math.max(60, Math.min(3600, qrTokenTtl.value))
  qrTokenTtl.value = val
  await qrApi.setQrTokenTtl(val)
}

async function loadPairingCodeTtl() {
  pairingCodeTtl.value = await getPairingCodeTtl()
}

async function savePairingCodeTtl() {
  const val = Math.max(60, Math.min(3600, pairingCodeTtl.value))
  pairingCodeTtl.value = val
  await setPairingCodeTtl(val)
}

onMounted(async () => {
  await loadQrTokenTtl()
  await loadPairingCodeTtl()
})
</script>
