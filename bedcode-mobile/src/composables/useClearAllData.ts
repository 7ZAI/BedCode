/**
 * 清除所有数据（设备级数据擦除）
 * -----------------------------------------------------------------------------
 * 票 2026-10-10：补回票 2026-10-09 阶段 B 随旧 `SettingsView` 删除而丢失的
 * 「清除所有数据」action（spec §1.6 回归项，本票必须补回）。
 *
 * **为什么这个动作留宿主而不下沉 wasm app**（与业务设置项的归属裁决相反）：
 * 业务设置项（自动重连 / 通知 / 终端上限…）是**产品事实**，真源归应用（§5.1 B3 修正）。
 * 但「擦除本机全部数据」是**设备生命周期动作**，它的清理对象里包含
 *   · 设备入场凭据（认证中心托管，ADR 0033）
 *   · 宿主连接态单例（配对设备 / 会话配置 / 活动会话）
 *   · 宿主 localStorage（跨应用共享，不属任何单个应用）
 * 这些都**不属于任何应用的业务事实**，且按 §8「凭据零过境」+ ADR 0031 fail-closed，
 * 插件**不得**持有或擦除认证凭据。把它放进应用设置页等于让插件伸手够宿主安全面——
 * 那是 §5.1 的越线，不是归属问题。
 *
 * 因此：应用设置页只管「重置业务设置」（`useAppSettings.reset`），本 composable
 * 承担「擦除设备数据」，入口挂在宿主壳的设置屏危险区。
 *
 * 执行序不可随意调整：先停连接与前台服务，再清本地，最后 reload——
 * 顺序反了会让仍在跑的 WS 订阅把已清的状态又写回去。
 */

import { logger } from '@/utils/frontendLogger'
import { useMobileConnection } from './useMobileConnection'
import { useForegroundService } from './useForegroundService'
import { clearAuthCredentials } from './useMobileCommands'
import { clearAllTasks } from './usePresetTasks'

/** 各阶段清理动作的结果——逐项记录，失败不吞但也不中断后续清理 */
export interface ClearAllDataResult {
  /** 断开连接 / 停前台服务是否成功（失败不阻断后续清理） */
  disconnected: boolean
  /** 是否走到最后的页面重载（false = 中途抛错） */
  completed: boolean
  /** 失败原因（可读，直接展示） */
  error?: string
}

/**
 * 擦除本机数据。
 *
 * 幂等性：重复调用安全（各清理面本身幂等）。不抛错——失败以返回值暴露，
 * 由调用方决定提示口径；擦除动作中途失败若抛到 UI 只会留下「清了一半」的
 * 不可解释状态，返回值 + 日志更利于排查。
 *
 * @param reload 清理完成后是否重载页面。测试传 false 避免 happy-dom 卸载整个环境。
 */
export async function clearAllData({ reload = true }: { reload?: boolean } = {}): Promise<ClearAllDataResult> {
  const connection = useMobileConnection()
  const { stopService } = useForegroundService()
  let disconnected = true

  // 1. 先断连 + 停前台服务：仍在跑的 WS 订阅会把连接历史 / 活动会话写回去
  try {
    if (connection.isConnected.value) {
      await connection.disconnect()
    }
    await stopService()
  } catch (e) {
    // 失败不阻断后续清理：断不开连接也应当把本地数据清掉，
    // 否则用户会被「连不上所以清不掉」卡住，且旧凭据仍在盘上
    disconnected = false
    logger.warn('[ClearAllData] disconnect/stopService failed, continuing cleanup:', e)
  }

  // 2~5. 本地数据清理：逐项独立，单项失败不阻断其余（与旧实现同口径）
  try {
    clearAllTasks()
    connection.clearConnectionHistory()
    connection.clearPairedDevices()
    connection.clearSessionConfigs()
    connection.clearActiveSessions()
    clearAuthCredentials()
    connection.clearCredentials()
    // localStorage 全清：宿主连接态缓存 / 界面偏好 / 各应用经宿主 KV 落的业务设置
    // 都在同一命名空间下，无法只清一部分而不留下互相矛盾的残留
    localStorage.clear()

    if (reload) {
      location.reload()
    }
    return { disconnected, completed: true }
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    logger.error(`[ClearAllData] cleanup failed: ${message}`)
    return { disconnected, completed: false, error: message }
  }
}