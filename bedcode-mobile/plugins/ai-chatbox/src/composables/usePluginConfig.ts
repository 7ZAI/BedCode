/**
 * 插件级全局配置（P3 思考模式）
 *
 * 读宿主 storage key `config` 并合并默认值——宿主配置页保存的值可能缺项
 * （旧版本无配置 / 手动改动 storage），必须逐字段归一化，非法枚举值回退默认，
 * 避免坏数据流入请求构建。
 *
 * 注：移动 SDK 未导出 PLUGIN_CONFIG_STORAGE_KEY 常量（桌面 SDK config.ts 有），
 * 此处本地定义同一约定值 'config'，与桌面端 storage key 保持一致。
 */
import { ref } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { PluginConfig, ReasoningEffort, ThinkingMode, CodeTheme } from '../types'
import {
  CODE_FONT_SIZE_MAX,
  CODE_FONT_SIZE_MIN,
  CODE_LINE_HEIGHT_MAX,
  CODE_LINE_HEIGHT_MIN,
  DEFAULT_CODE_LINE_HEIGHT,
  DEFAULT_PLUGIN_CONFIG,
} from '../types'

/** 插件配置 storage key（与宿主配置页 pluginStorageGet 共用，见桌面 SDK 约定） */
const PLUGIN_CONFIG_STORAGE_KEY = 'config'

const THINKING_MODES: ThinkingMode[] = ['default', 'enabled', 'disabled']
const REASONING_EFFORTS: ReasoningEffort[] = ['low', 'high', 'max']
const CODE_THEMES: CodeTheme[] = ['auto', 'light', 'dark', 'github-light', 'github-dark', 'dracula']

/** 旧版行距枚举 → 数字（v1 历史数据平滑迁移，取桌面端档位观感） */
const LEGACY_LINE_HEIGHTS: Record<string, number> = {
  compact: 1.35,
  normal: 1.6,
  relaxed: 1.8,
}

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
        codeLineHeight: normalizeLineHeight(saved.codeLineHeight),
        codeFontSize: normalizeFontSize(saved.codeFontSize),
        codeTheme: normalizeEnum(saved.codeTheme, CODE_THEMES, DEFAULT_PLUGIN_CONFIG.codeTheme),
      }
    } catch (e) {
      // 读取失败保持默认值（配置缺失不阻断聊天），仅记录日志
      console.error('[AI Chatbox] Failed to load plugin config:', e)
    } finally {
      loading.value = false
    }
  }

  /** 整表保存（移动端配置弹层用；桌面端由宿主配置页 pluginStorageSet 落盘） */
  async function saveConfig(next: PluginConfig): Promise<void> {
    config.value = { ...next }
    try {
      await context.storage.set(PLUGIN_CONFIG_STORAGE_KEY, { ...next })
    } catch (e) {
      // 保存失败不阻断本次会话内生效（仅持久化丢失），记录日志
      console.error('[AI Chatbox] Failed to save plugin config:', e)
    }
  }

  return { config, loading, loadConfig, saveConfig }
}

/** 枚举值归一化：不在白名单内（含 undefined/类型不符）一律回退默认 */
function normalizeEnum<T extends string>(value: unknown, whitelist: readonly T[], fallback: T): T {
  return whitelist.includes(value as T) ? (value as T) : fallback
}

/** 数字归一化：非有限数 / 超出 [MIN, MAX] 范围一律回退默认（桌面配置页可输入任意值） */
function normalizeFontSize(value: unknown): number {
  return typeof value === 'number' &&
    Number.isFinite(value) &&
    value >= CODE_FONT_SIZE_MIN &&
    value <= CODE_FONT_SIZE_MAX
    ? value
    : DEFAULT_PLUGIN_CONFIG.codeFontSize
}

/** 行距归一化：旧版枚举字符串映射为数字；数字夹取到 [0.5, 2]（保留一位小数） */
function normalizeLineHeight(value: unknown): number {
  if (typeof value === 'string') {
    const mapped = LEGACY_LINE_HEIGHTS[value]
    return mapped !== undefined ? mapped : DEFAULT_CODE_LINE_HEIGHT
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return Math.round(Math.min(Math.max(value, CODE_LINE_HEIGHT_MIN), CODE_LINE_HEIGHT_MAX) * 10) / 10
  }
  return DEFAULT_CODE_LINE_HEIGHT
}
