/**
 * 快照恢复拨号防线测试（桌面端镜像；移动端同构用例见
 * bedcode-mobile/src/__tests__/plugins/file-transfer/）
 *
 * 覆盖三条防线（2026-10-16 实机根因）：
 * ① 快照只恢复「近期可见」条目（isWithinRestoreWindow，TTL 窗口 ≤120s）——
 *    9 天前残留的旧网络地址（实测 192.168.1.4:47613）不得上屏成为拨号候选；
 * ② restored（未经本会话实时 mdns-found 刷新）条目拒绝拨号 + 触发宿主重查，
 *    实时条目接管（restored 清 false）后才按实时 endpoint 拨号；
 * ③ 纯函数边界：过期 / 时钟超前 / 缺失均判不可恢复。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { usePeerDevices } from '../composables/usePeerDevices'
import {
  DEVICE_TTL_MS,
  isWithinRestoreWindow,
} from '../composables/deviceState'

type EventHandler = (payload: any) => void

const NODE_A = 'a'.repeat(64)

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler[]>()
  const responders = new Map<string, (args: any) => unknown>()

  const context = {
    commands: {
      async execute(id: string, args?: any) {
        calls.push({ id, args })
        const respond = responders.get(id)
        if (!respond) throw new Error(`no responder for ${id}`)
        return respond(args)
      },
      register: vi.fn(),
    },
    events: {
      on(event: string, handler: EventHandler) {
        const list = handlers.get(event) ?? []
        list.push(handler)
        handlers.set(event, list)
        return {
          dispose: () =>
            handlers.set(
              event,
              (handlers.get(event) ?? []).filter((h) => h !== handler),
            ),
        }
      },
    },
  } as unknown as PluginContext

  function emit(event: string, payload: unknown): void {
    const list = handlers.get(event)
    if (!list?.length) throw new Error(`no captured handler for ${event}`)
    for (const handler of list) handler(payload)
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  const flush = () => new Promise((r) => setTimeout(r, 0))

  return { context, calls, emit, onCommand, flush }
}

/** mdns:found 载荷形状（引擎透传 wire） */
function makeFound(nodeId: string, name: string, overrides: Record<string, any> = {}) {
  return {
    instanceName: `bedcode-peer-${nodeId.slice(0, 8)}._bedcode-peer._tcp.local.`,
    addresses: ['10.62.87.97'],
    port: 47613,
    txtRecords: { id: nodeId, name, ver: '1', cap: '1' },
    ...overrides,
  }
}

describe('isWithinRestoreWindow (snapshot restore window)', () => {
  const NOW = Date.now()

  it('accepts entries seen within the TTL window', () => {
    expect(isWithinRestoreWindow(NOW, NOW)).toBe(true)
    expect(isWithinRestoreWindow(NOW - DEVICE_TTL_MS, NOW)).toBe(true)
  })

  it('rejects stale (beyond TTL), future, and non-numeric lastSeen', () => {
    expect(isWithinRestoreWindow(NOW - DEVICE_TTL_MS - 1, NOW)).toBe(false)
    expect(isWithinRestoreWindow(NOW - 9 * 24 * 60 * 60 * 1000, NOW)).toBe(false)
    expect(isWithinRestoreWindow(NOW + 1, NOW)).toBe(false)
    expect(isWithinRestoreWindow(undefined, NOW)).toBe(false)
    expect(isWithinRestoreWindow(Number.NaN, NOW)).toBe(false)
  })
})

describe('usePeerDevices snapshot dial guard', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    env.onCommand('file-transfer.get-device-snapshot', () => ({ devices: [] }))
    env.onCommand('file-transfer.save-device-snapshot', () => ({ ok: true }))
    env.onCommand('file-transfer.set-active-peer', () => ({ ok: true }))
    env.onCommand('file-transfer.refresh-devices', () => ({}))
  })

  it('drops stale snapshot entries beyond the restore window', async () => {
    env.onCommand('file-transfer.get-device-snapshot', () => ({
      devices: [
        {
          nodeId: NODE_A,
          deviceName: '旧网络残留',
          addr: '192.168.1.4',
          port: 47613,
          capabilitiesHex: '1',
          // 9 天前可见：换网/换身份后的陈旧条目不得恢复为拨号候选
          lastSeenMs: Date.now() - 9 * 24 * 60 * 60 * 1000,
        },
      ],
    }))
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()

    expect(dev.devices.value).toHaveLength(0)
    expect(dev.rows.value).toHaveLength(0)
  })

  it('refuses to dial restored entries until live mdns refresh replaces them', async () => {
    env.onCommand('file-transfer.get-device-snapshot', () => ({
      devices: [
        {
          nodeId: NODE_A,
          deviceName: 'Redmi K70E',
          addr: '192.168.1.4', // 快照旧网络残留地址（endpoint 未经本会话验证）
          port: 47613,
          capabilitiesHex: '1',
          lastSeenMs: Date.now() - 30_000, // TTL 窗口内 → 被恢复（recent 标注）
        },
      ],
    }))
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()

    // 未验证的恢复条目：拒绝拨号（不发 dial-peer），改为触发宿主即时重查
    expect(await dev.connect(NODE_A)).toBe('unreachable')
    expect(env.calls.some((c) => c.id === 'file-transfer.dial-peer')).toBe(false)
    expect(env.calls.some((c) => c.id === 'file-transfer.refresh-devices')).toBe(true)
    expect(dev.rows.value[0]?.dialError).toBe('unreachable')

    // 实时 mdns-found 接管（restored 清 false、地址更新）后按实时 endpoint 拨号
    env.onCommand('file-transfer.dial-peer', () => ({ status: 'connected' }))
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'Redmi K70E'))
    expect(await dev.connect(NODE_A)).toBe('connected')
    expect(env.calls).toContainEqual({
      id: 'file-transfer.dial-peer',
      args: { endpoint: { nodeId: NODE_A, addr: '10.62.87.97', port: 47613 } },
    })
  })
})