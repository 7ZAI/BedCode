/**
 * useLinkEncryption 单元测试（issue 08）
 *
 * 覆盖配置持久化往返、HTTP 通道子开关粒度判定与 pin 依赖：
 * isChannelEncryptionActive = 主开关 ∧ HTTP 子开关 ∧ 已 pin。
 * WS 通道子开关已随桌面端插件端点帧加密退役（票 06），本测试不再覆盖。
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { readFile } from 'node:fs/promises'
import {
  isChannelEncryptionActive,
  getPinnedKey,
  getPinnedFingerprint,
  initLinkCryptoPinSync,
  syncLinkCryptoContextToNative,
  useLinkEncryptionSettings,
} from '@/composables/useLinkEncryption'

// 动态 import @tauri-apps/api/core（sync 桥），mock invoke 捕获推送参数
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))
// 动态 import @tauri-apps/api/event（initLinkCryptoPinSync），捕获 listen 回调以手动触发
let pinHandler: ((payload: { kdPublicB64?: string | null; kdFingerprint?: string | null }) => void) | null = null
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_event: string, handler: (payload: unknown) => void) => {
    pinHandler = handler as typeof pinHandler
    return Promise.resolve(() => {
      pinHandler = null
    })
  }),
}))

beforeEach(() => {
  localStorage.clear()
  // 模块级单例跨用例残留：localStorage 清理不影响内存态，需显式重置回默认值
  const s = useLinkEncryptionSettings().settings
  s.value.enabled = false
  s.value.strictMode = false
  s.value.encryptHttp = true
})

describe('useLinkEncryptionSettings', () => {
  it('默认值：功能整体关，HTTP 载荷加密默认开', () => {
    const { settings } = useLinkEncryptionSettings()

    expect(settings.value.enabled).toBe(false)
    expect(settings.value.encryptHttp).toBe(true)
  })

  it('setEnabled / setChannel(http) / setStrictMode 变更并落盘，重载后保留', () => {
    const s1 = useLinkEncryptionSettings()
    s1.setEnabled(true)
    s1.setStrictMode(true)
    s1.setChannel('http', false)

    // 新读取方应看到同一单例的持久化状态
    const s2 = useLinkEncryptionSettings()
    expect(s2.settings.value.enabled).toBe(true)
    expect(s2.settings.value.strictMode).toBe(true)
    expect(s2.settings.value.encryptHttp).toBe(false)
    expect(JSON.parse(localStorage.getItem('link-encryption')!).encryptHttp).toBe(false)
  })

  it('isChannelEncryptionActive：主开关关时一律 false（即使已 pin）', () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    const { setEnabled } = useLinkEncryptionSettings()
    setEnabled(false)
    expect(isChannelEncryptionActive('http')).toBe(false)
    expect(getPinnedKey()).not.toBeNull()
  })

  it('isChannelEncryptionActive：主开+pin 下按 HTTP 子开关独立判定', () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    const { setEnabled, setChannel } = useLinkEncryptionSettings()
    setEnabled(true)

    setChannel('http', false)
    expect(isChannelEncryptionActive('http')).toBe(false)

    setChannel('http', true)
    expect(isChannelEncryptionActive('http')).toBe(true)
  })

  it('未 pin 时即使全开也不参与加密（协商无信任锚）', () => {
    const { setEnabled } = useLinkEncryptionSettings()
    setEnabled(true)
    expect(getPinnedKey()).toBeNull()
    expect(isChannelEncryptionActive('http')).toBe(false)
  })
})

describe('syncLinkCryptoContextToNative（set_link_crypto_context 推送）', () => {
  it('推送参数含全部字段（enabled/strictMode/encryptHttp/kdPublicB64）；不含已退役的 ws 开关', async () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    const { setEnabled, setStrictMode, setChannel } = useLinkEncryptionSettings()
    setEnabled(true)
    setStrictMode(true)
    setChannel('http', false)
    mockInvoke.mockResolvedValue(undefined)

    await syncLinkCryptoContextToNative()

    expect(mockInvoke).toHaveBeenCalledWith('set_link_crypto_context', {
      enabled: true,
      strictMode: true,
      encryptHttp: false,
      kdPublicB64: 'cHVibGljLWtleQ==',
    })
    expect(mockInvoke.mock.calls[0][1]).not.toHaveProperty('encryptWsEvent')
  })
})

describe('pin 落地（ws_link_crypto_pin 事件驱动 applyPin；HTTP 通道 pin 刷新已收束 Rust）', () => {
  // 合法 X25519 公钥（32 字节 0xAB 的 base64）：applyPin 写入前校验 32 字节，
  // 短/畸形公钥被拒绝——信任锚不应被污染
  const VALID_KID_B64 = 'q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6s='

  async function emitPin(payload: { kdPublicB64?: string | null; kdFingerprint?: string | null }) {
    await initLinkCryptoPinSync()
    // Tauri listen handler 收到 { payload } 事件形状（与真实事件一致）
    pinHandler?.({ payload } as never)
  }

  it('事件携带公钥+指纹时两者都写入', async () => {
    await emitPin({ kdPublicB64: VALID_KID_B64, kdFingerprint: 'aabbccdd' })
    expect(getPinnedKey()).toBe(VALID_KID_B64)
    expect(getPinnedFingerprint()).toBe('aabbccdd')
  })

  it('指纹缺失时公钥仍写入且旧指纹被清除（换机后旧指纹是 false-positive 信任锚）', async () => {
    localStorage.setItem('link_kd_fingerprint', 'stale-fingerprint')
    await emitPin({ kdPublicB64: VALID_KID_B64 })
    expect(getPinnedKey()).toBe(VALID_KID_B64)
    expect(getPinnedFingerprint()).toBeNull()
  })

  it('畸形公钥（非 32 字节）拒绝写入，且不清除既有 pin', async () => {
    localStorage.setItem('link_kd_public_b64', VALID_KID_B64)
    localStorage.setItem('link_kd_fingerprint', 'aabbccdd')
    await emitPin({ kdPublicB64: 'cHVibGljLWtleQ==' /* 10 字节 */ })
    expect(getPinnedKey()).toBe(VALID_KID_B64)
    expect(getPinnedFingerprint()).toBe('aabbccdd')
  })

  it('无公钥不写任何 pin（防空串污染；协商失败不清 pin 的信任锚语义）', async () => {
    localStorage.setItem('link_kd_public_b64', VALID_KID_B64)
    localStorage.setItem('link_kd_fingerprint', 'aabbccdd')
    await emitPin({})
    expect(getPinnedKey()).toBe(VALID_KID_B64)
    expect(getPinnedFingerprint()).toBe('aabbccdd')
  })
})

