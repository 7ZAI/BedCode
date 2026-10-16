<template>
  <SettingsSubPage :title="$t('settings.about.title')">
    <div class="px-4 py-4 space-y-5">
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.about.infoSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.about.currentVersion') }}</span>
            <span class="settings-value">v{{ appVersion }}</span>
          </div>
          <button class="settings-row" @click="openGitHub">
            <span class="settings-label">{{ $t('settings.about.githubRepo') }}</span>
            <svg class="w-4 h-4 shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>
      </section>

      <!-- 检查更新 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.about.updateSection') }}</h2>
        <div class="settings-group">
          <!-- 空闲/已是最新/失败：可点击检查 -->
          <button
            v-if="updateStatus === 'idle' || updateStatus === 'latest' || updateStatus === 'failed'"
            class="settings-row"
            @click="handleCheckUpdate"
          >
            <span class="settings-label">{{ getUpdateStatusText() }}</span>
            <svg class="w-4 h-4 shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </button>

          <!-- 检查中 -->
          <div v-if="updateStatus === 'checking'" class="settings-row">
            <span class="settings-label text-[var(--mobile-text-muted)]">{{ $t('settings.about.checkingUpdate') }}</span>
            <span class="inline-block w-4 h-4 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin" />
          </div>

          <!-- 失败时显示错误 -->
          <div v-if="updateStatus === 'failed' && errorMessage" class="settings-row">
            <span class="text-xs text-[var(--mobile-error)]">{{ errorMessage }}</span>
          </div>
        </div>

        <!-- 发现新版本 - 打开浏览器下载 -->
        <button
          v-if="updateStatus === 'available' && updateInfo"
          class="w-full bg-[color:color-mix(in_srgb,var(--mobile-accent)_15%,transparent)] border border-[color:color-mix(in_srgb,var(--mobile-accent)_30%,transparent)] text-[var(--mobile-accent)] py-3 rounded-xl text-sm font-medium active:bg-[color:color-mix(in_srgb,var(--mobile-accent)_25%,transparent)] transition-colors"
          @click="handleDownloadUpdate"
        >
          {{ $t('settings.about.downloadUpdate') }} ({{ updateInfo.version }})
        </button>
      </section>

      <!-- 危险操作：设备级擦除
           2026-10-14 从平台设置一级页迁到这里——「关于本机」才是这类动作的语境。
           擦除对象含设备入场凭据与宿主连接态（§8 凭据零过境 / ADR 0033），
           实现见 useClearAllData.ts，不得下沉到应用设置页。 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.about.dangerZone') }}</h2>
        <button
          type="button"
          class="w-full flex items-center gap-3 px-3 py-3 text-left rounded-xl border transition-colors duration-200 active:opacity-80"
          :style="{
            background: 'var(--mobile-error-muted)',
            borderColor: 'var(--mobile-error)',
            color: 'var(--mobile-error)',
          }"
          @click="onClearAllData"
        >
          <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          <span class="flex-1 min-w-0 text-sm">
            {{ $t('settings.about.clearAllData') }}
          </span>
        </button>
        <p class="px-1 text-xs text-[var(--mobile-text-muted)]">
          {{ $t('settings.about.clearAllDataHint') }}
        </p>
      </section>
    </div>

    <!-- Browser Confirm Modal -->
    <Teleport to="body">
      <Transition name="center-modal">
      <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelOpenBrowser">
        <div class="confirm-modal modal-panel">
          <p class="confirm-text">{{ $t('settings.browser.confirmOpen') }}</p>
          <p class="confirm-url text-xs text-[var(--mobile-text-muted)] mt-1 mb-4 break-all">{{ pendingUrl }}</p>
          <div class="confirm-buttons">
            <button class="confirm-btn cancel" @click="cancelOpenBrowser">{{ $t('common.button.cancel') }}</button>
            <button class="confirm-btn confirm" @click="confirmOpenBrowser">{{ $t('common.button.open') }}</button>
          </div>
        </div>
      </div>
      </Transition>
    </Teleport>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 关于设置二级页面 - 版本信息、GitHub 仓库、检查更新、危险操作
 * 更新检查基于 GitHub Releases API，发现新版本后引导浏览器下载 APK
 *
 * 危险操作（清除所有数据）于 2026-10-14 从平台设置一级页迁入：
 * 它擦除设备入场凭据与宿主连接态，属设备生命周期动作而非应用业务设置，
 * 归宿主（useClearAllData.ts 头注有完整裁决）。
 */
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import { invoke } from '@tauri-apps/api/core'
import { confirm } from '@tauri-apps/plugin-dialog'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { clearAllData } from '@/composables/useClearAllData'
import { useToast } from '@/composables/useToast'
import { useUpdateChecker } from '@/composables/useUpdateChecker'

const { t } = useI18n()
const toast = useToast()
const { status: updateStatus, errorMessage, updateInfo, checkForUpdate, getUpdateStatusText } = useUpdateChecker()

/** 应用版本号，由 Vite 编译时从 tauri.conf.json 注入 */
const appVersion = __APP_VERSION__

// 系统浏览器打开链接的确认弹窗状态
const showBrowserConfirm = ref(false)
const pendingUrl = ref('')

function openGitHub() {
  pendingUrl.value = 'https://github.com/7ZAI/BedCode'
  showBrowserConfirm.value = true
}

async function confirmOpenBrowser() {
  if (pendingUrl.value) {
    try {
      await invoke('open_url_in_browser', { url: pendingUrl.value })
    } catch (e) {
      logger.error('Failed to open URL:', e)
    }
  }
  showBrowserConfirm.value = false
  pendingUrl.value = ''
}

function cancelOpenBrowser() {
  showBrowserConfirm.value = false
  pendingUrl.value = ''
}

async function handleCheckUpdate() {
  await checkForUpdate()
}

/** 发现新版本后，打开浏览器下载 APK */
function handleDownloadUpdate() {
  if (updateInfo.value) {
    pendingUrl.value = updateInfo.value.downloadUrl
    showBrowserConfirm.value = true
  }
}

/**
 * 擦除本机数据
 *
 * 二次确认用宿主 confirm 对话框：这是不可撤销的动作，误触代价是重新配对 + 重建全部配置。
 * 断连失败不阻断清理（用户的目标是清数据，不是断连），但清理失败必须如实告知——
 * 不能 reload 完就说成功。
 */
async function onClearAllData(): Promise<void> {
  const confirmed = await confirm(t('settings.about.clearAllData'), {
    title: t('settings.about.clearAllData'),
    kind: 'warning',
  })
  if (!confirmed) return

  const result = await clearAllData()
  if (!result.completed) {
    // 清理失败时不能 reload——那会让用户以为已经清干净
    logger.error(`[AboutSettings] clear all data incomplete: ${result.error ?? 'unknown'}`)
    toast.error(t('settings.about.clearAllDataFailed'))
  }
}
</script>

<style scoped>
.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: clamp(260px, 320px, 380px);
  text-align: center;
}

.confirm-text {
  font-size: var(--font-size-lg);
  color: var(--mobile-text-primary);
  margin: 0;
}

.confirm-url {
  color: var(--mobile-accent);
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
  margin-top: 1.25rem;
}

.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: var(--font-size-base);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.confirm-btn.confirm:hover {
  opacity: 0.9;
}
</style>
