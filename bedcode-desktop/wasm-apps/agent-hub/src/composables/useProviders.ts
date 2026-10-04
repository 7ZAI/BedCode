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
  FetchModelsResult,
  ImportProvidersResult,
  ProvidersDomainState,
  ProviderTarget,
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
  /** 模型查询 URL（可空字符串 = 手动输入模型列表） */
  modelsUrl?: string
  /** v2 中心凭据（明文仅在 save 命令在途，不落前端状态） */
  apiKey?: string
}

/** 模型列表查询载荷：URL 必填；key 优先用 apiKey，其次 presetId 的中心库已存 key */
export interface FetchModelsPayload {
  url: string
  presetId?: number
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
  /** 模型列表查询进行中（guest 内 http_fetch 阻塞，本地瞬态） */
  const fetchingModels = ref(false)

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
 * 应用预设到目标 CLI（可多目标：一次写多个 CLI，逐目标独立成败）
 *
 * keySpec 四选一（stored 中心库 / inline / source / none）；force 仅用于
 * claude 桥接冲突的二次确认（guest 顶层读取，与 key 分离）。
 * `codexEnvKey` 只在目标含 codex 时有意义：codex 配置里只写 env 变量名，
 * 不吃 key 值（纯 codex 应用时 guest 也会跳过 key 解析）。
 */
  async function applyProvider(
    id: number,
    targets: ProviderTarget[],
    targetName: string,
    keySpec: ApplyKeySpec,
    force = false,
    codexEnvKey = '',
  ): Promise<CommandResult<ApplyProviderResult | null>> {
    if (applying.value) return { status: 'busy' }
    if (targets.length === 0) {
      // 不发空目标请求（guest 会拒；发出去只会得到一条无意义的失败）
      return { status: 'error', error: new Error('no target selected') }
    }
    applying.value = true
    try {
      const data = await context.commands.execute('agent-hub.apply-provider', {
        id,
        targets,
        targetName,
        key: keySpec,
        force,
        codex: { envKey: codexEnvKey },
      })
      return { status: 'ok', data: (data ?? null) as ApplyProviderResult | null }
    } catch (e) {
      console.error('[Agent Hub] apply-provider failed', e)
      return { status: 'error', error: e }
    } finally {
      applying.value = false
    }
  }

  /**
   * 查询模型列表（guest 代发 GET，解析 data[]/models[]/根数组）
   *
   * 失败一律走 `status: 'error'`——空列表/形状不认识都不能当「这个供应商就是
   * 0 个模型」，否则会一路写进目标配置（pi 里表现为 `/model` 看不到模型）
   */
  async function fetchModels(
    payload: FetchModelsPayload,
  ): Promise<CommandResult<FetchModelsResult | null>> {
    if (fetchingModels.value) return { status: 'busy' }
    fetchingModels.value = true
    try {
      const data = await context.commands.execute('agent-hub.fetch-models', payload)
      const result = (data ?? null) as FetchModelsResult | null
      if (!result || !Array.isArray(result.models)) {
        // guest 回了空回执：当作失败处理，不假装查到了模型
        return { status: 'error', error: new Error('empty model list') }
      }
      // 条目校验（2026-10-04 OCR A-03）：只查数组/长度会让非字符串或空白条目
      // 漏进下游 `mergeModelIds` 的 `id.trim()` 直接 TypeError（addAllFetched
      // 崩溃）；未 trim 的 id 与已 trim 列表比对 → chip 激活态误判。与
      // 「unrecognized shape 必须 fail-visible」契约一致：任一条目不是非空
      // 字符串，整个回执拒绝，而不是悄悄丢条目。
      const trimmed = result.models.map((m) => (typeof m === 'string' ? m.trim() : ''))
      if (trimmed.length === 0 || trimmed.some((m) => !m)) {
        return { status: 'error', error: new Error('invalid model list entries') }
      }
      return { status: 'ok', data: { ...result, models: trimmed } }
    } catch (e) {
      console.error('[Agent Hub] fetch-models failed', e)
      return { status: 'error', error: e }
    } finally {
      fetchingModels.value = false
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
    fetchingModels,
    refresh,
    savePreset,
    deletePreset,
    importProviders,
    applyProvider,
    fetchModels,
  }
}
