/**
 * useAiConfig 单测（接缝 4）：CRUD / activeModel 同步 / 拉取模型落库
 */
import { describe, it, expect } from 'vitest'
import { createMockContext, makeProvider } from './mockContext'
import { useAiConfig } from '../composables/useAiConfig'

function setup() {
  const mock = createMockContext()
  const config = useAiConfig(mock.context)
  return { mock, config }
}

describe('useAiConfig', () => {
  it('新增供应商：写入 storage + 首个供应商自动设为 active', async () => {
    const { mock, config } = setup()
    const p = await config.addProvider(makeProvider())

    expect(config.providers.value.length).toBe(1)
    expect(config.activeProviderId.value).toBe(p.id)
    expect(config.activeProvider.value?.name).toBe('DeepSeek')
    // storage 持久化（数组序列化）
    expect(mock.storageMap.get('apiProviders')).toHaveLength(1)
    expect(mock.storageMap.get('activeProvider')).toBe(p.id)
  })

  it('更新供应商：activeModel 跟随当前供应商同步', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())
    await config.setActiveModel('deepseek-reasoner')

    expect(config.activeModel.value).toBe('deepseek-reasoner')
    expect(config.activeProvider.value?.activeModel).toBe('deepseek-reasoner')
  })

  it('删除供应商：active 引用自动回退', async () => {
    const { config } = setup()
    const p1 = await config.addProvider(makeProvider({ id: 'p1', name: 'A' }))
    await config.addProvider(makeProvider({ id: 'p2', name: 'B' }))
    await config.setActiveProvider('p2')

    await config.removeProvider('p2')
    expect(config.providers.value.length).toBe(1)
    expect(config.activeProviderId.value).toBe('p1')
    expect(config.activeModel.value).toBe('deepseek-chat')
    void p1
  })

  it('拉取模型：命令返回模型列表', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())

    const models = await config.fetchModels(config.activeProvider.value!)
    expect(models).toEqual(['model-a', 'model-b'])
  })

  it('测试连接：命令返回回复文本', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())

    const reply = await config.testConnection(config.activeProvider.value!)
    expect(reply).toBe('pong')
  })

  it('loadConfig：从 storage 恢复配置并规范化旧数据', async () => {
    const { mock, config } = setup()
    // v1 旧数据：缺 apiFormat / activeModel
    mock.storageMap.set('apiProviders', [
      { id: 'old', name: 'Old', apiKey: 'sk', baseUrl: 'https://x/v1', models: ['m1'] },
    ])
    mock.storageMap.set('activeProvider', 'old')

    await config.loadConfig()

    expect(config.providers.value.length).toBe(1)
    const p = config.providers.value[0]
    expect(p.apiFormat).toBe('openai')
    expect(p.activeModel).toBe('m1')
    expect(config.activeProviderId.value).toBe('old')
  })
})
