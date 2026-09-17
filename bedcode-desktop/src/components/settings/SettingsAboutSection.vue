<template>
        <!-- ==================== ABOUT ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.about.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-5 py-4">
            <div class="flex items-center justify-between gap-4">
              <div class="flex items-center gap-2">
                <span
                  class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
                  >BedCode</span
                >
                <span class="wb-mono text-[var(--text-secondary)]">v{{ appVersion || '—' }}</span>
              </div>
              <div class="flex items-center gap-3">
                <!-- GitHub 仓库：系统浏览器打开 -->
                <button class="wb-btn-ghost" @click="openGitHub">
                  <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                    <path
                      d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.27-.01-1.17-.02-2.12-3.2.7-3.87-1.36-3.87-1.36-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.02 1.75 2.68 1.25 3.34.95.1-.74.4-1.25.73-1.54-2.55-.29-5.23-1.28-5.23-5.68 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .96-.31 3.15 1.18a10.96 10.96 0 015.74 0c2.19-1.49 3.15-1.18 3.15-1.18.62 1.59.23 2.76.11 3.05.73.81 1.18 1.83 1.18 3.09 0 4.41-2.69 5.38-5.25 5.67.41.35.77 1.05.77 2.12 0 1.53-.01 2.76-.01 3.14 0 .31.21.67.8.56A11.51 11.51 0 0023.5 12C23.5 5.65 18.35.5 12 .5z"
                    />
                  </svg>
                  {{ t('settings.about.githubRepo') }}
                </button>
                <!-- 下载进度 -->
                <template v-if="updateStatus === 'downloading'">
                  <div class="w-32 h-1.5 bg-[var(--border)] overflow-hidden">
                    <div
                      class="h-full bg-[var(--color-primary)] transition-all duration-300"
                      :style="{ width: downloadPercent + '%' }"
                    />
                  </div>
                  <span class="wb-mono text-[var(--text-secondary)]">{{ downloadPercent }}%</span>
                </template>
                <button
                  v-else-if="updateStatus === 'available'"
                  class="wb-btn-primary"
                  @click="handleInstallUpdate"
                >
                  {{ t('settings.about.downloadUpdate') }}
                </button>
                <span
                  v-else-if="
                    updateStatus !== 'idle' &&
                    updateStatus !== 'latest' &&
                    updateStatus !== 'failed'
                  "
                  class="text-xs text-[var(--text-secondary)]"
                >
                  {{ getUpdateStatusText() }}
                </span>
              </div>
            </div>
            <p v-if="updateStatus === 'failed'" class="mt-2 text-xs text-red-500">
              {{ errorMessage }}
            </p>
          </div>
        </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 关于分组（SettingsView 拆分产物）
 *
 * 版本号、GitHub 仓库链接、更新检查 / 下载 / 安装进度。
 * 挂载时读取应用版本号；更新状态由 useUpdateChecker（模块级单例）共享，
 * 工具栏的「检查更新」按钮与父组件共用同一状态。
 */
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import { useUpdateChecker } from '@/composables/useUpdateChecker'
import { getAppVersion } from '@/composables/useDesktopCommands'
import { open } from '@tauri-apps/plugin-shell'
import { logger } from '@/utils/frontendLogger'
import i18n from '@/locales'

const { t } = useI18n()
const toast = useToast()

const {
  status: updateStatus,
  downloadProgress,
  errorMessage,
  checkForUpdate,
  downloadAndInstall,
  getUpdateStatusText,
} = useUpdateChecker()

const appVersion = ref('')

async function handleCheckUpdate() {
  const update = await checkForUpdate()
  if (!update && updateStatus.value === 'latest') {
    toast.info(i18n.global.t('settings.about.alreadyLatest'))
  } else if (!update && updateStatus.value === 'failed') {
    toast.error(i18n.global.t('settings.about.checkFailed'))
  }
}

async function handleInstallUpdate() {
  await downloadAndInstall()
}

/** 在系统浏览器中打开 GitHub 仓库 */
async function openGitHub() {
  try {
    await open('https://github.com/7ZAI/BedCode')
  } catch (e) {
    logger.error('Failed to open GitHub repo:', e)
  }
}

const downloadPercent = computed(() => {
  if (downloadProgress.value.contentLength === 0) return 0
  return Math.round(
    (downloadProgress.value.downloaded / downloadProgress.value.contentLength) * 100,
  )
})

onMounted(async () => {
  try {
    appVersion.value = await getAppVersion()
  } catch {
    appVersion.value = '—'
  }
})
</script>
