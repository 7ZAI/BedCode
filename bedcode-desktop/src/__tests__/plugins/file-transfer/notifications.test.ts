/**
 * 对等连接通知行为契约（对等连接域从宿主 `useGlobalNotifications` 承接）
 *
 * 契约来源：宿主退役前 `useGlobalNotifications.ts` 的 nodeId 去重语义（回归基线）+
 * AGENTS §5 业务域归属（对等链路属 file-transfer 插件）。
 * 断言外部可见行为：toast 副作用、i18n key 与插值参数、订阅注销。
 *
 * 契约清单：
 * - C1 身份主键：nodeId（缺失即忽略，不参与去重）
 * - C2 首次连接 → connected；同 nodeId 重复连接（数据面短连接 churn）→ 静默
 * - C3 已知 nodeId 断开 → disconnected；未知 nodeId 断开 → 静默
 * - C4 展示名回退链：deviceName → fingerprintShort → nodeId 前 8 位 → i18n 兜底文案
 * - C5 接线：连接/断开事件分别产生 success / warning 提示，文案键与参数逐字正确
 * - C6 dispose 释放两条订阅
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

const { toastSuccess, toastWarning } = vi.hoisted(() => ({
  toastSuccess: vi.fn(),
  toastWarning: vi.fn(),
}))

vi.mock('vue-sonner', () => ({
  toast: { info: vi.fn(), warning: toastWarning, success: toastSuccess, error: vi.fn() },
}))

import {
  createPeerOnlineTracker,
  peerDisplayName,
  peerOnlineKey,
  startPeerNotifications,
} from '../../../../plugins/file-transfer/src/notifications'

type Handler = (payload: any) => void

/** 最小 mock PluginContext：捕获事件处理器 */
function makeContext() {
  const handlers = new Map<string, Handler>()
  const disposedEvents: string[] = []
  const t = vi.fn((key: string, params?: Record<string, unknown>) =>
    params ? `${key}|${String(params.name ?? '')}` : key,
  )
  const context = {
    i18n: { t },
    events: {
      on(event: string, handler: Handler) {
        handlers.set(event, handler)
        return {
          dispose: () => {
            disposedEvents.push(event)
            handlers.delete(event)
          },
        }
      },
    },
  } as unknown as PluginContext
  return { context, handlers, disposedEvents, t }
}

beforeEach(() => {
  toastSuccess.mockClear()
  toastWarning.mockClear()
})

describe('peerOnlineKey / peerDisplayName', () => {
  it('C1 主键 = nodeId；缺失 → 空键', () => {
    expect(peerOnlineKey({ nodeId: 'node-1' })).toBe('node-1')
    expect(peerOnlineKey({})).toBe('')
    expect(peerOnlineKey(null)).toBe('')
  })

  it('C4 展示名回退链：deviceName → fingerprintShort → nodeId 前 8 位', () => {
    expect(peerDisplayName({ nodeId: 'node-1', deviceName: 'Phone' })).toBe('Phone')
    expect(peerDisplayName({ nodeId: 'node-1', fingerprintShort: 'ab12cd' })).toBe('ab12cd')
    expect(peerDisplayName({ nodeId: '0123456789abcdef' })).toBe('01234567')
    expect(peerDisplayName(null)).toBe('')
  })
})

describe('createPeerOnlineTracker', () => {
  it('C2 首次连接 → connected，重复连接 → ignore', () => {
    const tracker = createPeerOnlineTracker()
    expect(tracker.observe({ nodeId: 'node-1' }, 'connected')).toBe('connected')
    expect(tracker.observe({ nodeId: 'node-1' }, 'connected')).toBe('ignore')
    expect(tracker.onlineCount).toBe(1)
  })

  it('C3 已知 nodeId 断开 → disconnected 并移出；未知 → ignore', () => {
    const tracker = createPeerOnlineTracker()
    tracker.observe({ nodeId: 'node-1' }, 'connected')
    expect(tracker.observe({ nodeId: 'node-2' }, 'disconnected')).toBe('ignore')
    expect(tracker.observe({ nodeId: 'node-1' }, 'disconnected')).toBe('disconnected')
    expect(tracker.onlineCount).toBe(0)
  })

  it('C1 缺 nodeId 的事件被忽略且不污染集合', () => {
    const tracker = createPeerOnlineTracker()
    expect(tracker.observe({}, 'connected')).toBe('ignore')
    expect(tracker.observe({}, 'disconnected')).toBe('ignore')
    expect(tracker.onlineCount).toBe(0)
  })
})

describe('startPeerNotifications', () => {
  it('C5 连接事件 → success 一次，文案键与设备名参数逐字正确', () => {
    const { context, handlers, t } = makeContext()
    const disposable = startPeerNotifications(context)

    handlers.get('peer-connected')!({ nodeId: 'node-1', deviceName: 'Phone' })

    expect(toastSuccess).toHaveBeenCalledTimes(1)
    expect(t).toHaveBeenCalledWith('transfer.notification.peerConnected', { name: 'Phone' })
    disposable.dispose()
  })

  it('C5 重复连接事件不重复提示；已知断开只提示一次', () => {
    const { context, handlers } = makeContext()
    const disposable = startPeerNotifications(context)

    handlers.get('peer-connected')!({ nodeId: 'node-1', deviceName: 'Phone' })
    handlers.get('peer-connected')!({ nodeId: 'node-1', deviceName: 'Phone' })
    expect(toastSuccess).toHaveBeenCalledTimes(1)

    handlers.get('peer-disconnected')!({ nodeId: 'node-9' })
    expect(toastWarning).not.toHaveBeenCalled()

    handlers.get('peer-disconnected')!({ nodeId: 'node-1', deviceName: 'Phone' })
    expect(toastWarning).toHaveBeenCalledTimes(1)
    disposable.dispose()
  })

  it('C5 无设备名时以短指纹展示（不留空名）', () => {
    const { context, handlers, t } = makeContext()
    const disposable = startPeerNotifications(context)

    handlers.get('peer-connected')!({ nodeId: 'node-1', fingerprintShort: 'ab12cd' })
    handlers.get('peer-disconnected')!({ nodeId: 'node-1', fingerprintShort: 'ab12cd' })

    expect(t).toHaveBeenCalledWith('transfer.notification.peerConnected', { name: 'ab12cd' })
    expect(t).toHaveBeenCalledWith('transfer.notification.peerDisconnected', { name: 'ab12cd' })
    disposable.dispose()
  })

  it('C6 dispose 释放两条订阅', () => {
    const { context, handlers, disposedEvents } = makeContext()
    const disposable = startPeerNotifications(context)

    disposable.dispose()

    expect(disposedEvents.sort()).toEqual(['peer-connected', 'peer-disconnected'])
    expect(handlers.size).toBe(0)
  })
})
