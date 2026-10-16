/**
 * 宿主页 composable 测试（票 2026-10-09）
 *
 * 行为契约（从实现推导）：
 * - requestPairing：调插件命令 + 置 requested 态
 * - verifyPairingCode：accepted=true → 回 idle + 清码 + true；accepted=false →
 *   回 idle + pairingError + false；命令抛错 → idle
 * - startSession / stopSession / removeSession：code!=0 → 抛带 message 的错误
 * - ws_paired / ws_auth_failed 事件 → 状态复位 / 错误注入（须先 subscribe）
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Disposable } from '@binblink/bedcode-plugin-sdk-mobile'
import { useHostPage } from '../useHostPage'
import { installMockMobileApi, uninstallMockMobileApi } from '../../terminal/__tests__/testKit'

/** 最小插件上下文替身（commands 可编程返回；events 内存总线可注入；respond 暴露给用例） */
function createFakeContext() {
  const listeners = new Map<string, Set<(payload?: unknown) => void>>()
  const responders = new Map<string, unknown>()
  return {
    commands: {
      execute: vi.fn(async (id: string) => {
        if (!responders.has(id)) throw new Error(`no responder for ${id}`)
        return responders.get(id)
      }),
    },
    events: {
      on: vi.fn((event: string, handler: (payload?: unknown) => void): Disposable => {
        let set = listeners.get(event)
        if (!set) {
          set = new Set()
          listeners.set(event, set)
        }
        set.add(handler)
        return { dispose: () => set!.delete(handler) }
      }),
      emit(event: string, payload?: unknown) {
        const set = listeners.get(event)
        if (set) for (const fn of [...set]) fn(payload)
      },
    },
    i18n: { t: (key: string) => key },
    dialogs: { showToast: vi.fn() },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
    /** 用例注册命令响应 */
    respond(command: string, result: unknown) {
      responders.set(command, result)
    },
  }
}

type FakeContext = ReturnType<typeof createFakeContext>

let ctx: FakeContext

beforeEach(() => {
  ctx = createFakeContext()
  installMockMobileApi({})
})

afterEach(() => {
  uninstallMockMobileApi()
})

function makeHost(provided: unknown = ctx) {
  return useHostPage(provided as any)
}

describe('配对流程状态机', () => {
  it('requestPairing：调用插件命令并进入 requested 态', async () => {
    ctx.respond('terminal-session.request-pairing', { ok: true })
    const host = makeHost()
    await host.requestPairing()
    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.request-pairing')
    expect(host.pairingStage.value).toBe('requested')
  })

  it('verifyPairingCode accepted=true：回 idle、清码、返回 true', async () => {
    ctx.respond('terminal-session.verify-pairing-code', { accepted: true })
    const host = makeHost()
    host.pairingCode.value = 'ABC123'
    const ok = await host.verifyPairingCode('ABC123')
    expect(ok).toBe(true)
    expect(host.pairingStage.value).toBe('idle')
    expect(host.pairingCode.value).toBe('')
    expect(host.pairingError.value).toBe('')
  })

  it('verifyPairingCode accepted=false：回 idle + 错误文案 + false（反例）', async () => {
    ctx.respond('terminal-session.verify-pairing-code', { accepted: false })
    const host = makeHost()
    const ok = await host.verifyPairingCode('BAD')
    expect(ok).toBe(false)
    expect(host.pairingStage.value).toBe('idle')
    expect(host.pairingError.value).toBe('hub.pairingFailed')
  })

  it('verifyPairingCode 命令抛错：异常上抛且状态回 idle', async () => {
    const host = makeHost()
    await expect(host.verifyPairingCode('X')).rejects.toThrow('no responder')
    expect(host.pairingStage.value).toBe('idle')
  })

  it('ws_paired 事件（subscribe 后）：清配对态（错误/码/阶段）', async () => {
    const host = makeHost()
    const subs = host.subscribe()
    host.pairingStage.value = 'requested'
    host.pairingCode.value = 'X'
    host.pairingError.value = 'hub.pairingFailed'
    ctx.events.emit('ws_paired')
    expect(host.pairingStage.value).toBe('idle')
    expect(host.pairingCode.value).toBe('')
    expect(host.pairingError.value).toBe('')
    subs.forEach((d) => d.dispose())
  })

  it('ws_auth_failed 事件（subscribe 后）：注入 reason 文案', async () => {
    const host = makeHost()
    const subs = host.subscribe()
    ctx.events.emit('ws_auth_failed', { reason: 'Device revoked' })
    expect(host.pairingError.value).toBe('Device revoked')
    subs.forEach((d) => d.dispose())
  })
})

