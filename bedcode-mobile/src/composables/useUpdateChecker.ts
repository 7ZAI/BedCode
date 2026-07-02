/**
 * Update Checker - GitHub Releases 版本检查
 *
 * 调用 GitHub API 检查最新 release，与当前版本比较
 */

import { ref } from 'vue'
import { fetch } from '@tauri-apps/plugin-http'
import { invoke } from '@tauri-apps/api/core'
import i18n from '@/locales'

const GITHUB_OWNER = '7ZAI'
const GITHUB_REPO = 'BedCode'
const GITHUB_API_URL = `https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest`

/** 版本比较结果 */
export interface UpdateInfo {
  hasUpdate: boolean
  currentVersion: string
  latestVersion: string
  downloadUrl: string
  releaseUrl: string
  releaseNotes: string
}

type UpdateStatus = 'idle' | 'checking' | 'available' | 'latest' | 'failed'

const status = ref<UpdateStatus>('idle')
const updateInfo = ref<UpdateInfo | null>(null)
const errorMessage = ref('')

/** 比较语义化版本号，返回 true 表示 remoteVersion > localVersion */
function isNewerVersion(remoteVersion: string, localVersion: string): boolean {
  const normalize = (v: string) => v.replace(/^v/, '')
  const rParts = normalize(remoteVersion).split('.').map(Number)
  const lParts = normalize(localVersion).split('.').map(Number)
  const len = Math.max(rParts.length, lParts.length)

  for (let i = 0; i < len; i++) {
    const r = rParts[i] || 0
    const l = lParts[i] || 0
    if (r > l) return true
    if (r < l) return false
  }
  return false
}

/** 从 release assets 中找到 Android APK 下载链接 */
function findApkAsset(assets: Array<{ name: string; browser_download_url: string }>): string {
  const apk = assets.find(a => a.name.endsWith('.apk'))
  return apk?.browser_download_url || ''
}

/** 检查 GitHub 最新 release */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  status.value = 'checking'
  errorMessage.value = ''

  try {
    const currentVersion = await invoke<string>('get_app_version')

    const response = await fetch(GITHUB_API_URL, {
      method: 'GET',
      headers: { 'Accept': 'application/vnd.github+json' },
    })

    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status}`)
    }

    const data = await response.json()

    const latestVersion = (data.tag_name as string) || ''
    const releaseUrl = (data.html_url as string) || ''
    const releaseNotes = (data.body as string) || ''
    const assets = (data.assets as Array<{ name: string; browser_download_url: string }>) || []
    const downloadUrl = findApkAsset(assets) || releaseUrl

    const hasUpdate = isNewerVersion(latestVersion, currentVersion)

    const info: UpdateInfo = {
      hasUpdate,
      currentVersion,
      latestVersion: latestVersion.replace(/^v/, ''),
      downloadUrl,
      releaseUrl,
      releaseNotes,
    }

    updateInfo.value = info
    status.value = hasUpdate ? 'available' : 'latest'
    return info
  } catch (e) {
    errorMessage.value = e instanceof Error ? e.message : String(e)
    status.value = 'failed'
    return null
  }
}

/** 获取状态描述文本 */
export function getUpdateStatusText(): string {
  const { t } = i18n.global
  switch (status.value) {
    case 'checking':
      return t('settings.about.checkingUpdate')
    case 'available':
      return t('settings.about.newVersionAvailable', { version: updateInfo.value?.latestVersion })
    case 'latest':
      return t('settings.about.alreadyLatest')
    case 'failed':
      return t('settings.about.updateCheckFailed')
    default:
      return t('settings.about.checkUpdate')
  }
}

export function useUpdateChecker() {
  return {
    status,
    updateInfo,
    errorMessage,
    checkForUpdate,
    getUpdateStatusText,
  }
}
