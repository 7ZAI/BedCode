/**
 * 插件设置核心逻辑 (Mobile)
 *
 * 经 `file-transfer.get-settings` / `set-settings` 读写，WASM 侧持久化到 storage。
 * 共享目录经 SAF 系统选择器选择（SettingsSection）或手动输入绝对路径；
 * 下载目录为只读展示（下载固定落系统 AppDownloadsDir）。
 */
import { ref } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { Settings } from '../types'

/** 并发数上限（与 WASM Queue 一致） */
export const CONCURRENCY_MAX = 8

/** 将 WASM get-settings 返回（snake_case download_dir）归一化为 camelCase */
function mapWireSettings(raw: any): Settings {
  return {
    roots: Array.isArray(raw?.roots) ? raw.roots : [],
    downloadDir: raw?.download_dir ?? raw?.downloadDir ?? '',
    concurrency: raw?.concurrency ?? 3,
  }
}

export function useSettings(context: PluginContext) {
  const settings = ref<Settings>({
    roots: [],
    downloadDir: '',
    concurrency: 3,
  })
  const loading = ref(false)

  /** 加载设置（含首次拉取） */
  async function load(): Promise<void> {
    loading.value = true
    try {
      const data = await context.commands.execute('file-transfer.get-settings', {})
      settings.value = mapWireSettings(data)
    } catch (e) {
      console.error('[File Transfer] get-settings failed:', e)
    } finally {
      loading.value = false
    }
  }

  /** 追加共享目录（手动输入绝对路径）
   *
   * 返回结果原因供 UI 精确提示：ok（已保存并挂载）/ duplicate（路径已存在）/
   * failed（保存或挂载失败）。路径规范化（去尾部分隔符）避免同目录误判重复。
   */
  async function addRoot(path: string): Promise<'ok' | 'duplicate' | 'failed'> {
    // 规范化：去首尾空白 + 尾部路径分隔符（避免同目录不同写法误判重复）
    const trimmed = path.trim().replace(/[\\/]+$/, '')
    if (!trimmed) return 'failed'
    if (settings.value.roots.includes(trimmed)) return 'duplicate'
    const next = [...settings.value.roots, trimmed]
    return (await persist({ roots: next })) ? 'ok' : 'failed'
  }

  /** 移除共享目录 */
  async function removeRoot(path: string): Promise<boolean> {
    const next = settings.value.roots.filter(r => r !== path)
    return persist({ roots: next })
  }

  /** 设置并发数（1–8） */
  async function setConcurrency(n: number): Promise<boolean> {
    const clamped = Math.min(Math.max(Math.round(n), 1), CONCURRENCY_MAX)
    return persist({ concurrency: clamped })
  }

  /** 写入 WASM 并同步本地状态（挂载失败时 set-settings 返回错误 → false） */
  async function persist(patch: Partial<Settings>): Promise<boolean> {
    try {
      await context.commands.execute('file-transfer.set-settings', {
        roots: patch.roots ?? settings.value.roots,
        downloadDir: patch.downloadDir ?? settings.value.downloadDir,
        concurrency: patch.concurrency ?? settings.value.concurrency,
      })
      settings.value = { ...settings.value, ...patch }
      return true
    } catch (e) {
      console.error('[File Transfer] set-settings failed:', e)
      return false
    }
  }

  return {
    settings,
    loading,
    load,
    addRoot,
    removeRoot,
    setConcurrency,
  }
}
