/**
 * AI 供应商配置管理
 *
 * providers CRUD + activeProvider/activeModel + 拉取模型列表 + 测试连接。
 * 持久化走宿主 storage（`apiProviders` / `activeProvider` / `activeModel`，
 * 与 v1 同机制）；`{dataDir}/providers.json` 由 Rust 侧 init 创建占位。
 * 供应商对象始终 camelCase 直传 Rust 命令（ApiProvider serde camelCase）。
 */
import { ref, computed } from 'vue'
import type { ApiProvider, ProviderPreset, ApiStyle } from '../types'
import { generateId } from '../types'
import { buildCompleteRequest, buildModelsRequest, getAdapter, parseModelsResponse } from '../adapters/registry'
import { isValidBaseUrl } from '../adapters/utils'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

const STORAGE_PROVIDERS = 'apiProviders'
const STORAGE_ACTIVE_PROVIDER = 'activeProvider'
const STORAGE_ACTIVE_MODEL = 'activeModel'

/** 跨供应商模型选择键分隔符（模型名可跨供应商重名，选择键必须以供应商限定） */
const MODEL_KEY_SEP = '::'

/** 拼接跨供应商模型选择键：`${providerId}::${model}`（输入框模型选择器选项 value） */
export function modelKey(providerId: string, model: string): string {
  return providerId + MODEL_KEY_SEP + model
}

/** 解析跨供应商模型选择键；非法键返回 null（防御：无法解析的选择直接忽略） */
export function parseModelKey(key: string): { providerId: string; model: string } | null {
  const idx = key.indexOf(MODEL_KEY_SEP)
  if (idx <= 0 || idx + MODEL_KEY_SEP.length >= key.length) return null
  return { providerId: key.slice(0, idx), model: key.slice(idx + MODEL_KEY_SEP.length) }
}

