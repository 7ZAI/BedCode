/**
 * 业务设置读写编排（票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 数据面全部经宿主通用 KV 桥（`mobileApi.readAllSettings` / `writeSetting`）：
 * 插件持有「键、默认值、取值范围、UI」，宿主只做持久化（§5.1 B3 归属）。
 *
 * 失败口径：加载失败回落默认值并保留错误标记（设置页仍可用，不空白页）；
 * 写入失败不更新本地值——宁可让用户看到开关弹回去，也不要「看着生效其实没存」。
 */
import { inject, reactive, ref } from 'vue'
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

export function useAppSettings(provided?: PluginContext) {
  const context = provided ?? inject<PluginContext>('pluginContext')
  if (!context) throw new Error('[settings] pluginContext not provided')
  const ctx = context
  const api = getMobileApi()
  const logger = ctx.logger

  /** 当前值（键 → 值）；初始为默认值，加载后覆盖 */
  const values = reactive<Record<string, SettingValue>>(
    Object.fromEntries(ALL_SETTINGS.map((def) => [def.key, def.fallback])),
  )

  const loading = ref(false)
  const saving = ref(false)
  /** 最近一次加载/写入失败（用于页面提示；不阻断操作） */
  const error = ref('')

  function errText(e: unknown): string {
    return e instanceof Error ? e.message : String(e)
  }

  async function load(): Promise<void> {
    loading.value = true
    error.value = ''
    try {
      const rows = await api.readAllSettings()
      for (const def of ALL_SETTINGS) {
        const raw = rows[`${KV_PREFIX}${def.key}`]
        // 键不存在时保留默认值（不写入 undefined）
        if (raw === undefined) continue
        values[def.key] = normalizeSetting(def, raw)
      }
    } catch (e) {
      // 读取失败回落默认值继续可用——但必须可观测，且不得静默
      error.value = errText(e)
      logger.warn(`[settings] load failed, falling back to defaults: ${errText(e)}`)
    } finally {
      loading.value = false
    }
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
    saving.value = true
    error.value = ''
    try {
      await api.writeSetting(`${KV_PREFIX}${def.key}`, toSettingString(normalized))
      values[def.key] = normalized
    } catch (e) {
      // 写失败不更新本地值：让 UI 弹回原态，而不是假装已保存
      values[def.key] = normalizeSetting(def, values[def.key])
      error.value = errText(e)
      logger.error(`[settings] write failed: key=${def.key} ${errText(e)}`)
      throw e
    } finally {
      saving.value = false
    }
  }

  /**
   * 重置全部业务设置为默认值
   *
   * 逐项写入而非一次性批量：桥只有单键写面，批量语义由本域编排。
   * 任一项失败即中止并上抛（部分重置比全不重置更难解释），已写入的不回滚。
   */
  async function reset(): Promise<void> {
    saving.value = true
    error.value = ''
    try {
      for (const def of ALL_SETTINGS) {
        await api.writeSetting(`${KV_PREFIX}${def.key}`, toSettingString(def.fallback))
        values[def.key] = def.fallback
      }
      logger.info('[settings] business settings reset to defaults')
    } catch (e) {
      error.value = errText(e)
      logger.error(`[settings] reset failed: ${errText(e)}`)
      throw e
    } finally {
      saving.value = false
    }
  }

  return {
    values,
    loading,
    saving,
    error,
    load,
    set,
    reset,
    t: ctx.i18n.t.bind(ctx.i18n),
    dialogs: ctx.dialogs,
    logger,
  }
}