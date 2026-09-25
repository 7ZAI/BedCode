/**
 * 设备连接通知行为契约（设备域从宿主 `useGlobalNotifications` 承接）
 *
 * 契约来源：宿主退役前 `useGlobalNotifications.ts` 的去重语义（回归基线）+
 * AGENTS §5 业务域归属（设备上下线属本插件）。
 * 断言外部可见行为：toast 副作用、i18n key 与插值参数、订阅注销；不测内部实现。
 *
 * 契约清单：
 * - C1 身份主键：指纹优先，兜底 device_id / addr；三者皆缺 → 空键（不参与去重）
 * - C2 上线跃迁：新身份 connected → 提示一次并计入在线集合
 * - C3 重复上线：同身份再次 connected（配对码/QR/reauth/事件 WS 各发一次）→ 静默
 * - C4 断开跃迁：已知在线身份 disconnected → 提示并移出集合
 * - C5 未知断开：不在集合内的身份 disconnected → 静默（未知状态不误报）
 * - C6 缺身份：空载荷事件 → 静默且不污染集合（后续同 device_id 事件仍能提示）
 * - C7 种子化：激活时 `session.devices.connect-list` 的指纹作为在线基线
 *   （其后该设备的 disconnected 能提示、其重复 connected 静默）
 * - C8 种子化失败（命令面报错）→ 不抛出，退化为纯事件驱动（事件仍可提示）
 * - C9 展示名：无设备名回退「移动设备」；i18n 键与插值参数逐字正确
 * - C10 注销：dispose 后两条订阅被释放，事件不再产生任何提示
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

const { toastInfo, toastWarning } = vi.hoisted(() => ({
  toastInfo: vi.fn(),
  toastWarning: vi.fn(),
}))

vi.mock('vue-sonner', () => ({
  toast: { info: toastInfo, warning: toastWarning, success: vi.fn(), error: vi.fn() },
}))

import {
  createDeviceOnlineTracker,
  deviceOnlineKey,
  startDeviceNotifications,
} from '../notifications'

type Handler = (payload: any) => void

/** 最小 mock PluginContext：捕获事件处理器 + 记录命令调用 */
function makeContext(seed: unknown = { connections: [] }, seedFails = false) {
  const handlers = new Map<string, Handler>()
  const disposedEvents: string[] = []
  const execute = vi.fn(async () => {
    if (seedFails) throw new Error('connect-list unavailable')
    return seed
  })
  // 简化的 i18n 替身：无参键返回真实文案（mobileDevice 是被当作插值值使用的），
  // 带参键返回「键|插值」以便断言 key 与参数双正确
  const TEXTS: Record<string, string> = {
    'session.notification.mobileDevice': '移动设备',
  }
  const t = vi.fn((key: string, params?: Record<string, unknown>) =>
    params ? `${key}|${String(params.name ?? '')}` : (TEXTS[key] ?? key),
  )
  const context = {
    i18n: { t },
    commands: { execute },
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
  return { context, handlers, disposedEvents, execute, t }
}

beforeEach(() => {
  toastInfo.mockClear()
  toastWarning.mockClear()
})

// ==================== C1 身份主键 ====================

describe('deviceOnlineKey', () => {
  it('C1 指纹优先于 device_id / addr', () => {
    expect(deviceOnlineKey({ fingerprint: 'fp-1', device_id: 'd-1', addr: '1.2.3.4' })).toBe('fp-1')
  })

  it('C1 无指纹时回退 device_id，再回退 addr', () => {
    expect(deviceOnlineKey({ device_id: 'd-1', addr: '1.2.3.4' })).toBe('d-1')
    expect(deviceOnlineKey({ addr: '1.2.3.4' })).toBe('1.2.3.4')
  })

  it('C1 三者皆缺（含 null / undefined）→ 空键', () => {
    expect(deviceOnlineKey({})).toBe('')
    expect(deviceOnlineKey(null)).toBe('')
    expect(deviceOnlineKey(undefined)).toBe('')
  })
})

// ==================== C2-C6 在线态跃迁 ====================

describe('createDeviceOnlineTracker', () => {
  it('C2 新身份上线 → connected 且计入集合', () => {
    const tracker = createDeviceOnlineTracker()
    expect(tracker.observe({ fingerprint: 'fp-1' }, 'connected')).toBe('connected')
    expect(tracker.onlineCount).toBe(1)
  })

  it('C3 同身份重复上线 → ignore 且集合不增长', () => {
    const tracker = createDeviceOnlineTracker()
    tracker.observe({ fingerprint: 'fp-1' }, 'connected')
    expect(tracker.observe({ fingerprint: 'fp-1' }, 'connected')).toBe('ignore')
    expect(tracker.onlineCount).toBe(1)
  })

  it('C4 已知身份断开 → disconnected 且移出集合', () => {
    const tracker = createDeviceOnlineTracker()
    tracker.observe({ fingerprint: 'fp-1' }, 'connected')
    expect(tracker.observe({ fingerprint: 'fp-1' }, 'disconnected')).toBe('disconnected')
    expect(tracker.onlineCount).toBe(0)
  })

  it('C5 未知身份断开 → ignore（不误报离线）', () => {
    const tracker = createDeviceOnlineTracker()
    expect(tracker.observe({ fingerprint: 'fp-unknown' }, 'disconnected')).toBe('ignore')
    expect(tracker.onlineCount).toBe(0)
  })

  it('C6 缺身份事件 → ignore 且不污染集合（后续同 device_id 仍可上线）', () => {
    const tracker = createDeviceOnlineTracker()
    expect(tracker.observe({}, 'connected')).toBe('ignore')
    expect(tracker.observe({}, 'disconnected')).toBe('ignore')
    expect(tracker.onlineCount).toBe(0)
    // 空键未入集合：带 device_id 的真实上线不被吞
    expect(tracker.observe({ device_id: 'd-1' }, 'connected')).toBe('connected')
  })

  it('C7 seed 的指纹视作已在线：断开可提示、重复上线静默', () => {
    const tracker = createDeviceOnlineTracker()
    tracker.seed(['fp-a', ''])
    expect(tracker.onlineCount).toBe(1)
    expect(tracker.observe({ fingerprint: 'fp-a' }, 'connected')).toBe('ignore')
    expect(tracker.observe({ fingerprint: 'fp-a' }, 'disconnected')).toBe('disconnected')
  })

  it('clear 清空集合（停用时不再残留在线态）', () => {
    const tracker = createDeviceOnlineTracker()
    tracker.observe({ fingerprint: 'fp-1' }, 'connected')
    tracker.clear()
    expect(tracker.onlineCount).toBe(0)
  })
})

// ==================== C7-C10 接线 ====================

describe('startDeviceNotifications', () => {
  it('C9 上线事件 → toast.info 一次，文案键与设备名参数逐字正确', async () => {
    const { context, handlers, t } = makeContext()
    const disposable = startDeviceNotifications(context)

    handlers.get('device-connected')!({ fingerprint: 'fp-1', device_name: 'Phone A' })

    expect(toastInfo).toHaveBeenCalledTimes(1)
    expect(t).toHaveBeenCalledWith('session.notification.deviceConnected', { name: 'Phone A' })
    expect(String(toastInfo.mock.calls[0][0])).toContain('Phone A')
    disposable.dispose()
  })

  it('C9 载荷无设备名 → 回退「移动设备」文案键', async () => {
    const { context, handlers, t } = makeContext()
    const disposable = startDeviceNotifications(context)

    handlers.get('device-connected')!({ fingerprint: 'fp-1' })

    expect(t).toHaveBeenCalledWith('session.notification.mobileDevice')
    expect(t).toHaveBeenCalledWith('session.notification.deviceConnected', { name: '移动设备' })
    disposable.dispose()
  })

  it('C3 重复上线事件不重复提示', () => {
    const { context, handlers } = makeContext()
    const disposable = startDeviceNotifications(context)

    handlers.get('device-connected')!({ fingerprint: 'fp-1' })
    handlers.get('device-connected')!({ fingerprint: 'fp-1' })

    expect(toastInfo).toHaveBeenCalledTimes(1)
    disposable.dispose()
  })

  it('C4/C5 已知身份断开提示，未知身份断开静默', () => {
    const { context, handlers } = makeContext()
    const disposable = startDeviceNotifications(context)

    handlers.get('device-connected')!({ fingerprint: 'fp-1', device_name: 'Phone A' })
    handlers.get('device-disconnected')!({ fingerprint: 'fp-unknown' })
    expect(toastWarning).not.toHaveBeenCalled()

    handlers.get('device-disconnected')!({ fingerprint: 'fp-1', device_name: 'Phone A' })
    expect(toastWarning).toHaveBeenCalledTimes(1)
    disposable.dispose()
  })

  it('C7 种子化回执的指纹成为在线基线（其后断开可提示）', async () => {
    const { context, handlers, execute } = makeContext({
      connections: [{ fingerprint: 'fp-seeded' }, { fingerprint: null }],
    })
    const disposable = startDeviceNotifications(context)
    await vi.waitFor(() => expect(execute).toHaveBeenCalledWith('session.devices.connect-list', {}))

    // 基线内设备重复上线静默
    handlers.get('device-connected')!({ fingerprint: 'fp-seeded' })
    expect(toastInfo).not.toHaveBeenCalled()

    // 基线内设备断开 → 提示（若未种子化会被误吞）
    handlers.get('device-disconnected')!({ fingerprint: 'fp-seeded', device_name: 'Tablet' })
    expect(toastWarning).toHaveBeenCalledTimes(1)
    disposable.dispose()
  })

  it('C8 种子化命令失败不抛出且退化为事件驱动', async () => {
    const { context, handlers, execute } = makeContext({ connections: [] }, true)
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const disposable = startDeviceNotifications(context)
    await vi.waitFor(() => expect(execute).toHaveBeenCalled())
    await vi.waitFor(() => expect(warnSpy).toHaveBeenCalled())

    handlers.get('device-connected')!({ fingerprint: 'fp-1' })
    expect(toastInfo).toHaveBeenCalledTimes(1)

    warnSpy.mockRestore()
    disposable.dispose()
  })

  it('C10 dispose 释放两条订阅（宿主此后不再向本插件派发）', () => {
    const { context, handlers, disposedEvents } = makeContext()
    const disposable = startDeviceNotifications(context)

    disposable.dispose()

    expect(disposedEvents.sort()).toEqual(['device-connected', 'device-disconnected'])
    expect(handlers.size).toBe(0)
  })
})
