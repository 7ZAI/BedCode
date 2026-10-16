/**
 * 业务设置读写编排（票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 数据面全部经宿主通用 KV 桥（`mobileApi.readAllSettings` / `writeSetting`）：
 * 插件持有「键、默认值、取值范围、UI」，宿主只做持久化（§5.1 B3 归属）。
 *
 * 失败口径：加载失败回落默认值并保留错误标记（设置页仍可用，不空白页）；
 * 写入失败不更新本地值——宁可让用户看到开关弹回去，也不要「看着生效其实没存」。
 *
 * **共享状态（本文件是应用内唯一真源）**：AppRoot 与设置页各自调用一次
 * `useAppSettings()`，状态必须互相可见——否则设置页改了主色，运行面根节点的
 * `data-palette` 读不到新值，表现为「设置了不生效」（2026-10-16 修）。
 * 因此 `values/loading/saving/error` 提升为模块级单例（与宿主
 * `useMobileSettings` 的模块单例同一形态）；首个实例惰性绑定上下文，
 * 后续实例复用同一份状态，load 经模块级 Promise 幂等（并发只读一次 KV）。
 * 测试隔离走 `vi.resetModules()` 重建模块，不提供生产测试钩子。
 */
import { inject, reactive, ref } from 'vue'
import type { Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'
import {
  ALL_SETTINGS,
  SETTINGS_BY_KEY,
  normalizeSetting,
  toSettingString,
} from './settingsModel'

export type SettingValue = boolean | number | string

/** 宿主 KV 的键前缀（与宿主 useMobileSettings 落库口径一致） */
const KV_PREFIX = 'mobile.'

interface AppSettingsState {
  /** 首个实例绑定的插件上下文（logger / i18n / dialogs 由此取） */
  context: PluginContext
  /** 当前值（键 → 值）；初始为默认值，加载后覆盖 */
  values: Record<string, SettingValue>
  loading: Ref<boolean>
  saving: Ref<boolean>
  /** 最近一次加载/写入失败（用于页面提示；不阻断操作） */
  error: Ref<string>
  /** 幂等加载的模块级 Promise（并发 load 只读一次 KV） */
  loadPromise: Promise<void> | null
}

/** 模块级单例：null = 尚未初始化（首个 useAppSettings 惰性建立） */
let sharedState: AppSettingsState | null = null

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

/** 建立/复用共享状态。首个实例绑定上下文；`vi.resetModules()` 重建模块即恢复未初始化 */
function ensureState(provided?: PluginContext): AppSettingsState {
  if (sharedState) return sharedState
  const context = provided ?? inject<PluginContext>('pluginContext')
  if (!context) throw new Error('[settings] pluginContext not provided')
  sharedState = {
    context,
    values: reactive<Record<string, SettingValue>>(
      Object.fromEntries(ALL_SETTINGS.map((def) => [def.key, def.fallback])),
    ),
    loading: ref(false),
    saving: ref(false),
    error: ref(''),
    loadPromise: null,
  }
  return sharedState
}

export function useAppSettings(provided?: PluginContext) {
  const state = ensureState(provided)
  const ctx = state.context
  const api = getMobileApi()
  const logger = ctx.logger

  async function load(): Promise<void> {
    if (state.loadPromise) return state.loadPromise
    state.loadPromise = (async () => {
      state.loading.value = true
      state.error.value = ''
      try {
        const rows = await api.readAllSettings()
        for (const def of ALL_SETTINGS) {
          const raw = rows[`${KV_PREFIX}${def.key}`]
          // 键不存在时保留默认值（不写入 undefined）
          if (raw === undefined) continue
          state.values[def.key] = normalizeSetting(def, raw)
        }
      } catch (e) {
        // 读取失败回落默认值继续可用——但必须可观测，且不得静默
        state.error.value = errText(e)
        logger.warn(`[settings] load failed, falling back to defaults: ${errText(e)}`)
      } finally {
        state.loading.value = false
      }
    })()
    return state.loadPromise
  }

  /**
   * 写单项
   *
   * 先归一再落库：越界值被夹回区间，非法值回落默认——避免把坏值写进 KV。
   */
  async function set(key: string, value: unknown): Promise<void> {
    const def = SETTINGS_BY_KEY[key]
    if (!def) {
      logger.warn(`[settings] unknown setting key ignored: ${key}`)
      return
    }
    const normalized = normalizeSetting(def, value)
    state.saving.value = true
    state.error.value = ''
    try {
      await api.writeSetting(`${KV_PREFIX}${def.key}`, toSettingString(normalized))
      state.values[def.key] = normalized
    } catch (e) {
      // 写失败不更新本地值：让 UI 弹回原态，而不是假装已保存
      state.values[def.key] = normalizeSetting(def, state.values[def.key])
      state.error.value = errText(e)
      logger.error(`[settings] write failed: key=${def.key} ${errText(e)}`)
      throw e
    } finally {
      state.saving.value = false
    }
  }

  /**
   * 重置全部业务设置为默认值
   *
   * 逐项写入而非一次性批量：桥只有单键写面，批量语义由本域编排。
   * 任一项失败即中止并上抛（部分重置比全不重置更难解释），已写入的不回滚。
   */
  async function reset(): Promise<void> {
    state.saving.value = true
    state.error.value = ''
    try {
      for (const def of ALL_SETTINGS) {
        await api.writeSetting(`${KV_PREFIX}${def.key}`, toSettingString(def.fallback))
        state.values[def.key] = def.fallback
      }
      logger.info('[settings] business settings reset to defaults')
    } catch (e) {
      state.error.value = errText(e)
      logger.error(`[settings] reset failed: ${errText(e)}`)
      throw e
    } finally {
      state.saving.value = false
    }
  }

  return {
    values: state.values,
    loading: state.loading,
    saving: state.saving,
    error: state.error,
    load,
    set,
    reset,
    t: ctx.i18n.t.bind(ctx.i18n),
    dialogs: ctx.dialogs,
    logger,
  }
}