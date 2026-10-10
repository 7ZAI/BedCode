/**
 * Dev Shell 宿主壳「最近使用」（与宿主 shell/composables/useShellRecent.ts 同构）
 * -----------------------------------------------------------------------------
 * 最近使用是平台侧的排序信号（首页「最近使用」区消费它），不是应用自己的状态，
 * 因此由壳持有。
 *
 * 持久化落在 localStorage：这是纯 UI 偏好，丢失只影响排序；隐私模式下写入失败
 * 按 warn 处理（不静默吞掉）。
 */

import { computed, ref, type ComputedRef } from 'vue'
import { logger } from '../logger'

const STORAGE_KEY = 'bedcode.devshell.shell.recentApps'
/** 保留上限：再多也不会被展示，留着只会让写入无限增长 */
const MAX_RECENT = 12

const recentIds = ref<string[]>(load())

function load(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((v): v is string => typeof v === 'string').slice(0, MAX_RECENT)
  } catch (e) {
    // 首次启动 / 隐私模式 / 旧格式损坏都不是故障，按空列表继续
    logger.warn(`recent apps load failed, fallback to empty: ${String(e)}`)
    return []
  }
}

function persist(ids: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids))
  } catch (e) {
    logger.warn(`recent apps persist failed (order may be lost): ${String(e)}`)
  }
}

/** 记录一次使用（同 id 提到最前，去重） */
function pushRecent(appId: string): void {
  if (!appId) return
  const next = [appId, ...recentIds.value.filter((id) => id !== appId)].slice(0, MAX_RECENT)
  recentIds.value = next
  persist(next)
}

/** 清空最近使用 */
function clearRecent(): void {
  recentIds.value = []
  persist([])
}

/** 最近使用（新 → 旧） */
export function useShellRecent(): {
  recentIds: ComputedRef<string[]>
  pushRecent: typeof pushRecent
  clearRecent: typeof clearRecent
} {
  return {
    recentIds: computed(() => recentIds.value),
    pushRecent,
    clearRecent,
  }
}

export { pushRecent, clearRecent }