/**
 * Update Checker - 基于宿主命令的更新检查（桌面端）
 *
 * **传输方式**：升级检查与安装的**发起权在 Rust 端**——前端只 invoke 宿主命令
 * `check_for_update` / `install_update`，拿已验签的版本元数据（Rust 侧拉取 + minisign
 * 公钥验签），下载进度经 `app://update-progress` 事件回流。
 *
 * 迁移原因（前端零资源访问红线，AGENTS.md §6）：原先直接调 `@tauri-apps/plugin-updater`
 * 的 `check()`，等于让**前端发起网络请求**。该能力已随 `updater:default` 权限一并从
 * `capabilities/default.json` 撤除，Rust 运行期即拒（见 `capabilities_test.rs` 防回接锁）。
 *
 * 错误处理（票 04 / ADR 0030）：失败时**不**把原始错误暴露给 UI——
 * 状态置 `failed`，界面显示通用文案（`settings.about.checkFailed`），
 * 技术详情经 `logger.error` 落盘（release 下 error/warn 仍转发，见 frontendLogger）。
 * 本组件不再对外提供可渲染的裸错误字段（曾暴露 `errorMessage = e.message`）。
 */

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
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

/** 宿主命令 `check_for_update` 的返回形状（只含已验签元数据，不含 updater 句柄） */
export interface UpdateCheckResult {
  /** 是否存在新版本 */
  available: boolean
  /** 新版本号（available=false 时为 null） */
  version: string | null
  /** 发布日期（签名元数据原样透传） */
  date: string | null
}

/** 宿主事件 `app://update-progress` 的载荷 */
interface UpdateProgressPayload {
  downloaded: number
  content_length: number
  finished: boolean
}

const status = ref<UpdateStatus>('idle')
const downloadProgress = ref({ downloaded: 0, contentLength: 0 })

/**
 * 检查更新
 *
 * 返回契约：**有新版**才返回结果对象，**已是最新**返回 `null`（失败也返回 `null`）。
 * 调用方靠 `null` + `status` 区分三类结果（`!r && status==='latest'` → “已是最新版本”
 * toast；`!r && status==='failed'` → 失败 toast）。返回真值对象会让后者永不成立。
 */
async function checkForUpdate(): Promise<UpdateCheckResult | null> {
  status.value = 'checking'

  try {
    const result = await invoke<UpdateCheckResult>('check_for_update')

    if (!result.available) {
      status.value = 'latest'
      return null
    }
    status.value = 'available'
    return result
  } catch (e) {
    // 票 04（ADR 0030）：详情只进日志，UI 走 status='failed' + 通用文案
    logger.error('[update-checker] checkForUpdate failed:', e)
    status.value = 'failed'
    return null
  }
}

/** 下载并安装更新 */
async function downloadAndInstall(onProgress?: (downloaded: number, total: number) => void) {
  status.value = 'downloading'
  downloadProgress.value = { downloaded: 0, contentLength: 0 }

  // 进度事件订阅必须在 invoke 之前建立：Rust 侧下载一开始就发事件，晚订阅会丢首批
  let unlisten: (() => void) | null = null
  try {
    unlisten = await listen<UpdateProgressPayload>('app://update-progress', (event) => {
      const { downloaded, content_length, finished } = event.payload
      if (content_length > 0) {
        downloadProgress.value.contentLength = content_length
      }
      // Rust 侧发的是**累计**字节，直接赋值（迁移前 JS 侧是逐 chunk 累加）
      downloadProgress.value.downloaded = downloaded
      onProgress?.(downloaded, downloadProgress.value.contentLength)
      if (finished) {
        status.value = 'downloaded'
      }
    })

    // false = 当前无可用更新：**不是失败**，保持既有状态直接返回
    const started = await invoke<boolean>('install_update')
    if (!started) {
      status.value = 'available'
      return
    }

    status.value = 'installing'
    await relaunch()
  } catch (e) {
    // 票 05（ADR 0030）：检查失败同样不产生未处理 rejection，统一 failed 态 + 通用文案
    logger.error('[update-checker] downloadAndInstall failed:', e)
    status.value = 'failed'
  } finally {
    // 必须解除订阅：relaunch 失败 / 无更新 / 抛错三条路径都会走到，否则监听器累积泄漏
    unlisten?.()
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