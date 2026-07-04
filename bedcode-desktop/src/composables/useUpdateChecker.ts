/**
 * Update Checker - 基于 tauri-plugin-updater 的更新检查（桌面端）
 *
 * 使用 Tauri 官方 updater 插件，支持签名验证和应用内更新
 */

import { ref } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import i18n from '@/locales'

/** 更新状态 */
type UpdateStatus = 'idle' | 'checking' | 'available' | 'latest' | 'failed' | 'downloading' | 'downloaded' | 'installing'

const status = ref<UpdateStatus>('idle')
const downloadProgress = ref({ downloaded: 0, contentLength: 0 })
const errorMessage = ref('')

/** 检查更新 */
async function checkForUpdate() {
  status.value = 'checking'
  errorMessage.value = ''

  try {
    const update = await check()

    if (update) {
      status.value = 'available'
    } else {
      status.value = 'latest'
    }
    return update
  } catch (e) {
    errorMessage.value = e instanceof Error ? e.message : String(e)
    status.value = 'failed'
    return null
  }
}

/** 下载并安装更新 */
async function downloadAndInstall(onProgress?: (downloaded: number, total: number) => void) {
  const update = await check()
  if (!update) return

  status.value = 'downloading'
  downloadProgress.value = { downloaded: 0, contentLength: 0 }

  try {
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          downloadProgress.value.contentLength = event.data.contentLength ?? 0
          break
        case 'Progress':
          downloadProgress.value.downloaded += event.data.chunkLength
          onProgress?.(downloadProgress.value.downloaded, downloadProgress.value.contentLength)
          break
        case 'Finished':
          status.value = 'downloaded'
          break
      }
    })

    status.value = 'installing'
    await relaunch()
  } catch (e) {
    errorMessage.value = e instanceof Error ? e.message : String(e)
    status.value = 'failed'
  }
}

/** 获取状态描述文本 */
function getUpdateStatusText(): string {
  const { t } = i18n.global
  switch (status.value) {
    case 'checking':
      return t('settings.about.checkingUpdate')
    case 'available':
      return t('settings.about.newVersionAvailable')
    case 'downloading':
      return t('settings.about.downloadingUpdate')
    case 'downloaded':
      return t('settings.about.downloadComplete')
    case 'installing':
      return t('settings.about.installingUpdate')
    case 'latest':
      return t('settings.about.alreadyLatest')
    case 'failed':
      return t('settings.about.checkFailed')
    default:
      return t('settings.about.checkUpdate')
  }
}

export function useUpdateChecker() {
  return {
    status,
    downloadProgress,
    errorMessage,
    checkForUpdate,
    downloadAndInstall,
    getUpdateStatusText,
  }
}
