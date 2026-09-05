/**
 * 插件设置 — host-peer 契约版
 *
 * 共享目录由宿主持久化（SharedDirDto：id/name/kind/path），添加 = 系统
 * 多目录选择器（一次可加多个）→ host mount-local；下载目录经 pick-download-dir
 * （系统选择器 + set_download_dir）；接收策略/超时经 set-settings。
 */
import { ref, computed, type Ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Settings } from '../types'

/** 宿主 SharedDirDto → 前端条目（保留 id 供移除寻址、path 供完整路径展示） */
export interface RootItem {
  id: string
  name: string
  path: string
}

export function useSettings(context: PluginContext) {
  const settings = ref<Settings>({
    roots: [],
    downloadDir: '',
    concurrency: 1,
    receivingPolicy: 'ask',
    approvalTimeoutSec: 60,
    encryption: false,
  }) as Ref<Settings>
  /** roots 带条目 id（宿主 DTO），与 Settings.roots（展示名列表）并行维护 */
  const rootItems = ref<RootItem[]>([]) as Ref<RootItem[]>
  const loading = ref(false)

  /** 拉取设置并归一化 */
  async function load(): Promise<void> {
    loading.value = true
    try {
      const r = await context.commands.execute('file-transfer.get-settings', {})
      if (r) {
        const policy = r.policy_mode ?? r.policyMode ?? 'ask'
        const normalized =
          policy === 'always_accept' ? 'accept' : policy === 'always_deny' ? 'reject' : 'ask'
        const rawRoots = Array.isArray(r.roots) ? r.roots : []
        rootItems.value = rawRoots.map((x: any) => {
          const name = (x?.name ?? x?.path ?? x?.id ?? '') as string
          // path 为展示真源（完整路径）；旧数据缺 path 时退化用 name
          const path = typeof x?.path === 'string' && x.path ? x.path : name
          return { id: x?.id ?? '', name, path }
        })
        settings.value = {
          ...settings.value,
          downloadDir: typeof r.download_dir === 'string' ? r.download_dir : (r.downloadDir ?? ''),
          receivingPolicy: normalized as Settings['receivingPolicy'],
          approvalTimeoutSec:
            typeof r.ask_timeout_sec === 'number'
              ? r.ask_timeout_sec
              : (typeof r.askTimeoutSecs === 'number' ? r.askTimeoutSecs : 60),
          encryption: r.encryption ?? r.encryption_enabled ?? false,
        }
      }
    } catch (e) {
      console.error('[File Transfer] get-settings failed:', e)
    } finally {
      loading.value = false
    }
  }

  /** 设置接收策略（即时保存） */
  async function setReceivingPolicy(policy: Settings['receivingPolicy']): Promise<void> {
    await context.commands.execute('file-transfer.set-settings', {
      receivingPolicy: policy,
      approvalTimeoutSec: settings.value.approvalTimeoutSec,
    })
    settings.value = { ...settings.value, receivingPolicy: policy }
  }

  /** 设置同意超时（10–600 钳制；仅 ask 策略生效） */
  async function setApprovalTimeoutSec(secs: number): Promise<void> {
    const clamped = Math.min(600, Math.max(10, Math.round(secs)))
    await context.commands.execute('file-transfer.set-settings', {
      receivingPolicy: settings.value.receivingPolicy,
      approvalTimeoutSec: clamped,
    })
    settings.value = { ...settings.value, approvalTimeoutSec: clamped }
  }

  /** 设置发送加密开关（即时保存） */
  async function setEncryption(enabled: boolean): Promise<void> {
    await context.commands.execute('file-transfer.set-settings', { encryption: enabled })
    settings.value = { ...settings.value, encryption: enabled }
  }

  /** 添加共享目录（系统多目录选择器，一次可添加多个 → 注册表 + set-shared-roots；
   * 用户取消返回空数组；兼容旧单条 {id,name,path} 契约） */
  async function addRoot(): Promise<string[]> {
    try {
      const r = await context.commands.execute('file-transfer.mount-local', {})
      await load()
      if (Array.isArray(r?.added)) {
        return r.added
          .map((x: any) => (typeof x?.path === 'string' ? x.path : ''))
          .filter(Boolean)
      }
      return typeof r?.path === 'string' && r.path ? [r.path] : []
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (!msg.includes('cancelled')) console.error('[File Transfer] mount-local failed:', e)
      return []
    }
  }

  /** 移除共享目录（按条目 id） */
  async function removeRoot(id: string): Promise<void> {
    await context.commands.execute('file-transfer.update-roots', { remove: id })
    rootItems.value = rootItems.value.filter((r) => r.id !== id)
  }

  /** 选择下载目录（插件命令内含系统选择器 + 持久化；取消返回 null） */
  async function pickDownloadDir(): Promise<string | null> {
    try {
      const result = await context.commands.execute('file-transfer.pick-download-dir', {})
      if (result?.cancelled) return null
      if (result?.path) {
        settings.value = { ...settings.value, downloadDir: result.path }
        return result.path as string
      }
      return null
    } catch (e) {
      console.error('[File Transfer] pick-download-dir failed:', e)
      return null
    }
  }

  /** 是否已配置共享目录（空态判断用） */
  const hasRoots = computed(() => rootItems.value.length > 0)

  return {
    settings,
    rootItems,
    loading,
    hasRoots,
    load,
    addRoot,
    removeRoot,
    pickDownloadDir,
    setReceivingPolicy,
    setApprovalTimeoutSec,
    setEncryption,
  }
}
