/**
 * useMdnsAdvertiser 单测：广播面退役适配（spec 2026-10-14-mobile-legacy-frontend-adaptation D3）
 *
 * 被测契约（来源 = spec D3「移除广告调用」+ 退役锁
 * `retired_mobile_mdns_advertise_lock`（移动端广播面零活消费者死面））：
 * - C-1 零命令调用（正向）：startAdvertise() / stopAdvertise() 不 invoke 任何
 *   Tauri 命令（`mdns_start_advertise` / `mdns_stop_advertise` 宿主侧已退役，
 *   适配后本地置位/复位，无网络副作用）
 * - C-2 状态机保真（正例）：startAdvertise 置位 isAdvertising=true，stopAdvertise
 *   复位 false（旧前端仍消费该状态，形状保持兼容）
 * - C-3 幂等短路（反例守卫）：重复 startAdvertise 不产生多余状态翻转副作用
 *   （isAdvertising 已 true 时返回，整体幂等）
 *
 * mock 边界：invoke 全 mock，断言零调用（C-1 的「零」即本契约的强断言）。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

import { useMdnsAdvertiser } from '@/composables/useMdnsAdvertiser'

describe('useMdnsAdvertiser 广播面退役适配（spec D3 + 退役锁）', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
    mockInvoke.mockResolvedValue(undefined)
  })

  it('C-1 零命令调用：startAdvertise / stopAdvertise 均不 invoke 任何后端命令（广播面已退役）', async () => {
    const { startAdvertise, stopAdvertise } = useMdnsAdvertiser()
    await startAdvertise(8765, 'test-device')
    await stopAdvertise()
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('C-2 状态机保真：startAdvertise 置位 true，stopAdvertise 复位 false', async () => {
    const { isAdvertising, startAdvertise, stopAdvertise } = useMdnsAdvertiser()
    expect(isAdvertising.value).toBe(false)
    await startAdvertise(8765, 'test-device')
    expect(isAdvertising.value).toBe(true)
    await stopAdvertise()
    expect(isAdvertising.value).toBe(false)
  })

  it('C-3 幂等守卫：重复 startAdvertise 不产生异常副作用', async () => {
    const { isAdvertising, startAdvertise } = useMdnsAdvertiser()
    await startAdvertise(1, 'a')
    await startAdvertise(2, 'b')
    // 已置位后再次调用不抛错、状态保持
    expect(isAdvertising.value).toBe(true)
    expect(mockInvoke).not.toHaveBeenCalled()
  })
})