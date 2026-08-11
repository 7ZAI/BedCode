/**
 * usePluginConfig 单测（接缝 3）：storage 配置读取 + 默认值合并 + 非法值回退
 *
 * 宿主配置页保存的值可能缺项（旧版本无配置 / 手动改动 storage），
 * 读取侧必须逐字段归一化；坏数据不得流入请求构建。
 */
import { describe, it, expect } from 'vitest'
import { createMockContext } from './mockContext'
import { usePluginConfig } from '../composables/usePluginConfig'
import { DEFAULT_PLUGIN_CONFIG } from '../types'

function setup() {
  const mock = createMockContext()
  const pluginConfig = usePluginConfig(mock.context)
  return { mock, pluginConfig }
}

describe('usePluginConfig', () => {
  it('storage 无配置：全部默认值（thinkingMode default / effort high / showReasoning true）', async () => {
    const { pluginConfig } = setup()
    await pluginConfig.loadConfig()
    expect(pluginConfig.config.value).toEqual(DEFAULT_PLUGIN_CONFIG)
  })

  it('已存配置缺项：按默认值补齐（宿主配置页只保存改动过的字段）', async () => {
    const { mock, pluginConfig } = setup()
    mock.storageMap.set('config', { thinkingMode: 'enabled' })
    await pluginConfig.loadConfig()
    expect(pluginConfig.config.value).toEqual({
      thinkingMode: 'enabled',
      reasoningEffort: 'high',
      showReasoning: true,
    })
  })

  it('全量配置读回：原样生效', async () => {
    const { mock, pluginConfig } = setup()
    mock.storageMap.set('config', {
      thinkingMode: 'disabled',
      reasoningEffort: 'max',
      showReasoning: false,
    })
    await pluginConfig.loadConfig()
    expect(pluginConfig.config.value).toEqual({
      thinkingMode: 'disabled',
      reasoningEffort: 'max',
      showReasoning: false,
    })
  })

  it('非法枚举值 / 类型不符：回退默认（坏数据不流入请求构建）', async () => {
    const { mock, pluginConfig } = setup()
    mock.storageMap.set('config', {
      thinkingMode: 'bogus',
      reasoningEffort: 42,
      showReasoning: 'yes',
    })
    await pluginConfig.loadConfig()
    expect(pluginConfig.config.value).toEqual(DEFAULT_PLUGIN_CONFIG)
  })

  it('storage 读取抛错：保持默认值不抛错（配置缺失不阻断聊天）', async () => {
    const mock = createMockContext({
      commands: {},
    })
    mock.context.storage.get = async () => {
      throw new Error('storage unavailable')
    }
    const pluginConfig = usePluginConfig(mock.context)
    await expect(pluginConfig.loadConfig()).resolves.toBeUndefined()
    expect(pluginConfig.config.value).toEqual(DEFAULT_PLUGIN_CONFIG)
  })

  it('未加载时即为默认值（请求构建永远拿得到合法配置）', () => {
    const { pluginConfig } = setup()
    expect(pluginConfig.config.value).toEqual(DEFAULT_PLUGIN_CONFIG)
  })
})
