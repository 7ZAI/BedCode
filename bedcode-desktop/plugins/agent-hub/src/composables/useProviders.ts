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
  async function savePreset(payload: PresetPayload): Promise<SavePresetResult | null> {
    if (saving.value) return null
    saving.value = true
    try {
      const data = await context.commands.execute('agent-hub.save-preset', payload)
      return (data ?? null) as SavePresetResult | null
    } catch (e) {
      console.error('[Agent Hub] save-preset failed', e)
      return null
    } finally {
      saving.value = false
    }
  }

  async function deletePreset(id: number) {
    if (saving.value) return
    saving.value = true
    try {
      await context.commands.execute('agent-hub.delete-preset', { id })
    } catch (e) {
      console.error('[Agent Hub] delete-preset failed', id, e)
    } finally {
      saving.value = false
    }
  }

  /** 反向导入（pi/opencode → 预设 + key 掩码） */
  async function importProviders(): Promise<ImportProvidersResult | null> {
    if (importing.value) return null
    importing.value = true
    try {
      const data = await context.commands.execute('agent-hub.import-providers', {})
      return (data ?? null) as ImportProvidersResult | null
    } catch (e) {
      console.error('[Agent Hub] import-providers failed', e)
      return null
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
  ): Promise<ApplyProviderResult | null> {
    if (applying.value) return null
    applying.value = true
    try {
      const data = await context.commands.execute('agent-hub.apply-provider', {
        id,
        target,
        targetName,
        key: keySpec,
        force,
      })
      return (data ?? null) as ApplyProviderResult | null
    } catch (e) {
      console.error('[Agent Hub] apply-provider failed', e)
      return null
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
