/**
 * 插件级全局配置（P3 思考模式）
 *
 * 读宿主 storage key `config`（与宿主配置页 pluginStorageGet 共用同一 key，
 * 见 SDK PLUGIN_CONFIG_STORAGE_KEY 约定）并合并默认值——宿主配置页保存的
 * 值可能缺项（旧版本无配置 / 手动改动 storage），必须逐字段归一化，
 * 非法枚举值回退默认，避免坏数据流入请求构建。
 */
import { ref } from 'vue'
import { PLUGIN_CONFIG_STORAGE_KEY } from '@bedcode/plugin-sdk-desktop'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { PluginConfig, ReasoningEffort, ThinkingMode, CodeLineHeight } from '../types'
import { DEFAULT_PLUGIN_CONFIG } from '../types'

const THINKING_MODES: ThinkingMode[] = ['default', 'enabled', 'disabled']
const REASONING_EFFORTS: ReasoningEffort[] = ['low', 'high', 'max']
const CODE_LINE_HEIGHTS: CodeLineHeight[] = ['compact', 'normal', 'relaxed']

export function usePluginConfig(context: PluginContext) {
  /** 当前生效配置（未加载/加载失败时即默认值，保证请求构建永远拿得到合法值） */
  const config = ref<PluginConfig>({ ...DEFAULT_PLUGIN_CONFIG })
  const loading = ref(false)

  /** 从宿主 storage 读取配置并合并默认值（未知键丢弃、非法值回退） */
  async function loadConfig(): Promise<void> {
    loading.value = true
    try {
      const saved = await context.storage.get<Partial<PluginConfig>>(PLUGIN_CONFIG_STORAGE_KEY)
      if (!saved || typeof saved !== 'object') return
      config.value = {
        thinkingMode: normalizeEnum(saved.thinkingMode, THINKING_MODES, DEFAULT_PLUGIN_CONFIG.thinkingMode),
        reasoningEffort: normalizeEnum(saved.reasoningEffort, REASONING_EFFORTS, DEFAULT_PLUGIN_CONFIG.reasoningEffort),
        showReasoning: typeof saved.showReasoning === 'boolean' ? saved.showReasoning : DEFAULT_PLUGIN_CONFIG.showReasoning,
        codeLineHeight: normalizeEnum(saved.codeLineHeight, CODE_LINE_HEIGHTS, DEFAULT_PLUGIN_CONFIG.codeLineHeight),
      }
    } catch (e) {
      // 读取失败保持默认值（配置缺失不阻断聊天），仅记录日志
      console.error('[AI Chatbox] Failed to load plugin config:', e)
    } finally {
      loading.value = false
    }
  }

  return { config, loading, loadConfig }
}

/** 枚举值归一化：不在白名单内（含 undefined/类型不符）一律回退默认 */
function normalizeEnum<T extends string>(value: unknown, whitelist: readonly T[], fallback: T): T {
  return whitelist.includes(value as T) ? (value as T) : fallback
}
