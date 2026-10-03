/**
 * Agent Hub 供应商管理域编排（票据 05 / v2 中心凭据库）
 *
 * - 挂载时拉取状态（guest 组装：插件库 presets + claude 只读视图 + 导入/应用
 *   回执），此后 guest 每次变更全量 emit `plugin:agent-hub:providers` 覆盖
 * - CRUD / 导入 / 应用均为同步命令（纯 fs + 插件库，无异步进程）；本地瞬态
 *   busy 遮罩区分三类动作
 * - v2 key 中心存储：save-preset 可选带 `apiKey` 入中心凭据库（明文只在
 *   save 命令在途），状态永远只回掩码；apply 的 stored 模式不发明文
 *   （guest 现读库内 key），inline / source / none 语义不变
 */
import { onMounted, onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type {
  ApplyProviderResult,
  ApplyKeySpec,
  ImportProvidersResult,
  ProvidersDomainState,
  SavePresetResult,
} from '../types'

export type UseProvidersReturn = ReturnType<typeof useProviders>

/**
 * 命令结果判别联合：把「成功 / 被忽略 / 失败」三种结局显式分开
 *
 * 修复前四个命令统一返回 `T | null`，而 `null` 同时代表「命令抛错」「busy 忽略」
 * 「guest 空回执」三义，调用方只能一律 `if (result === null) return` 静默吞掉——
 * 表现为点保存/删除/应用后界面毫无反应、也无任何提示（违反 fail-visible 红线）。
 * 现在失败与忽略必须被调用方显式接住：失败给用户错误提示，忽略保持静默。
 */
export type CommandResult<T> =
  | { status: 'ok'; data: T }
  /** 同一动作进行中，本次调用被忽略——不是失败，不应弹错误 */
  | { status: 'busy' }
  | { status: 'error'; error: unknown }

/** 保存预设载荷（id 缺省 = 新建）；apiKey 缺省 = 保留既有、'' = 清空、非空 = 设置 */
export interface PresetPayload {
  id?: number
  name: string
  baseUrl: string
  apiStyle: string
  models: string[]
  /** v2 中心凭据（明文仅在 save 命令在途，不落前端状态） */
  apiKey?: string
}

export function useProviders(context: PluginContext) {
  const state = ref<ProvidersDomainState | null>(null)
  /** 导入进行中（同步命令，本地瞬态） */
  const importing = ref(false)
  /** 应用进行中（同步命令，本地瞬态） */
  const applying = ref(false)
  /** 保存/删除进行中（同步命令，本地瞬态） */
  const saving = ref(false)

  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-providers-state', {})
      state.value = (data?.state ?? null) as ProvidersDomainState | null
    } catch (e) {
      console.error('[Agent Hub] get-providers-state failed', e)
    }
  }

  /** 新建/更新预设；同名冲突由调用方按 nameExists 呈现 */
  async function savePreset(payload: PresetPayload): Promise<CommandResult<SavePresetResult | null>> {
    if (saving.value) return { status: 'busy' }
    saving.value = true
    try {
      const data = await context.commands.execute('agent-hub.save-preset', payload)
      return { status: 'ok', data: (data ?? null) as SavePresetResult | null }
    } catch (e) {
      console.error('[Agent Hub] save-preset failed', e)
      return { status: 'error', error: e }
    } finally {
      saving.value = false
    }
  }

  async function deletePreset(id: number): Promise<CommandResult<null>> {
    if (saving.value) return { status: 'busy' }
    saving.value = true
    try {
      await context.commands.execute('agent-hub.delete-preset', { id })
      return { status: 'ok', data: null }
    } catch (e) {
      console.error('[Agent Hub] delete-preset failed', id, e)
      return { status: 'error', error: e }
    } finally {
      saving.value = false
    }
  }

  /** 反向导入（pi/opencode → 预设 + key 掩码） */
  async function importProviders(): Promise<CommandResult<ImportProvidersResult | null>> {
    if (importing.value) return { status: 'busy' }
    importing.value = true
    try {
      const data = await context.commands.execute('agent-hub.import-providers', {})
      return { status: 'ok', data: (data ?? null) as ImportProvidersResult | null }
    } catch (e) {
      console.error('[Agent Hub] import-providers failed', e)
      return { status: 'error', error: e }
    } finally {
      importing.value = false
    }
  }

  /**
   * 应用预设到目标 CLI；keySpec 四选一（stored 中心库 / inline / source / none）；
   * force 仅用于 claude 桥接冲突的二次确认（guest 顶层读取，与 key 分离）
   */
  async function applyProvider(
    id: number,
    target: string,
    targetName: string,
    keySpec: ApplyKeySpec,
    force = false,
  ): Promise<CommandResult<ApplyProviderResult | null>> {
    if (applying.value) return { status: 'busy' }
    applying.value = true
    try {
      const data = await context.commands.execute('agent-hub.apply-provider', {
        id,
        target,
        targetName,
        key: keySpec,
        force,
      })
      return { status: 'ok', data: (data ?? null) as ApplyProviderResult | null }
    } catch (e) {
      console.error('[Agent Hub] apply-provider failed', e)
      return { status: 'error', error: e }
    } finally {
      applying.value = false
    }
  }

  const subscription = context.events.on(
    'plugin:agent-hub:providers',
    (payload: ProvidersDomainState) => {
      state.value = payload
    },
  )

  onMounted(() => {
    void refresh()
  })

  onUnmounted(() => {
    subscription.dispose()
  })

  return {
    state,
    importing,
    applying,
    saving,
    refresh,
    savePreset,
    deletePreset,
    importProviders,
    applyProvider,
  }
}
