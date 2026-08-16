/**
 * 插件设置
 *
 * get/set-settings 命令封装 + 目录选择（经 context.fileService.pickDirectory，
 * WASM 的 pick-download-dir 命令无法弹窗）。
 *
 * 注意：WASM set-settings 读取的是**顶层** roots/downloadDir/concurrency 字段
 * （见 rust commands::set_settings），并非契约文档里的嵌套 { settings } 对象；
 * 本 composable 按实际实现传参。get-settings 返回 snake_case download_dir，
 * 在此归一化为 camelCase 内部模型。
 */
import { ref, computed, type Ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Settings } from '../types'

export function useSettings(context: PluginContext) {
  const settings = ref<Settings>({
    roots: [],
    downloadDir: '',
    concurrency: 3,
    receivingPolicy: 'ask',
    approvalTimeoutSec: 60,
  }) as Ref<Settings>
  const loading = ref(false)

  /** 拉取设置并归一化 */
  async function load(): Promise<void> {
    loading.value = true
    try {
      const r = await context.commands.execute('file-transfer.get-settings', {})
      if (r) {
        const policy = r.receiving_policy ?? r.receivingPolicy ?? 'ask'
        settings.value = {
          roots: Array.isArray(r.roots) ? r.roots : [],
          downloadDir: r.download_dir ?? r.downloadDir ?? '',
          concurrency: typeof r.concurrency === 'number' ? r.concurrency : 3,
          receivingPolicy: ['ask', 'accept', 'reject'].includes(policy) ? policy : 'ask',
          approvalTimeoutSec:
            typeof r.approval_timeout_sec === 'number' ? r.approval_timeout_sec : 60,
        }
      }
    } catch (e) {
      console.error('[File Transfer] get-settings failed:', e)
    } finally {
      loading.value = false
    }
  }

  /** 持久化当前设置到 WASM（顶层字段传参） */
  async function save(): Promise<void> {
    await context.commands.execute('file-transfer.set-settings', {
      roots: settings.value.roots,
      downloadDir: settings.value.downloadDir,
      concurrency: settings.value.concurrency,
      receivingPolicy: settings.value.receivingPolicy,
      approvalTimeoutSec: settings.value.approvalTimeoutSec,
    })
  }

  /** 设置接收策略（v2；即时保存） */
  async function setReceivingPolicy(policy: Settings['receivingPolicy']): Promise<void> {
    settings.value = { ...settings.value, receivingPolicy: policy }
    await save()
  }

  /** 设置同意超时（v2，10–600 钳制；仅 ask 策略生效） */
  async function setApprovalTimeoutSec(secs: number): Promise<void> {
    const clamped = Math.min(600, Math.max(10, Math.round(secs)))
    settings.value = { ...settings.value, approvalTimeoutSec: clamped }
    await save()
  }

  /** 添加共享目录（弹系统目录选择器；去重后持久化，返回 null 表示用户取消） */
  async function addRoot(): Promise<string | null> {
    const dir = await context.fileService.pickDirectory()
    if (!dir) return null
    if (!settings.value.roots.includes(dir)) {
      settings.value = { ...settings.value, roots: [...settings.value.roots, dir] }
      await save()
    }
    return dir
  }

  /** 移除共享目录 */
  async function removeRoot(dir: string): Promise<void> {
    settings.value = {
      ...settings.value,
      roots: settings.value.roots.filter(r => r !== dir),
    }
    await save()
  }

  /** 选择下载目录（弹系统目录选择器） */
  async function pickDownloadDir(): Promise<string | null> {
    const dir = await context.fileService.pickDirectory()
    if (!dir) return null
    settings.value = { ...settings.value, downloadDir: dir }
    await save()
    return dir
  }

  /** 设置并发数（钳制 1..8，同时调用 set-concurrency 即时生效） */
  async function setConcurrency(n: number): Promise<void> {
    const clamped = Math.min(8, Math.max(1, Math.round(n)))
    settings.value = { ...settings.value, concurrency: clamped }
    await context.commands.execute('file-transfer.set-concurrency', { concurrency: clamped })
  }

  /** 是否已配置共享目录（空态判断用） */
  const hasRoots = computed(() => settings.value.roots.length > 0)

  return {
    settings,
    loading,
    hasRoots,
    load,
    save,
    addRoot,
    removeRoot,
    pickDownloadDir,
    setConcurrency,
    setReceivingPolicy,
    setApprovalTimeoutSec,
  }
}
