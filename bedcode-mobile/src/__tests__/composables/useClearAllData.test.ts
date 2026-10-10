/**
 * useClearAllData 行为契约
 * -----------------------------------------------------------------------------
 * 被测契约（「清除所有数据」不可撤销，失败口径必须诚实）：
 * - C-CD1 已连接时先断开并停前台服务，再清本地——顺序反了会被仍在跑的
 *   WS 订阅把已清状态写回去
 * - C-CD2 清理面齐全：预设任务 / 连接历史 / 配对设备 / 会话配置 / 活动会话 /
 *   认证凭据 / 连接凭据 / localStorage 全清
 * - C-CD3 断连失败**不阻断**后续清理（用户目标是清数据，不是断连；
 *   阻断会留下凭据仍在盘上），但结果里必须标 disconnected=false
 * - C-CD4 清理失败不抛错、以 completed=false + error 如实返回，且**不重载**
 *   （重载会让用户以为已清干净）
 * - C-CD5 未连接时不调 disconnect（无意义调用），仍执行全部清理
 * - C-CD6 默认重载页面（调用方不传 reload 时）
 *
 * 替身只跨边界：连接/服务/预设任务 composable 与 Tauri 命令面。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref } from 'vue'

// ==================== 替身 ====================
//
// vi.mock 的工厂会被提升到文件顶部，其引用的变量必须用 vi.hoisted 一并提升，
// 否则工厂执行时 `spies` 还在暂时性死区（Cannot access before initialization）。

const { spies, state } = vi.hoisted(() => ({
  spies: {
    disconnect: vi.fn(),
    stopService: vi.fn(),
    clearConnectionHistory: vi.fn(),
    clearPairedDevices: vi.fn(),
    clearSessionConfigs: vi.fn(),
    clearActiveSessions: vi.fn(),
    clearCredentials: vi.fn(),
    clearAuthCredentials: vi.fn(),
    clearAllTasks: vi.fn(),
  },
  state: { isConnected: false as boolean },
}))

/** 未使用的占位：保留与真实 composable 同形，避免 mock 缺导出报形状错 */
const _unused = {
  disconnect: vi.fn(async () => {}),
  stopService: vi.fn(async () => {}),
  clearConnectionHistory: vi.fn(),
  clearPairedDevices: vi.fn(),
  clearSessionConfigs: vi.fn(),
  clearActiveSessions: vi.fn(),
  clearCredentials: vi.fn(),
  clearAuthCredentials: vi.fn(),
  clearAllTasks: vi.fn(),
  startService: vi.fn(async () => {}),
}

vi.mock('@/composables/useMobileConnection', () => ({
  useMobileConnection: () => ({
    isConnected: ref(state.isConnected),
    disconnect: spies.disconnect,
    clearConnectionHistory: spies.clearConnectionHistory,
    clearPairedDevices: spies.clearPairedDevices,
    clearSessionConfigs: spies.clearSessionConfigs,
    clearActiveSessions: spies.clearActiveSessions,
    clearCredentials: spies.clearCredentials,
  }),
}))

vi.mock('@/composables/useForegroundService', () => ({
  useForegroundService: () => ({
    stopService: spies.stopService,
    startService: _unused.startService,
  }),
}))

vi.mock('@/composables/useMobileCommands', () => ({
  clearAuthCredentials: spies.clearAuthCredentials,
}))

vi.mock('@/composables/usePresetTasks', () => ({
  clearAllTasks: spies.clearAllTasks,
}))

import { clearAllData } from '@/composables/useClearAllData'

/** localStorage.clear 的观察点（happy-dom 提供真 localStorage，这里包一层计数） */
let clearCalls = 0

beforeEach(() => {
  vi.clearAllMocks()
  clearCalls = 0
  state.isConnected = false
  localStorage.setItem('probe', 'x')
  const orig = localStorage.clear.bind(localStorage)
  vi.spyOn(Storage.prototype, 'clear').mockImplementation(() => {
    clearCalls += 1
    orig()
  })
})