describe('会话命令错误归一', () => {
  it('startSession code!=0：抛带 message 的错误', async () => {
    ctx.respond('terminal-session.start-session', { code: -1, message: 'network down' })
    const host = makeHost()
    await expect(host.startSession('cfg1')).rejects.toThrow('network down')
  })

  it('startSession code!=0 无 message：抛兜底 i18n key（供调用方 toast）', async () => {
    ctx.respond('terminal-session.start-session', { code: -1 })
    const host = makeHost()
    await expect(host.startSession('cfg1')).rejects.toThrow('hub.startFailed')
  })

  it('startSession code=0：返回新会话 id', async () => {
    ctx.respond('terminal-session.start-session', { code: 0, data: { sessionId: 's1' } })
    const host = makeHost()
    await expect(host.startSession('cfg1')).resolves.toBe('s1')
  })

  it('stopSession / removeSession code!=0：抛错', async () => {
    ctx.respond('terminal-session.stop-session', { code: 1, message: 'no such session' })
    ctx.respond('terminal-session.remove-session', { code: 1, message: 'forbidden' })
    const host = makeHost()
    await expect(host.stopSession('s1')).rejects.toThrow('no such session')
    await expect(host.removeSession('s1')).rejects.toThrow('forbidden')
  })
})

describe('扫码认证（插件命令面）', () => {
  it('authenticateWithQr accepted=true：透传 token 且返回 true', async () => {
    ctx.respond('terminal-session.authenticate-with-qr', { accepted: true })
    const host = makeHost()

    await expect(host.authenticateWithQr('tok-1')).resolves.toBe(true)
    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.authenticate-with-qr', {
      token: 'tok-1',
    })
    expect(ctx.logger.warn).not.toHaveBeenCalled()
  })

  it('authenticateWithQr accepted=false：返回 false 且落 warn（不抛）', async () => {
    ctx.respond('terminal-session.authenticate-with-qr', { accepted: false })
    const host = makeHost()

    await expect(host.authenticateWithQr('bad')).resolves.toBe(false)
    expect(String(ctx.logger.warn.mock.calls[0][0])).toContain('qr auth rejected')
  })

  it('authenticateWithQr code!=0：抛服务端 message（错误码归一）', async () => {
    ctx.respond('terminal-session.authenticate-with-qr', { code: 403, message: 'expired token' })
    const host = makeHost()

    await expect(host.authenticateWithQr('t')).rejects.toThrow('expired token')
  })
})

describe('连接引擎失败的机制落点（错误分类 + 日志）', () => {
  it('connectDevice 失败：错误槽位落分类 key 且 logger.error 记录', async () => {
    installMockMobileApi({
      connectDevice: async () => {
        throw new Error('host unreachable')
      },
    })
    const host = makeHost()

    await expect(host.connectDevice({ address: '10.0.0.2', port: 8765 })).rejects.toThrow(
      'host unreachable',
    )
    expect(host.deviceError.value).toBe('hub.unreachableToast')
    expect(ctx.logger.error).toHaveBeenCalledTimes(1)
    expect(String(ctx.logger.error.mock.calls[0][0])).toContain('10.0.0.2')
  })

  it('connectDevice 成功：清空错误槽位且不记错误日志', async () => {
    installMockMobileApi({ connectDevice: async () => {} })
    const host = makeHost()

    await host.connectDevice({ address: '10.0.0.3', port: 8765 })
    expect(host.deviceError.value).toBe('')
    expect(ctx.logger.error).not.toHaveBeenCalled()
  })

  it('startScan 失败：不抛（保持当前扫描态）但落 error 日志', async () => {
    installMockMobileApi({
      mdnsStart: async () => {
        throw new Error('mdns engine offline')
      },
    })
    const host = makeHost()

    await expect(host.startScan(false)).resolves.toBeUndefined()
    expect(String(ctx.logger.error.mock.calls[0][0])).toContain('mdns engine offline')
  })

  it('refreshBiometric 失败：状态置空并落 warn 日志（不阻断主流程）', async () => {
    const api = installMockMobileApi({})
    api.getBiometricKeyStatus = async () => {
      throw new Error('keystore unavailable')
    }
    const host = makeHost()

    await expect(host.refreshBiometric()).resolves.toBeUndefined()
    expect(host.biometric.value).toBeNull()
    expect(String(ctx.logger.warn.mock.calls[0][0])).toContain('keystore unavailable')
  })
})

