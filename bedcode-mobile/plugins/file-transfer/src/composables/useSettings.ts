/**
 * 插件设置核心逻辑 (Mobile) — host-peer 契约版
 *
 * 共享目录条目由宿主持久化（SAF 树授权 + 免授权特殊条目，builtin 不可移除），
 * 添加 = 弹 SAF 目录树选择器（host peer_add_shared_directory）；接收策略
 * （ask/accept/reject）与同意超时经 host set_receive_policy 写入。
 * 并发数与下载目录为移动端固定值（只读展示/隐藏）。
 */
import { ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { Settings, SharedRoot, ReceivingPolicy } from '../types'

/** 宿主 SharedDirDto → 前端 SharedRoot */
function mapWireRoot(raw: any): SharedRoot {
  const builtin = raw?.builtin === true
  return {
    id: raw?.id ?? '',
    kind: builtin ? 'private_downloads' : 'saf',
    name: raw?.name ?? '',
    documentId: raw?.treeUri ?? raw?.tree_uri ?? '',
    authorized: true,
  }
}

function mapWireSettings(raw: any): Settings {
  const policy = raw?.policy_mode ?? raw?.policyMode ?? 'ask'
  const normalized: ReceivingPolicy =
    policy === 'always_accept' ? 'accept' : policy === 'always_deny' ? 'reject' : 'ask'
  return {
    roots: Array.isArray(raw?.roots) ? raw.roots.map(mapWireRoot) : [],
    downloadDir: raw?.download_dir ?? raw?.downloadDir ?? 'MediaStore/Downloads',
    concurrency: 1,
    receivingPolicy: normalized,
    approvalTimeoutSec: raw?.ask_timeout_sec ?? raw?.askTimeoutSec ?? 60,
    encryption: raw?.encryption ?? raw?.encryption_enabled ?? false,
  }
}

export function useSettings(context: PluginContext) {
  const settings = ref<Settings>({
    roots: [],
    downloadDir: 'MediaStore/Downloads',
    concurrency: 1,
    receivingPolicy: 'ask',
    approvalTimeoutSec: 60,
    encryption: false,
  })
  const loading = ref(false)

  /** 加载设置（宿主读取） */
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

  /**
   * 追加共享目录：弹系统目录树选择器（授权由宿主完成）
   * ok / cancelled / failed / unsupported
   */
  async function addRoot(): Promise<'ok' | 'duplicate' | 'cancelled' | 'failed' | 'unsupported'> {
    try {
      await context.commands.execute('file-transfer.mount-local', {})
    } catch (e) {
      const msg = String(e)
      if (msg.includes('cancelled')) return 'cancelled'
      if (msg.includes('not supported') || msg.includes('unsupported')) return 'unsupported'
      console.error('[File Transfer] mount-local failed:', e)
      return 'failed'
    }
    await load()
    // 宿主对重复条目幂等（同 URI 复用），此处不再前端判重
    return 'ok'
  }

  /** 移除共享目录条目（builtin 特殊条目由宿主拒绝移除） */
  async function removeRoot(id: string): Promise<boolean> {
    try {
      const result = await context.commands.execute('file-transfer.update-roots', { remove: id })
      if (result?.removed === false) return false
    } catch (e) {
      console.error('[File Transfer] remove root failed:', e)
      return false
    }
    settings.value = { ...settings.value, roots: settings.value.roots.filter((r) => r.id !== id) }
    return true
  }

  /** v2 设置接收策略（ask/accept/reject） */
  /** 设置发送加密开关（即时保存） */
  async function setEncryption(enabled: boolean): Promise<boolean> {
    try {
      await context.commands.execute('file-transfer.set-settings', { encryption: enabled })
      settings.value = { ...settings.value, encryption: enabled }
      return true
    } catch (e) {
      console.error('[File Transfer] set encryption failed:', e)
      return false
    }
  }

  async function setReceivingPolicy(policy: ReceivingPolicy): Promise<boolean> {
    try {
      await context.commands.execute('file-transfer.set-settings', {
        receivingPolicy: policy,
        approvalTimeoutSec: settings.value.approvalTimeoutSec,
      })
      settings.value = { ...settings.value, receivingPolicy: policy }
      return true
    } catch (e) {
      console.error('[File Transfer] set receiving policy failed:', e)
      return false
    }
  }

  /** v2 设置同意超时（秒，10–600，仅 ask 策略生效） */
  async function setApprovalTimeout(secs: number): Promise<boolean> {
    const clamped = Math.min(Math.max(Math.round(secs), 10), 600)
    try {
      await context.commands.execute('file-transfer.set-settings', {
        receivingPolicy: settings.value.receivingPolicy,
        approvalTimeoutSec: clamped,
      })
      settings.value = { ...settings.value, approvalTimeoutSec: clamped }
      return true
    } catch (e) {
      console.error('[File Transfer] set approval timeout failed:', e)
      return false
    }
  }

  return {
    settings,
    loading,
    load,
    addRoot,
    removeRoot,
    setReceivingPolicy,
    setApprovalTimeout,
    setEncryption,
  }
}
