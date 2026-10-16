/**
 * 连接历史取数与编排（票 14）
 *
 * 取数红线（spec D2）：只经插件命令通道（`session.devices.history-*`）——
 * 禁止直调宿主 `list_connection_history` / `delete_connection_history`（宿主
 * 命令面是给宿主 UI 的兼容接缝）。
 *
 * 组织口径（spec D3「列表组织在插件」）：按连接日分组、统计成功/失败数、
 * 方式与结果的展示文案映射全在本模块；内核只给原始记录（`connection_history`
 * 表经 host-auth 记录面读出）。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

/** 连接历史条目（camelCase，与宿主 `ConnectionHistory` 序列化字段对应） */
export interface ConnectionHistoryEntry {
  id: number
  deviceId: string
  authMethod: string
  result: string
  address: string | null
  connectedAt: string
  disconnectedAt: string | null
}

/** 按连接日分组的展示模型 */
export interface ConnectionHistoryGroup {
  date: string
  entries: ConnectionHistoryEntry[]
}

/** 认证方式 → i18n key 后缀（未知方式兜底 unknown，与宿主同映射） */
const METHOD_KEY_SUFFIX: Record<string, string> = {
  pairing_code: 'pairingCode',
  qr: 'qr',
  biometric: 'biometric',
  jwt: 'jwt',
}

export function useConnectionHistory(context: PluginContext) {
  const entries = ref<ConnectionHistoryEntry[]>([])
  const isLoading = ref(false)

  const successCount = computed(() => entries.value.filter((e) => e.result === 'success').length)
  const failCount = computed(() => entries.value.length - successCount.value)

  /** 按连接日分组，保持原顺序（后端已按时间倒序） */
  const groups = computed<ConnectionHistoryGroup[]>(() => {
    const map = new Map<string, ConnectionHistoryEntry[]>()
    for (const entry of entries.value) {
      const key = dayKey(entry.connectedAt)
      const bucket = map.get(key)
      if (bucket) {
        bucket.push(entry)
      } else {
        map.set(key, [entry])
      }
    }
    return Array.from(map.entries()).map(([date, groupEntries]) => ({
      date,
      entries: groupEntries,
    }))
  })

  /** 连接日（本地时区的 YYYY-MM-DD；非法时间回退 unknown 文案） */
  function dayKey(timeStr: string): string {
    const date = new Date(timeStr)
    if (isNaN(date.getTime())) return 'unknown'
    const y = date.getFullYear()
    const m = String(date.getMonth() + 1).padStart(2, '0')
    const d = String(date.getDate()).padStart(2, '0')
    return `${y}-${m}-${d}`
  }

  /** 时分秒（本地时区；非法时间回退 unknown 文案） */
  function clockTime(timeStr: string): string {
    const date = new Date(timeStr)
    if (isNaN(date.getTime())) return 'unknown'
    return date.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
  }

  /** 认证方式 i18n key 后缀（未知方式 → unknown） */
  function methodKeySuffix(authMethod: string): string {
    return METHOD_KEY_SUFFIX[authMethod] ?? 'unknown'
  }

  /** 加载指定设备的连接历史 */
  async function load(deviceId: string): Promise<void> {
    isLoading.value = true
    try {
      const list = (await context.commands.execute('session.devices.history-list', {
        deviceId,
      })) as ConnectionHistoryEntry[] | undefined
      entries.value = Array.isArray(list) ? list : []
    } finally {
      isLoading.value = false
    }
  }

  /** 清空指定设备的连接历史（成功后本地列表同步清空） */
  async function clear(deviceId: string): Promise<void> {
    await context.commands.execute('session.devices.history-clear', { deviceId })
    entries.value = []
  }

  return {
    entries: entries as Ref<ConnectionHistoryEntry[]>,
    isLoading,
    successCount,
    failCount,
    groups: groups as ComputedRef<ConnectionHistoryGroup[]>,
    dayKey,
    clockTime,
    methodKeySuffix,
    load,
    clear,
  }
}

export type ConnectionHistory = ReturnType<typeof useConnectionHistory>
