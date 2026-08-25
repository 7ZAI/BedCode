/**
 * deriveDeviceRows 纯函数测试（ticket 02）
 *
 * 覆盖三态归并优先级、拨号错误呈现边界、能力位归一化、活跃标记约束。
 */

import { describe, it, expect } from 'vitest'
import {
  deriveDeviceRows,
  type DiscoveredDevice,
} from '../../../../plugins/file-transfer/src/composables/deviceState'

const NODE_A = 'a'.repeat(32)
const NODE_B = 'b'.repeat(32)
const NODE_C = 'c'.repeat(32)

function makeDevice(overrides: Partial<DiscoveredDevice> = {}): DiscoveredDevice {
  return {
    nodeId: NODE_A,
    deviceName: '张三的手机',
    addr: '192.168.1.10:47613',
    fileTransfer: true,
    ...overrides,
  }
}

describe('deriveDeviceRows', () => {
  it('returns empty rows for empty discovery snapshot', () => {
    expect(
      deriveDeviceRows({
        devices: [],
        connectingIds: new Set(),
        connectedIds: new Set(),
        dialErrors: {},
        activePeerId: '',
      }),
    ).toEqual([])
  })

  it('derives idle status for discovered but unconnected devices', () => {
    const rows = deriveDeviceRows({
      devices: [makeDevice()],
      connectingIds: new Set(),
      connectedIds: new Set(),
      dialErrors: {},
      activePeerId: '',
    })
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      nodeId: NODE_A,
      deviceName: '张三的手机',
      addr: '192.168.1.10:47613',
      fileTransfer: true,
      status: 'idle',
      dialError: null,
      isActive: false,
    })
  })

  it('connecting beats idle; connected beats connecting (priority merge)', () => {
    const rows = deriveDeviceRows({
      devices: [
        makeDevice({ nodeId: NODE_A }),
        makeDevice({ nodeId: NODE_B }),
        makeDevice({ nodeId: NODE_C, deviceName: '' }),
      ],
      // NODE_C 同时出现在握手中与已连接集合：已连接优先
      connectingIds: new Set([NODE_A, NODE_C]),
      connectedIds: new Set([NODE_B, NODE_C]),
      dialErrors: {},
      activePeerId: '',
    })
    expect(rows[0]!.status).toBe('connecting')
    expect(rows[1]!.status).toBe('connected')
    expect(rows[2]!.status).toBe('connected')
  })

  it('surfaces dial errors only in idle state and clears them once connected', () => {
    const base = {
      devices: [makeDevice({ nodeId: NODE_A }), makeDevice({ nodeId: NODE_B })],
      connectingIds: new Set<string>(),
      connectedIds: new Set<string>([NODE_B]),
      dialErrors: { [NODE_A]: 'denied', [NODE_B]: 'unreachable' } as Record<
        string,
        'denied' | 'unreachable'
      >,
      activePeerId: '',
    }
    const rows = deriveDeviceRows(base)
    expect(rows[0]!.dialError).toBe('denied')
    // 已连接态不呈现历史错误（宿主事件已确认连接成功）
    expect(rows[1]!.dialError).toBeNull()
    expect(rows[1]!.status).toBe('connected')
  })

  it('hides dial error while connecting (retry in progress)', () => {
    const rows = deriveDeviceRows({
      devices: [makeDevice()],
      connectingIds: new Set([NODE_A]),
      connectedIds: new Set(),
      dialErrors: { [NODE_A]: 'denied' },
      activePeerId: '',
    })
    expect(rows[0]!.status).toBe('connecting')
    expect(rows[0]!.dialError).toBeNull()
  })

  it('normalizes missing capability flag to capable and falls back name to nodeId', () => {
    const rows = deriveDeviceRows({
      devices: [{ nodeId: NODE_A, deviceName: '' }],
      connectingIds: new Set(),
      connectedIds: new Set(),
      dialErrors: {},
      activePeerId: '',
    })
    expect(rows[0]!.fileTransfer).toBe(true)
    expect(rows[0]!.deviceName).toBe(NODE_A)
  })

  it('marks active only when the node is both connected and selected', () => {
    const rows = deriveDeviceRows({
      devices: [makeDevice({ nodeId: NODE_A }), makeDevice({ nodeId: NODE_B })],
      connectingIds: new Set(),
      connectedIds: new Set([NODE_A]),
      dialErrors: {},
      // 活跃 id 指向未连接节点：不标记
      activePeerId: NODE_B,
    })
    expect(rows[0]!.isActive).toBe(false)
    expect(rows[1]!.isActive).toBe(false)

    const activated = deriveDeviceRows({
      devices: [makeDevice({ nodeId: NODE_A })],
      connectingIds: new Set(),
      connectedIds: new Set([NODE_A]),
      dialErrors: {},
      activePeerId: NODE_A,
    })
    expect(activated[0]!.isActive).toBe(true)
  })
})
