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
 * 关于设置二级页面 - 版本信息、GitHub 仓库、检查更新
 * 更新检查基于 GitHub Releases API，发现新版本后引导浏览器下载 APK
 */
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { useUpdateChecker } from '@/composables/useUpdateChecker'

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
      console.error('Failed to open URL:', e)
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