describe('C-CD1 / C-CD5 断连与停服', () => {
  it('should_disconnectAndStopServiceFirst_when_connected', async () => {
    state.isConnected = true
    const order: string[] = []
    spies.disconnect.mockImplementation(async () => {
      order.push('disconnect')
    })
    spies.stopService.mockImplementation(async () => {
      order.push('stopService')
    })
    spies.clearConnectionHistory.mockImplementation(() => {
      order.push('clearHistory')
    })

    const result = await clearAllData({ reload: false })

    expect(order.slice(0, 3)).toEqual(['disconnect', 'stopService', 'clearHistory'])
    expect(result.disconnected).toBe(true)
    expect(result.completed).toBe(true)
  })

  it('should_notDisconnect_when_notConnected', async () => {
    state.isConnected = false
    const result = await clearAllData({ reload: false })

    // 无意义调用：未连接时调 disconnect 只会走一条空转分支
    expect(spies.disconnect).not.toHaveBeenCalled()
    expect(result.completed).toBe(true)
  })
})

describe('C-CD2 清理面齐全', () => {
  it('should_clearEveryDataStore_when_invoked', async () => {
    await clearAllData({ reload: false })

    expect(spies.clearAllTasks, '预设任务').toHaveBeenCalled()
    expect(spies.clearConnectionHistory, '连接历史').toHaveBeenCalled()
    expect(spies.clearPairedDevices, '配对设备').toHaveBeenCalled()
    expect(spies.clearSessionConfigs, '会话配置').toHaveBeenCalled()
    expect(spies.clearActiveSessions, '活动会话').toHaveBeenCalled()
    expect(spies.clearAuthCredentials, '认证凭据').toHaveBeenCalled()
    expect(spies.clearCredentials, '连接凭据').toHaveBeenCalled()
    expect(clearCalls, 'localStorage').toBeGreaterThan(0)
    expect(localStorage.getItem('probe'), 'localStorage 应被清空').toBeNull()
  })
})

describe('C-CD3 断连失败不阻断清理', () => {
  it('should_stillClearLocalData_when_disconnectThrows', async () => {
    state.isConnected = true
    spies.disconnect.mockRejectedValueOnce(new Error('socket stuck'))

    const result = await clearAllData({ reload: false })

    // 断不开连接也必须把本地数据清掉——否则凭据仍在盘上，比连接还重要
    expect(spies.clearAllTasks).toHaveBeenCalled()
    expect(spies.clearAuthCredentials).toHaveBeenCalled()
    expect(clearCalls).toBeGreaterThan(0)
    expect(result.disconnected, '断连失败必须如实标记').toBe(false)
    expect(result.completed, '清理本身仍应完成').toBe(true)
  })

  it('should_stillClearLocalData_when_stopServiceThrows', async () => {
    spies.stopService.mockRejectedValueOnce(new Error('service busy'))

    const result = await clearAllData({ reload: false })

    expect(spies.clearAllTasks).toHaveBeenCalled()
    expect(result.disconnected).toBe(false)
  })
})

describe('C-CD4 清理失败如实暴露', () => {
  it('should_returnFailureInsteadOfThrowing_when_cleanupThrows', async () => {
    spies.clearSessionConfigs.mockImplementationOnce(() => {
      throw new Error('db locked')
    })

    const result = await clearAllData({ reload: false })

    // 擦除中途失败若抛到 UI 只会留下「清了一半」的不可解释状态
    expect(result.completed).toBe(false)
    expect(result.error).toContain('db locked')
  })
})

describe('C-CD6 页面重载', () => {
  it('should_reloadPage_when_reloadNotSuppressed', async () => {
    const reload = vi.fn()
    vi.stubGlobal('location', { ...location, reload })

    await clearAllData()

    expect(reload, '默认应重载，让各模块从干净状态重建').toHaveBeenCalledTimes(1)
    vi.unstubAllGlobals()
  })

  it('should_notReload_when_reloadSuppressed', async () => {
    const reload = vi.fn()
    vi.stubGlobal('location', { ...location, reload })

    await clearAllData({ reload: false })

    expect(reload).not.toHaveBeenCalled()
    vi.unstubAllGlobals()
  })
})