describe('连接后静默换新 JWT（返回用户免配对）', () => {
  it('thost_authenticatesSilently_when_connectSucceedsWithStoredCredentials', async () => {
    const api = installMockMobileApi({
      connectDevice: async () => {},
      authenticateWithStoredToken: async () => {
        api.isPaired.value = true
        api.connectionStatus.value = 'paired'
        return true
      },
    })
    const host = makeHost()

    await host.connectDevice({ address: '10.0.0.4', port: 8765 })

    expect(host.isAuthenticated.value).toBe(true)
    expect(host.needsAuth.value).toBe(false)
  })

  it('thost_keepsConnectedStateForPairing_when_noStoredCredentials', async () => {
    const api = installMockMobileApi({
      connectDevice: async () => {},
      authenticateWithStoredToken: async () => false,
    })
    api.connectionStatus.value = 'connected'
    const host = makeHost()

    await host.connectDevice({ address: '10.0.0.5', port: 8765 })

    expect(host.isAuthenticated.value).toBe(false)
    expect(host.needsAuth.value).toBe(true)
  })

  it('thost_reportsAuthenticatedFromThisSilentAuth_when_pairedEventNotYetLanded', async () => {
    const api = installMockMobileApi({
      connectDevice: async () => {},
      // 换新成功，但 ws_paired 事件尚未到达宿主：isPaired 仍为 false
      authenticateWithStoredToken: async () => true,
    })
    api.connectionStatus.value = 'connected'
    const host = makeHost()

    const result = await host.connectDevice({ address: '10.0.0.7', port: 8765 })

    // 状态位仍为未认证（事件未落地），但返回值必须反映「本次静默换新成功」
    // ——调用方（连接流程）据它裁决是否弹配对口，读状态位会误弹。
    expect(host.isAuthenticated.value).toBe(false)
    expect(result).toEqual({ authenticated: true })
  })

  it('thost_reportsNotAuthenticated_when_storedCredentialsRejected', async () => {
    const api = installMockMobileApi({
      connectDevice: async () => {},
      authenticateWithStoredToken: async () => false,
    })
    api.connectionStatus.value = 'connected'
    const host = makeHost()

    await expect(host.connectDevice({ address: '10.0.0.8', port: 8765 })).resolves.toEqual({
      authenticated: false,
    })
  })

  it('thost_skipsSilentAuth_when_disabledForQrFlow（扫码自带一次性 token）', async () => {
    let called = 0
    const api = installMockMobileApi({
      connectDevice: async () => {},
      authenticateWithStoredToken: async () => {
        called += 1
        return false
      },
    })
    api.connectionStatus.value = 'connected'
    const host = makeHost()

    await host.connectDevice({ address: '10.0.0.6', port: 8765 }, { silentAuth: false })

    expect(called).toBe(0)
  })
})

describe('取消连接', () => {
  it('thost_forwardsCancelToEngine_and_swallowsFailure', async () => {
    const api = installMockMobileApi({})
    api.cancelConnection = async () => {
      throw new Error('no active connection')
    }
    const host = makeHost()

    await expect(host.cancelConnect()).resolves.toBeUndefined()
    expect(String(ctx.logger.warn.mock.calls[0][0])).toContain('no active connection')
  })
})
describe('连接历史写入 / 移除（认证成功后记一笔）', () => {
  it('addHistory：转发宿主域并带设备名', async () => {
    const api = installMockMobileApi({})
    const add = vi.fn(async () => {})
    api.addToConnectionHistory = add
    const host = makeHost()

    await host.addHistory('10.0.0.9:8765', 'desk-9')

    expect(add).toHaveBeenCalledWith('10.0.0.9:8765', 'desk-9')
  })

  it('addHistory 失败：落 warn 且不上抛（不能因记历史失败让连接变红）', async () => {
    const api = installMockMobileApi({})
    api.addToConnectionHistory = async () => {
      throw new Error('kv write failed')
    }
    const host = makeHost()

    await expect(host.addHistory('10.0.0.9:8765', 'desk-9')).resolves.toBeUndefined()
    expect(String(ctx.logger.warn.mock.calls[0][0])).toContain('kv write failed')
  })

  it('removeHistory：转发宿主域（行内 × 按钮）', async () => {
    const api = installMockMobileApi({})
    const remove = vi.fn(async () => {})
    api.removeFromConnectionHistory = remove
    const host = makeHost()

    await host.removeHistory('10.0.0.9:8765')

    expect(remove).toHaveBeenCalledWith('10.0.0.9:8765')
  })
})

describe('宿主机设置投影（首选认证方式 / 默认端口 / 终端上限）', () => {
  it('mobileSettings 直接透传宿主单例（连接页与会话页据此还原旧行为）', () => {
    const api = installMockMobileApi({})
    api.mobileSettings.value = { preferredAuthMethod: 'biometric', defaultPort: 9000, maxOpenTerminals: 3 }
    const host = makeHost()

    expect(host.mobileSettings.value.preferredAuthMethod).toBe('biometric')
    expect(host.mobileSettings.value.defaultPort).toBe(9000)
    expect(host.mobileSettings.value.maxOpenTerminals).toBe(3)
  })
})
