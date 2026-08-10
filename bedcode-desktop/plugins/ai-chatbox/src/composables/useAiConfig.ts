/**
 * AI 供应商配置管理
 *
 * providers CRUD + activeProvider/activeModel + 拉取模型列表 + 测试连接。
 * 持久化走宿主 storage（`apiProviders` / `activeProvider` / `activeModel`，
 * 与 v1 同机制）；`{dataDir}/providers.json` 由 Rust 侧 init 创建占位。
 * 供应商对象始终 camelCase 直传 Rust 命令（ApiProvider serde camelCase）。
 */
import { ref, computed } from 'vue'
import type { ApiProvider, ProviderPreset, ApiFormat } from '../types'
import { generateId } from '../types'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import { getI18n } from '@bedcode/plugin-sdk-desktop'

const STORAGE_PROVIDERS = 'apiProviders'
const STORAGE_ACTIVE_PROVIDER = 'activeProvider'
const STORAGE_ACTIVE_MODEL = 'activeModel'

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
    } catch (e) {
      console.error('[AI Chatbox] Failed to load config:', e)
    } finally {
      loading.value = false
    }
  }

  /** 规范化旧数据（v1 可能缺 apiFormat / activeModel 字段） */
  function normalizeProvider(p: Partial<ApiProvider>): ApiProvider {
    return {
      id: p.id || generateId(),
      name: p.name || 'Unnamed',
      apiKey: p.apiKey || '',
      baseUrl: p.baseUrl || '',
      apiFormat: (p.apiFormat as ApiFormat) || 'openai',
      models: p.models || [],
      activeModel: p.activeModel || (p.models && p.models[0]) || '',
    }
  }

  /** 持久化 providers 列表（同步 activeProviderId 有效性） */
  async function saveProviders(): Promise<void> {
    await context.storage.set(STORAGE_PROVIDERS, providers.value)
    if (activeProviderId.value && !providers.value.some(p => p.id === activeProviderId.value)) {
      activeProviderId.value = providers.value[0]?.id || ''
      await context.storage.set(STORAGE_ACTIVE_PROVIDER, activeProviderId.value)
    }
  }

  /** 新增供应商（从预设、自定义模板或完整表单对象；已带 id 的表单对象原样保留） */
  async function addProvider(preset?: ProviderPreset | ApiProvider): Promise<ApiProvider> {
    const hasId = preset && 'id' in preset && !!(preset as ApiProvider).id
    const provider: ApiProvider = hasId
      ? { ...(preset as ApiProvider) }
      : {
          id: generateId(),
          // 默认名取宿主 i18n（composable 禁用中文硬编码）；存翻译后文本以便
          // 列表/表单直接展示，语言切换后新创建的供应商才用新语言（既有行为）
          name: (preset as ProviderPreset)?.name || getI18n().global.t('desktop.plugin.aiChatbox.customProviders'),
          apiKey: '',
          baseUrl: (preset as ProviderPreset)?.baseUrl || '',
          apiFormat: 'openai',
          models: (preset as ProviderPreset)?.models ? [...(preset as ProviderPreset).models] : [],
          activeModel: (preset as ProviderPreset)?.models?.[0] || '',
        }
    providers.value.push(provider)
    await saveProviders()
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

  async function setActiveModel(model: string): Promise<void> {
    activeModel.value = model
    await context.storage.set(STORAGE_ACTIVE_MODEL, model)
    // 同步回供应商记录（持久化当前选择）
    if (activeProvider.value) {
      const updated = { ...activeProvider.value, activeModel: model }
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

  /** 拉取模型列表（真实 GET /models；失败抛错，由调用方回退预设） */
  async function fetchModels(provider: ApiProvider): Promise<string[]> {
    const result = await context.commands.execute('ai-chatbox.fetch-models', { provider })
    const models = result?.models
    if (!Array.isArray(models)) throw new Error('bad response')
    return models as string[]
  }

  /** 测试连接（非流式短请求）；成功返回回复文本，失败抛错 */
  async function testConnection(provider: ApiProvider): Promise<string> {
    const result = await context.commands.execute('ai-chatbox.chat-complete', {
      provider,
      messages: [{ role: 'user', content: 'ping' }],
    })
    return result?.content ?? ''
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
