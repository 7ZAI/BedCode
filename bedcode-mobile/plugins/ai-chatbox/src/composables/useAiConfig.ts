/**
 * API Provider 配置管理 (Mobile)
 *
 * 管理多个 API 供应商配置的增删改查、活跃切换、数据迁移
 */
import { ref, computed } from 'vue'
import type { ApiProvider, ProviderPreset, ApiFormat } from '../types'
import { PROVIDER_PRESETS, generateId } from '../types'

/** 配置管理 composable */
export function useAiConfig(
  storageGet: (key: string) => Promise<any>,
  storageSet: (key: string, value: any) => Promise<void>,
) {
  const providers = ref<ApiProvider[]>([])
  const activeProviderId = ref('')
  const activeModel = ref('')
  const loading = ref(false)
  const showProviderManager = ref(false)

  /** 当前活跃的 provider */
  const activeProvider = computed<ApiProvider | undefined>(() =>
    providers.value.find(p => p.id === activeProviderId.value)
  )

  /** 是否已配置至少一个 provider */
  const hasProvider = computed(() => providers.value.length > 0)

  /** 从 storage 加载配置（含旧数据迁移） */
  async function loadConfig(): Promise<void> {
    loading.value = true
    try {
      const savedProviders = await storageGet('apiProviders')
      if (savedProviders) {
        const parsed = typeof savedProviders === 'string' ? JSON.parse(savedProviders) : savedProviders
        const rawList = Array.isArray(parsed) ? parsed : []

        // 迁移旧格式：无 id/apiFormat/models 字段
        providers.value = rawList.map((p: any) => {
          if (!p.id) {
            return {
              id: generateId(),
              name: p.name || '',
              apiKey: p.apiKey || '',
              baseUrl: p.baseUrl || '',
              apiFormat: (p.apiFormat || 'openai') as ApiFormat,
              models: p.models || (p.model ? [p.model] : []),
              activeModel: p.activeModel || p.model || '',
            } as ApiProvider
          }
          return p as ApiProvider
        })

        // 迁移后立即保存
        if (rawList.some((p: any) => !p.id)) {
          await saveConfig()
        }
      }

      const savedActiveId = await storageGet('activeProvider')
      activeProviderId.value = typeof savedActiveId === 'string' ? savedActiveId : ''

      const savedActiveModel = await storageGet('activeModel')
      activeModel.value = typeof savedActiveModel === 'string' ? savedActiveModel : ''

      // 兼容：旧格式存的是 name 而非 id
      if (!activeProviderId.value && providers.value.length > 0) {
        activeProviderId.value = providers.value[0].id
      }

      // 同步 activeModel：确保它在当前 provider 的 models 列表中
      const current = activeProvider.value
      if (current && current.models.length > 0 && !current.models.includes(activeModel.value)) {
        activeModel.value = current.activeModel || current.models[0]
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load config:', e)
    } finally {
      loading.value = false
    }
  }

  /** 保存配置到 storage */
  async function saveConfig(): Promise<void> {
    try {
      await storageSet('apiProviders', JSON.stringify(providers.value))
      await storageSet('activeProvider', activeProviderId.value)
      await storageSet('activeModel', activeModel.value)
    } catch (e) {
      console.error('[AI Chatbox] Failed to save config:', e)
    }
  }

  /** 添加 provider */
  async function addProvider(provider: ApiProvider): Promise<void> {
    if (providers.value.some(p => p.name === provider.name)) {
      throw new Error('desktop.plugin.aiChatbox.providerExists')
    }
    providers.value.push(provider)
    if (!activeProviderId.value) {
      activeProviderId.value = provider.id
      activeModel.value = provider.activeModel || provider.models[0] || ''
    }
    await saveConfig()
  }

  /** 删除 provider */
  async function removeProvider(id: string): Promise<void> {
    providers.value = providers.value.filter(p => p.id !== id)
    if (activeProviderId.value === id) {
      activeProviderId.value = providers.value[0]?.id || ''
      const current = activeProvider.value
      activeModel.value = current?.activeModel || current?.models[0] || ''
    }
    await saveConfig()
  }

  /** 更新 provider */
  async function updateProvider(id: string, provider: ApiProvider): Promise<void> {
    const index = providers.value.findIndex(p => p.id === id)
    if (index === -1) return
    providers.value[index] = provider
    if (activeProviderId.value === id) {
      activeModel.value = provider.activeModel || provider.models[0] || ''
    }
    await saveConfig()
  }

  /** 切换活跃 provider */
  async function setActiveProvider(id: string): Promise<void> {
    if (!providers.value.some(p => p.id === id)) return
    activeProviderId.value = id
    const current = providers.value.find(p => p.id === id)
    activeModel.value = current?.activeModel || current?.models[0] || ''
    await saveConfig()
  }

  /** 切换活跃模型 */
  async function setActiveModel(modelId: string): Promise<void> {
    activeModel.value = modelId
    // 同步更新 provider 的 activeModel
    const current = activeProvider.value
    if (current) {
      current.activeModel = modelId
    }
    await saveConfig()
  }

  /** 从预设创建 provider */
  async function addFromPreset(preset: ProviderPreset, apiKey: string): Promise<void> {
    const provider: ApiProvider = {
      id: generateId(),
      name: preset.name,
      apiKey,
      baseUrl: preset.baseUrl,
      apiFormat: preset.apiFormat,
      models: [...preset.models],
      activeModel: preset.models[0] || '',
    }
    await addProvider(provider)
  }

  return {
    providers,
    activeProviderId,
    activeProvider,
    activeModel,
    hasProvider,
    loading,
    showProviderManager,
    loadConfig,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    setActiveModel,
    addFromPreset,
    PROVIDER_PRESETS,
  }
}