export function useAiConfig(context: PluginContext) {
  const providers = ref<ApiProvider[]>([])
  const activeProviderId = ref('')
  const activeModel = ref('')
  const loading = ref(false)

  const activeProvider = computed(() =>
    providers.value.find(p => p.id === activeProviderId.value) || null
  )

  const hasProvider = computed(() => providers.value.length > 0)

  /** 从 storage 加载配置 */
  async function loadConfig(): Promise<void> {
    loading.value = true
    try {
      const [rawProviders, activeId, model] = await Promise.all([
        context.storage.get<string>('apiProviders'),
        context.storage.get<string>('activeProvider'),
        context.storage.get<string>('activeModel'),
      ])
      if (rawProviders) {
        const parsed = typeof rawProviders === 'string' ? JSON.parse(rawProviders) : rawProviders
        if (Array.isArray(parsed)) {
          providers.value = parsed.map(normalizeProvider)
        }
      }
      activeProviderId.value = activeId || (providers.value[0]?.id ?? '')
      if (!activeModel.value) {
        activeModel.value = model || providers.value[0]?.activeModel || providers.value[0]?.models[0] || ''
      }
      // 恢复的 activeModel 若不属于当前供应商（该供应商已删除或模型列表变更），
      // 回退到供应商记录/首个模型，避免模型选择器显示无效选中
      const restored = activeProvider.value
      if (restored && activeModel.value && !restored.models.includes(activeModel.value)) {
        activeModel.value = restored.activeModel || restored.models[0] || ''
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load config:', e)
    } finally {
      loading.value = false
    }
  }

  /** 规范化旧数据（v1 可能缺 apiStyle / activeModel；v2 旧数据可能缺 presetId，保持 undefined 走默认头像） */
  function normalizeProvider(p: Partial<ApiProvider>): ApiProvider {
    return {
      id: p.id || generateId(),
      name: p.name || 'Unnamed',
      apiKey: p.apiKey || '',
      baseUrl: p.baseUrl || '',
      apiStyle: normalizeApiStyle(p),
      models: p.models || [],
      activeModel: p.activeModel || (p.models && p.models[0]) || '',
      presetId: p.presetId,
    }
  }

  /** 协议方言归一化：旧数据 apiFormat 键映射到 apiStyle（缺失/未知一律按 openai 处理） */
  function normalizeApiStyle(p: Partial<ApiProvider>): ApiStyle {
    const legacy = (p as { apiFormat?: unknown }).apiFormat
    const raw = p.apiStyle ?? legacy
    return raw === 'anthropic' || raw === 'gemini' || raw === 'custom' ? raw : 'openai'
  }

  /** 持久化 providers 列表（同步 activeProviderId 有效性） */
  async function saveProviders(): Promise<void> {
    await context.storage.set(STORAGE_PROVIDERS, providers.value)
    if (activeProviderId.value && !providers.value.some(p => p.id === activeProviderId.value)) {
      activeProviderId.value = providers.value[0]?.id || ''
      await context.storage.set(STORAGE_ACTIVE_PROVIDER, activeProviderId.value)
    }
  }

  /** 判别表单对象：ApiProvider 必含 activeModel（表单保存/normalize 均保证），
      ProviderPreset 永不含——两者现在都有 id 字段，不能再用 id 判别 */
  function isApiProvider(p: ProviderPreset | ApiProvider): p is ApiProvider {
    return 'activeModel' in p
  }

  /** 新增供应商（从预设模板、自定义模板或完整表单对象；表单对象原样保留） */
  async function addProvider(preset?: ProviderPreset | ApiProvider): Promise<ApiProvider> {
    const isFormObject = preset != null && isApiProvider(preset)
    const provider: ApiProvider = isFormObject
      ? { ...(preset as ApiProvider) }
      : {
          id: generateId(),
          // 自定义模板名称留空，由用户填写（composable 不持有中文硬编码）
          name: (preset as ProviderPreset)?.name || '',
          apiKey: '',
          baseUrl: (preset as ProviderPreset)?.baseUrl || '',
          apiStyle: 'openai',
          models: (preset as ProviderPreset)?.models ? [...(preset as ProviderPreset).models] : [],
          activeModel: (preset as ProviderPreset)?.models?.[0] || '',
          // 从预设模板创建时写入模板 id（自定义/旧数据保持 undefined）
          presetId: (preset as ProviderPreset)?.id,
        }
    providers.value.push(provider)
    await saveProviders()
    // 仅首个供应商自动激活；后续新增不打断当前对话的激活供应商
    if (!activeProviderId.value) {
      await setActiveProvider(provider.id)
    }
    return provider
  }

  /** 更新供应商（含模型/activeModel 变更） */
  async function updateProvider(provider: ApiProvider): Promise<void> {
    const idx = providers.value.findIndex(p => p.id === provider.id)
    if (idx === -1) return
    providers.value[idx] = { ...provider }
    await saveProviders()
    // activeModel 变更跟随当前供应商
    if (activeProviderId.value === provider.id) {
      activeModel.value = provider.activeModel || provider.models[0] || ''
      await context.storage.set(STORAGE_ACTIVE_MODEL, activeModel.value)
    }
  }

  /** 删除供应商（同时清理 active 引用） */
  async function removeProvider(id: string): Promise<void> {
    providers.value = providers.value.filter(p => p.id !== id)
    await saveProviders()
    if (activeProviderId.value === id) {
      activeProviderId.value = providers.value[0]?.id || ''
      activeModel.value = providers.value[0]?.activeModel || providers.value[0]?.models[0] || ''
      await context.storage.set(STORAGE_ACTIVE_PROVIDER, activeProviderId.value)
      await context.storage.set(STORAGE_ACTIVE_MODEL, activeModel.value)
    }
  }

  async function setActiveProvider(id: string): Promise<void> {
    activeProviderId.value = id
    await context.storage.set(STORAGE_ACTIVE_PROVIDER, id)
    const p = providers.value.find(x => x.id === id)
    if (p) {
      activeModel.value = p.activeModel || p.models[0] || ''
      await context.storage.set(STORAGE_ACTIVE_MODEL, activeModel.value)
    }
  }

  /**
   * 选择模型（跨供应商）：key 为 `${providerId}::${model}` 复合键；
   * 模型属于其他供应商时先切换激活供应商，再持久化选择（多供应商模型混选）
   */
  async function setActiveModel(key: string): Promise<void> {
    const parsed = parseModelKey(key)
    if (!parsed) return
    if (parsed.providerId !== activeProviderId.value) {
      await setActiveProvider(parsed.providerId)
    }
    activeModel.value = parsed.model
    await context.storage.set(STORAGE_ACTIVE_MODEL, parsed.model)
    // 同步回供应商记录（持久化当前选择）
    if (activeProvider.value) {
      const updated = { ...activeProvider.value, activeModel: parsed.model }
      await updateProvider(updated)
    }
  }

  /** 构造发给 Rust 的 provider 载荷（camelCase + 对话级 model 覆盖） */
  function buildRequestProvider(overrideModel?: string): ApiProvider | null {
    const p = activeProvider.value
    if (!p) return null
    return {
      ...p,
      model: overrideModel || activeModel.value || p.activeModel || p.models[0] || '',
    }
  }

  /** 拉取模型列表（经适配层构建请求；失败抛错，由调用方回退预设） */
  async function fetchModels(provider: ApiProvider): Promise<string[]> {
    // 与 sendMessage 同源的 baseUrl 校验前移：非法地址不发请求（宿主错误晦涩）
    if (!isValidBaseUrl(provider.baseUrl)) {
      throw new Error('invalid base url')
    }
    const request = buildModelsRequest(provider)
    const result = await context.commands.execute('ai-chatbox.fetch-models', { request })
    const status = Number(result?.status ?? 0)
    const body = String(result?.body ?? '')
    if (status !== 200) {
      throw new Error(`API error ${status}: ${body}`)
    }
    return parseModelsResponse(provider.apiStyle, body)
  }

  /** 测试连接（非流式短请求）；成功返回回复文本，失败抛错 */
  async function testConnection(provider: ApiProvider): Promise<string> {
    // 同上：非法 baseUrl 直接拦截，避免透传到宿主的晦涩错误
    if (!isValidBaseUrl(provider.baseUrl)) {
      throw new Error('invalid base url')
    }
    const request = buildCompleteRequest(provider, [{ role: 'user', content: 'ping' }])
    const result = await context.commands.execute('ai-chatbox.chat-complete', { request })
    const status = Number(result?.status ?? 0)
    const body = String(result?.body ?? '')
    if (status !== 200) {
      throw new Error(`API error ${status}: ${body}`)
    }
    return getAdapter(provider.apiStyle).parseCompleteResponse(body)
  }

  return {
    providers,
    activeProviderId,
    activeProvider,
    activeModel,
    hasProvider,
    loading,
    loadConfig,
    addProvider,
    updateProvider,
    removeProvider,
    setActiveProvider,
    setActiveModel,
    buildRequestProvider,
    fetchModels,
    testConnection,
  }
}
