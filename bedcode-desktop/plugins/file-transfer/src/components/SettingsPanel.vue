<script setup lang="ts">
/**
 * SettingsPanel — 插件设置（覆盖层面板）— host-peer 契约版
 *
 * 共享目录列表管理（添加 = 系统目录选择器，条目由宿主持久化）、下载目录、
 * 接收策略/超时、安全告知常驻文案。纯展示组件，写操作经 emit 交给父级。
 */
import { inject } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Settings } from '../types'
import type { RootItem } from '../composables/useSettings'
import TrustedPeersSection from './TrustedPeersSection.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  settings: Settings
  /** 共享目录条目（宿主 DTO，带 id 供移除寻址） */
  rootItems: RootItem[]
}>()

const emit = defineEmits<{
  (e: 'addRoot'): void
  (e: 'removeRoot', id: string): void
  (e: 'pickDownloadDir'): void
  (e: 'setReceivingPolicy', policy: 'ask' | 'accept' | 'reject'): void
  (e: 'setApprovalTimeoutSec', secs: number): void
  (e: 'setEncryption', enabled: boolean): void
  (e: 'close'): void
}>()

// ==================== v2 接收策略（自绘分段控件，禁原生 select） ====================

/** 策略选项（顺序即展示顺序） */
const POLICY_OPTIONS: Array<{ value: 'ask' | 'accept' | 'reject'; key: string }> = [
  { value: 'ask', key: 'transfer.settings.receivingPolicyAsk' },
  { value: 'accept', key: 'transfer.settings.receivingPolicyAccept' },
  { value: 'reject', key: 'transfer.settings.receivingPolicyReject' },
]

/** 加密开关选项（复用分段控件标记结构；布尔二值） */
const ENCRYPTION_OPTIONS: Array<{ value: boolean; key: string }> = [
  { value: false, key: 'transfer.settings.encryptionOff' },
  { value: true, key: 'transfer.settings.encryptionOn' },
]

/** 超时输入：数字键盘 + 失焦提交（钳制在 composable 内） */
function onTimeoutBlur(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v)) emit('setApprovalTimeoutSec', v)
}
</script>

<template>
  <div class="ft-settings-backdrop">
    <div class="ft-settings">
      <div class="ft-topbar">
        <h2 class="ft-settings-title">{{ t('transfer.topbar.settings') }}</h2>
        <div class="ft-spacer"></div>
        <button class="ft-btn" @click="emit('close')">
          {{ t('transfer.topbar.closeSettings') }}
        </button>
      </div>

      <div class="ft-settings-body">
        <!-- 共享目录 -->
        <section class="ft-settings-section">
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.sharedRoots') }}</h3>
          <div
            v-if="rootItems.length === 0"
            class="ft-dir-value ft-dir-value--empty ft-dir-value--placeholder"
          >
            {{ t('transfer.settings.noRoots') }}
          </div>
          <div v-else class="ft-root-list">
            <div v-for="root in rootItems" :key="root.id" class="ft-root-item">
              <span class="ft-root-path" :title="root.name">{{ root.name }}</span>
              <button
                class="ft-mini-btn ft-mini-btn--ghost"
                :title="t('transfer.settings.removeRoot')"
                @click="emit('removeRoot', root.id)"
              >
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M18 6L6 18M6 6l12 12"
                  />
                </svg>
              </button>
            </div>
          </div>
          <div class="ft-settings-row ft-settings-row--push ft-settings-row--right">
            <button class="ft-btn" @click="emit('addRoot')">
              {{ t('transfer.settings.addRoot') }}
            </button>
          </div>
        </section>

        <!-- 下载目录 -->
        <section class="ft-settings-section">
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.downloadDir') }}</h3>
          <div class="ft-settings-row">
            <span
              class="ft-dir-value"
              :class="{ 'ft-dir-value--empty ft-dir-value--placeholder': !settings.downloadDir }"
            >
              {{ settings.downloadDir || t('transfer.settings.noDownloadDir') }}
            </span>
            <button class="ft-btn" @click="emit('pickDownloadDir')">
              {{ t('transfer.settings.chooseDir') }}
            </button>
          </div>
        </section>

        <!-- 接收策略（自绘分段控件，禁原生 select；超时输入仅 ask 时显示） -->
        <section class="ft-settings-section">
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.receivingPolicy') }}</h3>
          <div class="ft-segmented" role="tablist">
            <button
              v-for="opt in POLICY_OPTIONS"
              :key="opt.value"
              class="ft-segmented-item"
              :class="{ 'ft-segmented-item--active': settings.receivingPolicy === opt.value }"
              role="tab"
              :aria-selected="settings.receivingPolicy === opt.value"
              @click="emit('setReceivingPolicy', opt.value)"
            >
              {{ t(opt.key) }}
            </button>
          </div>
          <p class="ft-settings-helper">{{ t('transfer.settings.receivingPolicyHint') }}</p>
          <div
            v-if="settings.receivingPolicy === 'ask'"
            class="ft-settings-row ft-settings-timeout"
          >
            <label class="ft-timeout-label" for="ft-approval-timeout">
              {{ t('transfer.settings.approvalTimeout') }}
            </label>
            <input
              id="ft-approval-timeout"
              class="ft-timeout-input"
              type="number"
              min="10"
              max="600"
              step="1"
              :value="settings.approvalTimeoutSec"
              @blur="onTimeoutBlur"
            />
          </div>
        </section>

        <!-- 传输加密（默认关；发送侧生效，接收端经 Offer 头自动解密） -->
        <section class="ft-settings-section">
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.encryption') }}</h3>
          <div class="ft-segmented" role="tablist">
            <button
              v-for="opt in ENCRYPTION_OPTIONS"
              :key="opt.key"
              class="ft-segmented-item"
              :class="{ 'ft-segmented-item--active': settings.encryption === opt.value }"
              role="tab"
              :aria-selected="settings.encryption === opt.value"
              @click="emit('setEncryption', opt.value)"
            >
              {{ t(opt.key) }}
            </button>
          </div>
          <p class="ft-settings-helper">{{ t('transfer.settings.encryptionHint') }}</p>
        </section>

        <!-- 可信对端管理（spec 决策 8）：独立分区，列表/撤销自包含 -->
        <TrustedPeersSection />

        <!-- 安全告知（spec §10 常驻） -->
        <div class="ft-warning">
          <svg class="ft-warning-ico" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0zM12 9v4M12 17h.01"
            />
          </svg>
          <span>{{ t('transfer.settings.plainWarning') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
