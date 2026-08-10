/**
 * useAiConfig 单测（接缝 4）：providers CRUD、activeModel 同步、
 * 拉取模型落库、storage 恢复
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
  it('新增供应商：预设回填 + 首个自动设为 active', async () => {
    const { config } = setup()
    const p = await config.addProvider(makeProvider())

    expect(p.id).toBeTruthy()
    expect(p.baseUrl).toBe('https://api.deepseek.com/v1')
    expect(config.providers.value.length).toBe(1)
    expect(config.activeProviderId.value).toBe(p.id)
    expect(config.activeModel.value).toBe('deepseek-chat')
  })

  it('持久化：providers/active 写入 storage', async () => {
    const { mock, config } = setup()
    await config.addProvider(makeProvider())

    // 数组对象直接存储（v1 同机制）
    expect(mock.storageMap.get('apiProviders')).toHaveLength(1)
    expect(mock.storageMap.get('activeProvider')).toBe('p1')
  })

  it('更新供应商：activeModel 变更同步到当前供应商', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())

    const updated = { ...config.providers.value[0], models: ['a', 'b'], activeModel: 'b' }
    await config.updateProvider(updated)

    expect(config.providers.value[0].activeModel).toBe('b')
    expect(config.activeModel.value).toBe('b')
  })

  it('删除当前供应商：active 回退到剩余首个', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())
    await config.addProvider({ name: 'Qwen', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: ['qwen-turbo'] })

    await config.removeProvider('p1')
    expect(config.providers.value.length).toBe(1)
    expect(config.activeProviderId.value).toBe(config.providers.value[0].id)
  })

  it('切换模型：持久化 activeModel + 同步供应商记录', async () => {
    const { mock, config } = setup()
    await config.addProvider(makeProvider())
    await config.setActiveModel('deepseek-reasoner')

    expect(mock.storageMap.get('activeModel')).toBe('deepseek-reasoner')
    expect(config.providers.value[0].activeModel).toBe('deepseek-reasoner')
  })

  it('拉取模型：命令调用 + 落库', async () => {
    const { mock, config } = setup()
    const p = await config.addProvider(makeProvider())

    const models = await config.fetchModels(p)
    expect(models).toEqual(['model-a', 'model-b'])
    expect(mock.calls.find(c => c.command === 'ai-chatbox.fetch-models')).toBeTruthy()
  })

  it('测试连接：chat-complete 返回回复文本', async () => {
    const { config } = setup()
    const p = await config.addProvider(makeProvider())

    const reply = await config.testConnection(p)
    expect(reply).toBe('pong')
  })

  it('loadConfig：从 storage 恢复供应商与 active 状态', async () => {
    const mock = createMockContext()
    // 预置 storage（模拟上次会话遗留）
    mock.storageMap.set('apiProviders', [makeProvider()])
    mock.storageMap.set('activeProvider', 'p1')
    mock.storageMap.set('activeModel', 'deepseek-reasoner')

    const config = useAiConfig(mock.context)
    await config.loadConfig()

    expect(config.providers.value.length).toBe(1)
    expect(config.activeProviderId.value).toBe('p1')
    expect(config.activeModel.value).toBe('deepseek-reasoner')
  })

  it('buildRequestProvider：对话级 model 覆盖优先', async () => {
    const { config } = setup()
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')

    const req = config.buildRequestProvider('override-model')
    expect(req!.model).toBe('override-model')
    expect(req!.id).toBe('p1')
  })
})
