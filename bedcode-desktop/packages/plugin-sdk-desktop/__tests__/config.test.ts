/**
 * SDK 配置助手单测
 *
 * defineConfiguration 把插件开发者的声明式 schema 规整为
 * manifest contributes.configuration 结构：title 缺省回退到属性 key。
 */
import { describe, it, expect } from 'vitest'
import { PLUGIN_CONFIG_STORAGE_KEY, defineConfiguration } from '../src/config'

describe('PLUGIN_CONFIG_STORAGE_KEY', () => {
  it('固定为 config —— 与宿主配置页 pluginStorageGet 共享同一 key', () => {
    expect(PLUGIN_CONFIG_STORAGE_KEY).toBe('config')
  })
})

describe('defineConfiguration', () => {
  it('title 透传，properties 保留 type/default/description', () => {
    const config = defineConfiguration('My Plugin Settings', {
      apiKey: { type: 'string', title: 'API Key', description: 'Your API key' },
      maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
      debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
    })

    expect(config.title).toBe('My Plugin Settings')
    expect(config.properties.apiKey).toEqual({
      type: 'string',
      title: 'API Key',
      description: 'Your API key',
    })
    expect(config.properties.maxRetries).toEqual({
      type: 'number',
      title: 'Max Retries',
      default: 3,
    })
    expect(config.properties.debugMode).toEqual({
      type: 'boolean',
      title: 'Debug Mode',
      default: false,
    })
  })

  it('属性未给 title → 回退为属性 key', () => {
    const config = defineConfiguration('S', {
      apiKey: { type: 'string' },
    })
    expect(config.properties.apiKey.title).toBe('apiKey')
  })

  it('enum 透传（string 属性的可选值列表）', () => {
    const config = defineConfiguration('S', {
      theme: { type: 'string', title: 'Theme', enum: ['light', 'dark'] },
    })
    expect(config.properties.theme.enum).toEqual(['light', 'dark'])
  })

  it('未提供的可选字段不落入结果对象（值为 undefined，JSON 序列化后消失）', () => {
    const config = defineConfiguration('S', {
      apiKey: { type: 'string', title: 'Key' },
    })
    // 直接比较忽略 undefined 值
    expect(config.properties.apiKey).toEqual({ type: 'string', title: 'Key' })
    // 写入 plugin.json 的序列化形态：无 description/default/enum 键
    expect(JSON.parse(JSON.stringify(config.properties.apiKey))).toEqual({
      type: 'string',
      title: 'Key',
    })
  })
})
