/**
 * 配对流命令门面集成测试（L2 场景 1）
 *
 * 协作实体：usePairing（composable） + useDeviceStore（真实 Pinia store） +
 * 宿主薄转发命令门面（`generate_pairing_code` / `clear_pairing_code` /
 * `get_current_pairing_code` / `list_paired_devices` / `remove_paired_device`）。
 *
 * **票 14 范围变更**：配对 / 设备视图已搬入 `com.bedcode.session` 插件，视图侧流程
 * 断言（配对码展示与倒计时、QR 渲染、device-connected 驱动状态流转、设备列表分区）
 * 随视图迁至插件工程（`plugins/session/src/__tests__/deviceCenter.test.ts`）。
 * 本文件保留**接缝不变**的那一半——宿主命令门面的集成（命令名 + 入参 + 状态落地），
 * 它与视图归属无关：插件侧最终也经同一条宿主命令/互调链路取得同一批事实。
 *
 * 测试 seam（与 useServer.test.ts 同模式）：
 * - 只 mock @tauri-apps/api 边界：core.invoke（按命令名捕获调用与入参）
 * - Pinia / composables / store 内部逻辑全部真实执行
 * - fixture 数据取自工厂（makePairing / makePairingCodeInfo）
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { usePairing } from '@/composables/usePairing'
import { useDeviceStore } from '@/stores/device'
import { makePairing, makePairingCodeInfo } from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// ==================== 测试基建 ====================

/** 后端状态（可变的模拟 DB） */
let pairedDevices: any[]
/** 下一次 generate_pairing_code 的返回 */
let generatedCode: ReturnType<typeof makePairingCodeInfo>

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'generate_pairing_code':
        return Promise.resolve(generatedCode)
      case 'clear_pairing_code':
        return Promise.resolve(undefined)
      case 'get_current_pairing_code':
        return Promise.resolve(null)
      case 'list_paired_devices':
        return Promise.resolve([...pairedDevices])
      case 'remove_paired_device':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  // 去掉调用数组首元素（命令名），只保留参数：与 toHaveBeenCalledWith 的参数形态一致
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

beforeEach(() => {
  vi.clearAllMocks()
  setActivePinia(createPinia())
  pairedDevices = [
    makePairing({
      id: 'device-1',
      deviceName: 'Phone 1',
      deviceFingerprint: 'fp-1',
      address: '192.168.1.50',
    }),
  ]
  generatedCode = makePairingCodeInfo({ code: '654321', expires_in: 60 })
  installInvokeMock()
})

// ==================== 场景 ====================

describe('配对流：宿主命令门面 × usePairing × useDeviceStore', () => {
  it('生成配对码：经 generate_pairing_code 取得回执，composable 状态与回执一致', async () => {
    const pairing = usePairing()

    await pairing.generateCode()

    expect(invokeCalls('generate_pairing_code')).toHaveLength(1)
    expect(pairing.pairingCode.value?.code).toBe('654321')
    expect(pairing.pairingCode.value?.expires_in).toBe(60)
  })

  it('取消配对码：clear_pairing_code 落地后 composable 状态置空', async () => {
    const pairing = usePairing()
    await pairing.generateCode()
    expect(pairing.pairingCode.value).not.toBeNull()

    await pairing.clearCode()

    expect(invokeCalls('clear_pairing_code')).toHaveLength(1)
    expect(pairing.pairingCode.value).toBeNull()
  })

  it('恢复当前配对码：无活跃码时返回 false 且不残留状态', async () => {
    const pairing = usePairing()

    const restored = await pairing.checkCurrentCode()

    expect(invokeCalls('get_current_pairing_code')).toHaveLength(1)
    expect(restored).toBe(false)
    expect(pairing.pairingCode.value).toBeNull()
  })

  it('设备列表：list_paired_devices 回执逐条落地到 store', async () => {
    const deviceStore = useDeviceStore()

    await deviceStore.loadPairedDevices()

    expect(invokeCalls('list_paired_devices')).toHaveLength(1)
    expect(deviceStore.pairedDevices).toHaveLength(1)
    expect(deviceStore.pairedDevices[0].deviceName).toBe('Phone 1')
    expect(deviceStore.pairedDevices[0].address).toBe('192.168.1.50')
  })

  it('撤销设备：remove_paired_device 带 id，随后重新取列表（界面与后端一致）', async () => {
    const deviceStore = useDeviceStore()
    await deviceStore.loadPairedDevices()
    pairedDevices = []

    await deviceStore.removeDevice('device-1')

    expect(invokeCalls('remove_paired_device')[0][0]).toEqual({ id: 'device-1' })
    expect(invokeCalls('list_paired_devices')).toHaveLength(2)
    expect(deviceStore.pairedDevices).toHaveLength(0)
  })
})
