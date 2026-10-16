/**
 * useMdnsDiscovery 单测：mDNS 发现列表的启动/停止生命周期与事件合并契约
 *
 * 被测契约（来源：连接页「扫描发现」区交互需求 + composable 公开行为）：
 * - C-201 startDiscovery() 默认清空上次结果（重新扫描 = 从零发现）
 * - C-202 startDiscovery({keepResults:true}) 保留已发现列表（返回连接页后台续扫）
 * - C-203 已在扫描时再次 startDiscovery 幂等短路，不重复 invoke
 * - C-204 mdns_service_resolved 按 instance_name 合并：同实例更新、异实例追加
 * - C-205 mdns_service_removed 按 instance_name 摘除
 * - C-206 mdns_start_discovery 抛错 → isScanning 复位 false 且错误上抛（不吞异常）
 * - C-207 stopDiscovery 停扫描并注销监听；未扫描时短路不 invoke
 *
 * 单例模块级状态：每个用例 vi.resetModules() + 动态 import 拿全新实例，避免顺序依赖。
 * 替身只跨进程边界（tauri invoke / event listen）。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// ==================== 替身 ====================

const mockInvoke = vi.fn()

// 挡掉日志转发：frontendLogger 在 dev 分支起 400ms 攒批定时器，到点调
// invoke('report_frontend_log')。本文件每个用例都 resetModules，那只定时器随旧模块
// 实例一起被遗弃却仍在跑，会在后续用例的 mockInvoke 上留下与被测契约无关的调用，
// 让「未扫描时不 invoke」这类全局断言偶发红。日志不是本文件的被测对象，直接替身挡掉。
vi.mock('@/utils/frontendLogger', () => ({
  logger: { trace: vi.fn(), debug: vi.fn(), log: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

/** 事件名 → 监听回调表（测试通过 emitEvent 投喂宿主事件） */
const listeners = new Map<string, (payload: unknown) => void>()
/** listen 注册时返回的注销函数计数（用于验证 stopDiscovery 真注销监听） */
const unlistenCalls: string[] = []

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (payload: unknown) => void) => {
    listeners.set(event, handler)
    return () => {
      unlistenCalls.push(event)
      listeners.delete(event)
    }
  }),
}))

// ==================== 测试基建 ====================

/** 已发现服务的最小合法形状（对齐 composable 的 DiscoveredService） */
function service(instanceName: string, address = '10.0.0.1', port = 8765) {
  return {
    instance_name: instanceName,
    host_name: `${instanceName}.local`,
    address,
    port,
    txt_records: {},
    platform: 'desktop',
    device_name: instanceName,
  }
}

/** 每次用例都重建模块实例（清空模块级 discoveredServices / isScanning / 监听表） */
async function freshComposable() {
  vi.resetModules()
  listeners.clear()
  unlistenCalls.length = 0
  mockInvoke.mockReset()
  mockInvoke.mockResolvedValue(undefined)
  const mod = await import('@/composables/useMdnsDiscovery')
  return mod.useMdnsDiscovery()
}

/** 取列表的实例名序列（断言用，避免各处重复手写字段映射） */
const names = (list: ReadonlyArray<{ instance_name: string }>) => list.map(s => s.instance_name)

/** 投喂一个 mDNS 事件（Tauri 事件通道形状：handler 收到 { payload }） */
function emitEvent(event: string, payload: unknown): void {
  const handler = listeners.get(event)
  if (!handler) throw new Error(`监听器未注册: ${event}`)
  handler({ payload })
}

beforeEach(() => {
  mockInvoke.mockReset()
  mockInvoke.mockResolvedValue(undefined)
})

afterEach(() => {
  vi.clearAllMocks()
})

// ==================== C-201 / C-202 启动选项 ====================

describe('startDiscovery 启动选项', () => {
  it('should_清空上次结果并置扫描态_when_默认启动', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    expect(mdns.discoveredServices.value).toHaveLength(1)

    await mdns.stopDiscovery()
    await mdns.startDiscovery()

    // 反例面：默认启动必须把旧列表清掉（否则「重新扫描」看着像没生效）
    expect(mdns.discoveredServices.value).toEqual([])
    expect(mdns.isScanning.value).toBe(true)
    expect(mockInvoke).toHaveBeenCalledWith('mdns_start_discovery')
  })

  it('should_保留已发现列表_when_keepResults_为真', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    await mdns.stopDiscovery()

    // 反例面：keepResults 只保留旧列表，不跳过宿主扫描命令
    await mdns.startDiscovery({ keepResults: true })

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-a'])
    expect(mdns.isScanning.value).toBe(true)
    expect(mockInvoke).toHaveBeenCalledWith('mdns_start_discovery')
  })
})

