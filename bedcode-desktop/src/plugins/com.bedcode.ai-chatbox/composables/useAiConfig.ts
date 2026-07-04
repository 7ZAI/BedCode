/**
 * API Provider 配置管理
 *
 * 管理多个 OpenAI 兼容 API 配置的增删改查、活跃切换、预设模板
 */
import { ref, computed } from 'vue'
import type { ApiProvider, ProviderPreset } from '../types'
import { PROVIDER_PRESETS } from '../types'

/** 配置管理 composable */
export function useAiConfig(
  storageGet: (key: string) => Promise<any>,
  storageSet: (key: string, value: any) => Promise<void>,
) {
  const providers = ref<ApiProvider[]>([])
  const activeProviderName = ref('')
  const loading = ref(false)
  const showProviderManager = ref(false)

  /** 当前活跃的 provider */
  const activeProvider = computed<ApiProvider | undefined>(() =>
    providers.value.find(p => p.name === activeProviderName.value)
  )

  /** 是否已配置至少一个 provider */
  const hasProvider = computed(() => providers.value.length > 0)

  /** 从 storage 加载配置 */
  async function loadConfig(): Promise<void> {
    loading.value = true
    try {
      const savedProviders = await storageGet('apiProviders')
      if (savedProviders) {
        const parsed = typeof savedProviders === 'string' ? JSON.parse(savedProviders) : savedProviders
        providers.value = Array.isArray(parsed) ? parsed : []
      }

      const savedActive = await storageGet('activeProvider')
      activeProviderName.value = typeof savedActive === 'string' ? savedActive : ''

      if (!activeProviderName.value && providers.value.length > 0) {
        activeProviderName.value = providers.value[0].name
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
      await storageSet('activeProvider', activeProviderName.value)
    } catch (e) {
      console.error('[AI Chatbox] Failed to save config:', e)
    }
  }

  /** 添加 provider */
  async function addProvider(provider: ApiProvider): Promise<void> {
    if (providers.value.some(p => p.name === provider.name)) {
      throw new Error(`Provider "${provider.name}" 已存在`)
    }
    providers.value.push(provider)
    if (!activeProviderName.value) {
      activeProviderName.value = provider.name
    }
    await saveConfig()
  }

  /** 删除 provider */
  async function removeProvider(name: string): Promise<void> {
    providers.value = providers.value.filter(p => p.name !== name)
    if (activeProviderName.value === name) {
      activeProviderName.value = providers.value[0]?.name || ''
    }
    await saveConfig()
  }

  /** 更新 provider */
  async function updateProvider(oldName: string, provider: ApiProvider): Promise<void> {
    const index = providers.value.findIndex(p => p.name === oldName)
    if (index === -1) return
    providers.value[index] = provider
    if (activeProviderName.value === oldName) {
      activeProviderName.value = provider.name
    }
    await saveConfig()
  }

  /** 切换活跃 provider */
  async function setActiveProvider(name: string): Promise<void> {
    if (!providers.value.some(p => p.name === name)) return
    activeProviderName.value = name
    await saveConfig()
  }

  /** 从预设创建 provider（只填 API Key） */
  async function addFromPreset(preset: ProviderPreset, apiKey: string): Promise<void> {
    await addProvider({
      name: preset.name,
      apiKey,
      baseUrl: preset.baseUrl,
      model: preset.model,
    })
  }

  return {
    providers,
    activeProviderName,
    activeProvider,
    hasProvider,
    loading,
    showProviderManager,
    loadConfig,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    addFromPreset,
    PROVIDER_PRESETS,
  }
}
