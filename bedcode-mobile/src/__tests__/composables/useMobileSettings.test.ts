/**
 * useMobileSettings 单测：设置持久化命令面适配（spec 2026-10-14-mobile-legacy-frontend-adaptation D3）
 *
 * 被测契约（来源 = spec D3 + 实现分支）：
 * - C-1 命令改名（正向）：改动设置项触发 saveSettings → 后端持久化 invoke 的命令名
 *   必须为 `set_db_setting_mobile`（后端 2026-10-10 能力域集中化改名，旧名
 *   `set_db_setting` 已退役），且 key 带 `mobile.` 前缀
 * - C-2 命令改名（反向护栏）：同一路径累计调用的命令集合里**不得**出现退役旧名
 *   `set_db_setting`（出现即适配失效，防回接）
 * - C-3 失败隔离（回归护栏）：单次 invoke reject 被 catch 吞掉，不阻断后续设置项
 *   的持久化（既有行为，guard 防回退改动）
 *
 * 被测实现：`useMobileSettings` 模块级单例 + `watch(settings, saveSettings, {deep:true})`——
 * 改动 settings.value 即触发 saveSettings（行为入口与真实调用方 SettingsView 一致）。
 *
 * mock 边界：仅 mock 跨进程 invoke（Tauri IPC）；store 用真 pinia（非 mock）。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

vi.mock('@/utils/frontendLogger', () => ({
  logger: { log: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

import { useMobileSettings } from '@/composables/useMobileSettings'

/** 按命令名取累计调用数组（含参数） */
function callsOf(command: string) {
  return mockInvoke.mock.calls.filter(([c]) => c === command)
}

describe('useMobileSettings 设置持久化命令面适配（spec D3）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockInvoke.mockResolvedValue(undefined)
    localStorage.clear()
  })

  it('C-1 正向：改动设置项 → 持久化命令名为 set_db_setting_mobile，key 带 mobile. 前缀', async () => {
    const { settings } = useMobileSettings()
    // 触发 deep watch → saveSettings → 遍历 settings 逐项 invoke 后端
    settings.value.autoReconnect = false
    await nextTick()
    await nextTick()

    const calls = callsOf('set_db_setting_mobile')
    // 至少写了一条设置（autoReconnect），且其余项也已随 watch 落库
    expect(calls.length).toBeGreaterThan(0)
    for (const [, payload] of calls) {
      expect(String(payload.key)).toMatch(/^mobile\./)
    }
    // 确认 autoReconnect 这一具体项被持久化（值字符串化）
    const reconnectCall = calls.find(([, p]) => p.key === 'mobile.autoReconnect')
    expect(reconnectCall).toBeDefined()
    expect(reconnectCall![1].value).toBe('false')
  })

  it('C-2 反向护栏：同一路径不得出现退役旧命令名 set_db_setting', async () => {
    const { settings } = useMobileSettings()
    settings.value.keepAlive = false
    settings.value.fontSize = 'large'
    await nextTick()
    await nextTick()

    expect(callsOf('set_db_setting').length).toBe(0)
    expect(callsOf('set_db_setting_mobile').length).toBeGreaterThan(0)
  })

  it('C-3 失败隔离：单次 invoke reject 不阻断后续设置项持久化', async () => {
    // autoReconnect 首次写入失败，其余项仍应被持久化
    mockInvoke.mockImplementationOnce(() => Promise.reject(new Error('backend down')))
    const { settings } = useMobileSettings()
    settings.value.notifyOnWaiting = false
    settings.value.vibrate = false
    await nextTick()
    await nextTick()

    const calls = callsOf('set_db_setting_mobile')
    // 失败项有记录（尝试过），且 vibrate 项在失败后仍被写入（未被异常截断）
    const vibrateCall = calls.find(([, p]) => p.key === 'mobile.vibrate')
    expect(vibrateCall).toBeDefined()
    expect(vibrateCall![1].value).toBe('false')
  })
})