describe('结构锁：WS 帧级加密与 waiting_input 漂移零残留（票 06）', () => {
  it('上层生产面不再有 WS 加密开关字段/通道名/设置页行/i18n key（linkCrypto 库自身除外）', async () => {
    const [enc, view, zh, en] = await Promise.all([
      readFile('src/composables/useLinkEncryption.ts', 'utf-8'),
      readFile('src/views/settings/ConnectionSettingsView.vue', 'utf-8'),
      readFile('src/locales/zh-CN/settings.ts', 'utf-8'),
      readFile('src/locales/en/settings.ts', 'utf-8'),
    ])
    // useLinkEncryption：无 ws 字段、无 ws 通道字面量（带引号的字面量；文件头注释的
    // 「ws-terminal / ws-event」说辞是退役说明，不带引号故不匹配）
    expect(enc).not.toContain('encryptWsTerminal')
    expect(enc).not.toContain('encryptWsEvent')
    expect(enc).not.toContain("'ws-terminal'")
    expect(enc).not.toContain("'ws-event'")
    expect(enc).not.toMatch(/LinkCryptoChannel = '[^']*\|/)
    // 设置页：不再渲染 WS 子开关行 / 引用退役 i18n key
    expect(view).not.toContain('ws-terminal')
    expect(view).not.toContain('ws-event')
    expect(view).not.toContain('linkEncryptWsTerminal')
    expect(view).not.toContain('linkEncryptWsEvent')
    // i18n 双文件：退役 key 同步删除
    for (const locales of [zh, en]) {
      expect(locales).not.toContain('linkEncryptWsTerminal')
      expect(locales).not.toContain('linkEncryptWsEvent')
    }
  })

  // 旧宿主 SessionsView / SessionCard 随阶段 B 退役：会话状态展示面已迁
  // terminal-session 宿主页域（host/utils.ts 的状态键表 + host/components/SessionsSection.vue）
  it('会话状态比较统一到桌面 wire 字面量 waitingInput（snake_case 漂移零残留）', async () => {
    const [mc, utils, section] = await Promise.all([
      readFile('src/composables/useMobileConnection.ts', 'utf-8'),
      readFile('wasm-apps/terminal-session/src/host/utils.ts', 'utf-8'),
      readFile('wasm-apps/terminal-session/src/host/components/SessionsSection.vue', 'utf-8'),
    ])
    // 反例（三方一致）：任何一面都不允许出现已退役的 snake_case 字面量
    for (const src of [mc, utils, section]) {
      expect(src).not.toContain("'waiting_input'")
    }
    // 正例：状态键真源（宿主连接域 + 插件宿主页域的键表）必须用 wire 字面量 waitingInput
    for (const src of [mc, utils]) {
      expect(src).toContain('waitingInput')
    }
  })
})
