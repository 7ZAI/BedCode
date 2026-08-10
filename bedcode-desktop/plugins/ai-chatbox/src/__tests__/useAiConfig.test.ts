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

  it('从预设模板添加：写入 presetId（首个自动激活）', async () => {
    const { config } = setup()
    const p = await config.addProvider({
      id: 'qwen',
      name: '通义千问 (Qwen)',
      baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
      models: ['qwen-turbo'],
    })

    expect(p.presetId).toBe('qwen')
    expect(config.providers.value[0].presetId).toBe('qwen')
    // 首个供应商仍自动激活（既有逻辑保留）
    expect(config.activeProviderId.value).toBe(p.id)
  })

  it('新增供应商不自动激活（首个除外）：不打断当前对话的激活供应商', async () => {
    const { config } = setup()
    const first = await config.addProvider(makeProvider())
    const second = await config.addProvider(makeProvider({
      id: 'p2',
      name: 'Qwen',
      baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
      models: ['qwen-turbo'],
    }))

    expect(config.providers.value.length).toBe(2)
    expect(config.activeProviderId.value).toBe(first.id)
    expect(config.activeProviderId.value).not.toBe(second.id)
    expect(config.activeModel.value).toBe('deepseek-chat')
  })

  it('已带 presetId 的表单对象：原样保留（编辑/保存路径）', async () => {
    const { config } = setup()
    const p = await config.addProvider(makeProvider({ presetId: 'deepseek' }))
    expect(p.presetId).toBe('deepseek')
    expect(config.providers.value[0].presetId).toBe('deepseek')
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
    await config.addProvider(makeProvider({
      id: 'p2',
      name: 'Qwen',
      baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
      models: ['qwen-turbo'],
      activeModel: 'qwen-turbo',
    }))

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

  it('loadConfig：旧数据无 presetId 归一化不崩溃、不强制补默认值', async () => {
    const mock = createMockContext()
    const legacy = makeProvider()
    delete legacy.presetId
    mock.storageMap.set('apiProviders', [legacy])
    mock.storageMap.set('activeProvider', 'p1')

    const config = useAiConfig(mock.context)
    await config.loadConfig()

    expect(config.providers.value.length).toBe(1)
    // 无 presetId → 保持 undefined（UI 走首字母头像兜底）
    expect(config.providers.value[0].presetId).toBeUndefined()
    expect(config.activeProviderId.value).toBe('p1')
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
