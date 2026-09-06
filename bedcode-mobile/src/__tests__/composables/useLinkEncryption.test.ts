/**
 * useLinkEncryption 单元测试（issue 08）
 *
 * 覆盖配置持久化往返、通道子开关粒度判定与 pin 依赖：
 * isChannelEncryptionActive = 主开关 ∧ 对应通道子开关 ∧ 已 pin。
 */

import { beforeEach, describe, expect, it } from 'vitest'
import {
  isChannelEncryptionActive,
  getPinnedKey,
  getPinnedFingerprint,
  notePinFromAuthData,
  useLinkEncryptionSettings,
} from '@/composables/useLinkEncryption'

beforeEach(() => {
  localStorage.clear()
  // 模块级单例跨用例残留：localStorage 清理不影响内存态，需显式重置回默认值
  const s = useLinkEncryptionSettings().settings
  s.value.enabled = false
  s.value.strictMode = false
  s.value.encryptHttp = true
  s.value.encryptWsTerminal = true
  s.value.encryptWsEvent = true
})

describe('useLinkEncryptionSettings', () => {
  it('默认值：功能整体关，通道子开关全开', () => {
    const { settings } = useLinkEncryptionSettings()

    expect(settings.value.enabled).toBe(false)
    expect(settings.value.encryptHttp).toBe(true)
    expect(settings.value.encryptWsTerminal).toBe(true)
    expect(settings.value.encryptWsEvent).toBe(true)
  })

  it('setEnabled / setChannel / setStrictMode 变更并落盘，重载后保留', () => {
    const s1 = useLinkEncryptionSettings()
    s1.setEnabled(true)
    s1.setStrictMode(true)
    s1.setChannel('ws-terminal', false)

    // 新读取方应看到同一单例的持久化状态
    const s2 = useLinkEncryptionSettings()
    expect(s2.settings.value.enabled).toBe(true)
    expect(s2.settings.value.strictMode).toBe(true)
    expect(s2.settings.value.encryptWsTerminal).toBe(false)
    expect(JSON.parse(localStorage.getItem('link-encryption')!).encryptWsTerminal).toBe(false)
  })

  it('isChannelEncryptionActive：主开关关时一律 false（即使已 pin）', () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    const { setEnabled } = useLinkEncryptionSettings()
    setEnabled(false)
    expect(isChannelEncryptionActive('http')).toBe(false)
    expect(isChannelEncryptionActive('ws-terminal')).toBe(false)
    expect(getPinnedKey()).not.toBeNull()
  })

  it('isChannelEncryptionActive：主开+pin 下按通道子开关独立判定', () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    const { setEnabled, setChannel } = useLinkEncryptionSettings()
    setEnabled(true)

    setChannel('http', false)
    expect(isChannelEncryptionActive('http')).toBe(false)
    expect(isChannelEncryptionActive('ws-terminal')).toBe(true)
    expect(isChannelEncryptionActive('ws-event')).toBe(true)

    setChannel('http', true)
    setChannel('ws-terminal', false)
    setChannel('ws-event', false)
    expect(isChannelEncryptionActive('http')).toBe(true)
    expect(isChannelEncryptionActive('ws-terminal')).toBe(false)
    expect(isChannelEncryptionActive('ws-event')).toBe(false)
  })

  it('未 pin 时即使全开也不参与加密（协商无信任锚）', () => {
    const { setEnabled } = useLinkEncryptionSettings()
    setEnabled(true)
    expect(getPinnedKey()).toBeNull()
    expect(isChannelEncryptionActive('http')).toBe(false)
  })
})

describe('notePinFromAuthData', () => {
  it('auth 数据携带公钥+指纹时两者都写入', () => {
    notePinFromAuthData({ kdPublicB64: 'cHVibGljLWtleQ==', kdFingerprint: 'aabbccdd' })
    expect(getPinnedKey()).toBe('cHVibGljLWtleQ==')
    expect(getPinnedFingerprint()).toBe('aabbccdd')
  })

  it('指纹缺失时公钥仍写入（公钥为协商必需，指纹仅展示）', () => {
    notePinFromAuthData({ kdPublicB64: 'cHVibGljLWtleQ==' })
    expect(getPinnedKey()).toBe('cHVibGljLWtleQ==')
    expect(getPinnedFingerprint()).toBeNull()
  })

  it('无公钥不写任何 pin（防空串污染）', () => {
    localStorage.setItem('link_kd_public_b64', 'cHVibGljLWtleQ==')
    localStorage.setItem('link_kd_fingerprint', 'aabbccdd')
    notePinFromAuthData({})
    // 既有 pin 不被清空（协商失败不清 pin 的信任锚语义）
    expect(getPinnedKey()).toBe('cHVibGljLWtleQ==')
    expect(getPinnedFingerprint()).toBe('aabbccdd')
  })
})
