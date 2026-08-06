/**
 * Plugin Configuration Helpers
 *
 * SDK 侧配置约定：统一 storage key + 声明式配置助手。
 * 插件运行时读写配置走 context.storage，与宿主配置页共享 key。
 */

import type { PluginConfiguration, ConfigProperty } from './types'

/**
 * 插件配置在 storage 中的统一键名
 *
 * 插件运行时：context.storage.get(PLUGIN_CONFIG_STORAGE_KEY)
 * 宿主配置页：pluginStorageGet(pluginId, PLUGIN_CONFIG_STORAGE_KEY)
 * 两者共享同一 key，保证数据一致
 */
export const PLUGIN_CONFIG_STORAGE_KEY = 'config'

/**
 * 声明式构建插件配置（manifest contributes.configuration）
 *
 * 用途：插件开发者在代码中声明配置 schema，与 plugin.json 中保持一致。
 * 实际 manifest 中的 configuration 以 plugin.json 为单一真源。
 *
 * @example
 * ```ts
 * const config = defineConfiguration('My Plugin Settings', {
 *   apiKey: { type: 'string', title: 'API Key', description: 'Your API key' },
 *   maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
 *   debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
 * })
 * ```
 */
export function defineConfiguration(
  title: string,
  properties: Record<string, Omit<ConfigProperty, 'title'> & { title?: string }>
): PluginConfiguration {
  const props: Record<string, ConfigProperty> = {}
  for (const [key, value] of Object.entries(properties)) {
    props[key] = {
      type: value.type,
      title: value.title ?? key,
      description: value.description,
      default: value.default,
      enum: value.enum,
    }
  }
  return { title, properties: props }
}