// ==================== C-203 幂等 ====================

describe('startDiscovery 幂等', () => {
  it('should_不重复调用宿主命令_when_已在扫描中', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    mockInvoke.mockClear()

    await mdns.startDiscovery({ keepResults: true })

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(mdns.isScanning.value).toBe(true)
  })
})

// ==================== C-204 / C-205 事件合并 ====================

describe('发现列表事件合并', () => {
  it('should_更新同实例而不追加_when_resolved_重复到达', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()

    emitEvent('mdns_service_resolved', service('desk-a', '10.0.0.1', 8765))
    emitEvent('mdns_service_resolved', service('desk-a', '10.0.0.9', 9999))

    expect(mdns.discoveredServices.value).toHaveLength(1)
    expect(mdns.discoveredServices.value[0].address).toBe('10.0.0.9')
    expect(mdns.discoveredServices.value[0].port).toBe(9999)
  })

  it('should_追加异实例_when_resolved_新实例到达', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()

    emitEvent('mdns_service_resolved', service('desk-a'))
    emitEvent('mdns_service_resolved', service('desk-b', '10.0.0.2'))

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-a', 'desk-b'])
  })

  it('should_只摘除对应实例_when_removed_到达', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    emitEvent('mdns_service_resolved', service('desk-b'))

    emitEvent('mdns_service_removed', { instance_name: 'desk-a' })

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-b'])
  })
})

// ==================== C-206 启动失败 ====================

describe('startDiscovery 异常', () => {
  it('should_复位扫描态并上抛_when_宿主启动命令失败', async () => {
    const mdns = await freshComposable()
    mockInvoke.mockRejectedValueOnce(new Error('mdns busy'))

    await expect(mdns.startDiscovery()).rejects.toThrow('mdns busy')
    expect(mdns.isScanning.value).toBe(false)
  })

  it('should_复位扫描态_when_停止命令失败', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    mockInvoke.mockRejectedValueOnce(new Error('stop failed'))

    // 停止失败不得把扫描态留成 true（否则 UI 永远显示「停止扫描」按钮）
    await mdns.stopDiscovery()

    expect(mdns.isScanning.value).toBe(false)
  })
})

// ==================== C-207 停止 ====================

describe('stopDiscovery', () => {
  it('should_停扫描并注销监听_when_调用', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    mockInvoke.mockClear()

    await mdns.stopDiscovery()

    expect(mockInvoke).toHaveBeenCalledWith('mdns_stop_discovery')
    expect(mdns.isScanning.value).toBe(false)
    expect(unlistenCalls).toContain('mdns_service_resolved')
    // 监听注销后事件不再进列表（否则停止扫描仍会被后台事件塞新设备）
    expect(listeners.has('mdns_service_resolved')).toBe(false)
    expect(mdns.discoveredServices.value).toHaveLength(1)
  })

  it('should_不调用宿主停止命令_when_本就未扫描', async () => {
    const mdns = await freshComposable()

    await mdns.stopDiscovery()

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(mdns.isScanning.value).toBe(false)
  })

  it('should_重启后重新接收事件_when_停止再启动', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    await mdns.stopDiscovery()
    await mdns.startDiscovery({ keepResults: true })

    emitEvent('mdns_service_resolved', service('desk-b'))

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-a', 'desk-b'])
  })
})

// ==================== refreshServices ====================

describe('refreshServices', () => {
  it('should_以宿主返回列表覆盖本地列表', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    mockInvoke.mockResolvedValueOnce([service('desk-x', '10.0.0.7')])

    await mdns.refreshServices()

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-x'])
  })

  it('should_保留本地列表_when_宿主读取失败', async () => {
    const mdns = await freshComposable()
    await mdns.startDiscovery()
    emitEvent('mdns_service_resolved', service('desk-a'))
    mockInvoke.mockRejectedValueOnce(new Error('read failed'))

    await mdns.refreshServices()

    expect(names(mdns.discoveredServices.value)).toEqual(['desk-a'])
  })
})