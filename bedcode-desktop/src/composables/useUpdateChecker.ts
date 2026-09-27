/**
 * Update Checker - 基于 tauri-plugin-updater 的更新检查（桌面端）
 *
 * 使用 Tauri 官方 updater 插件，支持签名验证和应用内更新。
 *
 * 错误处理（票 04 / ADR 0030）：失败时**不**把原始错误暴露给 UI——
 * 状态置 `failed`，界面显示通用文案（`settings.about.checkFailed`），
 * 技术详情经 `logger.error` 落盘（release 下 error/warn 仍转发，见 frontendLogger）。
 * 本组件不再对外提供可渲染的裸错误字段（曾暴露 `errorMessage = e.message`）。
 */

import { ref } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import i18n from '@/locales'
import { logger } from '@/utils/frontendLogger'

/** 更新状态 */
type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'latest'
  | 'failed'
  | 'downloading'
  | 'downloaded'
  | 'installing'

const status = ref<UpdateStatus>('idle')
const downloadProgress = ref({ downloaded: 0, contentLength: 0 })

/** 检查更新 */
async function checkForUpdate() {
  status.value = 'checking'

  try {
    const update = await check()

    if (update) {
      status.value = 'available'
    } else {
      status.value = 'latest'
    }
    return update
  } catch (e) {
    // 票 04（ADR 0030）：详情只进日志，UI 走 status='failed' + 通用文案
    logger.error('[update-checker] checkForUpdate failed:', e)
    status.value = 'failed'
    return null
  }
}

/** 下载并安装更新 */
async function downloadAndInstall(onProgress?: (downloaded: number, total: number) => void) {
  try {
    const update = await check()
    if (!update) return

    status.value = 'downloading'
    downloadProgress.value = { downloaded: 0, contentLength: 0 }

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
    // 票 05（ADR 0030）：`check()` 也在 try 内——网络/签名检查失败不再产生
    // **未处理 rejection**（曾把原始错误抛成 unhandled rejection），统一走
    // failed 态 + 通用文案，详情只进日志
    logger.error('[update-checker] downloadAndInstall failed:', e)
    status.value = 'failed'
  }
}

/** 获取状态描述文本（UI 唯一文案来源；failed → 通用提示，永不返回技术详情） */
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
    checkForUpdate,
    downloadAndInstall,
    getUpdateStatusText,
  }
}