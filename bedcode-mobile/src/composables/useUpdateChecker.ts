/**
 * Update Checker - 基于 GitHub Releases API 的更新检查（移动端）
 *
 * 直接请求 GitHub API 比较版本号，发现新版本后引导用户到浏览器下载 APK
 * 不依赖 tauri-plugin-updater / tauri-plugin-process
 */

import { ref } from 'vue'
import { fetch } from '@tauri-apps/plugin-http'
import i18n from '@/locales'

/** 更新状态 */
type UpdateStatus = 'idle' | 'checking' | 'available' | 'latest' | 'failed'

/** GitHub Release 资产 */
interface ReleaseAsset {
  name: string
  browser_download_url: string
  size: number
}

/** GitHub Release 信息 */
interface GitHubRelease {
  tag_name: string
  name: string
  body: string
  html_url: string
  assets: ReleaseAsset[]
}

/** 新版本信息 */
export interface UpdateInfo {
  version: string
  downloadUrl: string
  releaseUrl: string
  releaseNotes: string
}

const GITHUB_REPO = '7ZAI/BedCode'
const GITHUB_API_LATEST = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`

const status = ref<UpdateStatus>('idle')
const errorMessage = ref('')
const updateInfo = ref<UpdateInfo | null>(null)

/** 从语义化版本字符串中提取可比较的数字数组 */
function parseVersion(v: string): number[] {
  return v
    .replace(/^v/, '')
    .split('.')
    .map((s) => parseInt(s, 10) || 0)
}

/** 比较两个语义化版本，a > b 返回正数 */
function compareVersions(a: string, b: string): number {
  const va = parseVersion(a)
  const vb = parseVersion(b)
  const len = Math.max(va.length, vb.length)
  for (let i = 0; i < len; i++) {
    const diff = (va[i] || 0) - (vb[i] || 0)
    if (diff !== 0) return diff
  }
  return 0
}

/** 从 release assets 中查找 APK 下载地址 */
function findApkAsset(assets: ReleaseAsset[]): ReleaseAsset | undefined {
  return assets.find((a) => a.name.endsWith('.apk'))
}

/** 检查更新 */
async function checkForUpdate(): Promise<UpdateInfo | null> {
  status.value = 'checking'
  errorMessage.value = ''
  updateInfo.value = null

  try {
    const response = await fetch(GITHUB_API_LATEST, {
      method: 'GET',
      headers: { Accept: 'application/vnd.github+json' },
    })

    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status}`)
    }

    const release: GitHubRelease = await response.json()
    const remoteVersion = release.tag_name
    const localVersion = __APP_VERSION__

    if (compareVersions(remoteVersion, localVersion) > 0) {
      const apkAsset = findApkAsset(release.assets)
      const info: UpdateInfo = {
        version: remoteVersion,
        downloadUrl: apkAsset?.browser_download_url ?? release.html_url,
        releaseUrl: release.html_url,
        releaseNotes: release.body || '',
      }
      updateInfo.value = info
      status.value = 'available'
      return info
    }

    status.value = 'latest'
    return null
  } catch (e) {
    errorMessage.value = e instanceof Error ? e.message : String(e)
    status.value = 'failed'
    return null
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
    errorMessage,
    updateInfo,
    checkForUpdate,
    getUpdateStatusText,
  }
}
