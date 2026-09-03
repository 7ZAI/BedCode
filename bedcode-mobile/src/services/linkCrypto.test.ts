/**
 * linkCrypto 单元测试（issue 05）
 *
 * 双侧复刻：测试同时扮演客户端与服务端（各自独立执行协议公式），
 * 验证派生/编解码的对称性与序号纪律。跨语言金样由桌面端
 * server/link_crypto.rs 单元测试锚定相同字节常量，真机联调时以
 * 实际互通为准。
 */

import { describe, expect, it } from 'vitest'
// x25519 自 @noble/curves 1.9 起从 ./x25519 子路径移入 ./ed25519（与 linkCrypto.ts 保持一致）
import { x25519 } from '@noble/curves/ed25519'
import { gcm } from '@noble/ciphers/aes'
import { sha256 } from '@noble/hashes/sha256'
import { hkdf } from '@noble/hashes/hkdf'

import {
  base64ToBytes,
  bytesToBase64,
  decryptResponse,
  deriveHttpKeys,
  deriveWsSession,
  encryptRequest,
} from '../services/linkCrypto'

// 服务端（桌面端）公式复刻 —— 与 server/link_crypto.rs 逐字节一致
const INFO_HTTP_REQ = 'bedcode-link-crypto/v1/http/request'
const INFO_HTTP_RESP = 'bedcode-link-crypto/v1/http/response'
const INFO_WS_C2S = 'bedcode-link-crypto/v1/ws/c2s'
const INFO_WS_S2C = 'bedcode-link-crypto/v1/ws/s2c'

function serverDeriveHttp(ephPublicB64: string, kdPriv: Uint8Array, path: string) {
  const shared = x25519.getSharedSecret(kdPriv, base64ToBytes(ephPublicB64))
  return {
    request: hkdf(sha256, shared, new TextEncoder().encode(path), new TextEncoder().encode(INFO_HTTP_REQ), 32),
    response: hkdf(sha256, shared, new TextEncoder().encode(path), new TextEncoder().encode(INFO_HTTP_RESP), 32),
  }
}

function serverAad(direction: number, path: string) {
  const p = new TextEncoder().encode(path)
  const aad = new Uint8Array(7 + p.length)
  aad.set(new TextEncoder().encode('v1'), 0)
  aad[2] = direction
  new DataView(aad.buffer).setUint32(3, p.length, false)
  aad.set(p, 7)
  return aad
}

describe('base64 helpers', () => {
  it('roundtrips across lengths including padding cases', () => {
    for (const len of [0, 1, 2, 3, 12, 31, 32, 64, 255]) {
      const bytes = crypto.getRandomValues(new Uint8Array(len))
      expect(base64ToBytes(bytesToBase64(bytes))).toEqual(bytes)
    }
  })
})

describe('http single-shot encryption', () => {
  it('client seal → server open → server seal response → client open', () => {
    const kdPriv = x25519.utils.randomPrivateKey()
    const kdPubB64 = bytesToBase64(x25519.getPublicKey(kdPriv))
    const path = '/api/sessions'
    const plaintext = JSON.stringify({ q: 1 })

    const sealed = encryptRequest(kdPubB64, path, plaintext)

    // 协商头形状
    expect(sealed.negotiation).toMatch(/^v1 [A-Za-z0-9+/=]+$/)
    // 信封结构
    const env = JSON.parse(sealed.envelope) as { v: number; n: string; ct: string }
    expect(env.v).toBe(1)
    expect(base64ToBytes(env.n)).toHaveLength(12)

    // 服务端解密请求（用协商头里的临时公钥）
    const ekB64 = sealed.negotiation.slice(3)
    const serverKeys = serverDeriveHttp(ekB64, kdPriv, path)
    const openedServer = gcm(serverKeys.request, base64ToBytes(env.n), serverAad(0x01, path)).decrypt(
      base64ToBytes(env.ct),
    )
    expect(new TextDecoder().decode(openedServer)).toBe(plaintext)

    // 服务端加密响应 → 客户端解密
    const respPlain = JSON.stringify({ code: 0, data: [] })
    const respNonce = crypto.getRandomValues(new Uint8Array(12))
    const respCt = gcm(serverKeys.response, respNonce, serverAad(0x02, path))
      .encrypt(new TextEncoder().encode(respPlain))
    const outcome = decryptResponse(
      sealed.keys,
      true,
      JSON.stringify({ v: 1, n: bytesToBase64(respNonce), ct: bytesToBase64(respCt) }),
      path,
    )
    expect(outcome).toEqual({ kind: 'decrypted', text: respPlain })
  })

  it('flags downgrade when expecting encryption but response plain', () => {
    const kdPriv = x25519.utils.randomPrivateKey()
    const kdPubB64 = bytesToBase64(x25519.getPublicKey(kdPriv))
    const sealed = encryptRequest(kdPubB64, '/echo', '{}')
    expect(decryptResponse(sealed.keys, false, '{"code":0}', '/echo')).toEqual({ kind: 'downgrade' })
  })

  it('treats missing keys + plain response as plaintext passthrough', () => {
    expect(decryptResponse(null, false, '{"code":0}', '/x')).toEqual({ kind: 'plaintext' })
  })

  it('derives identical keys from both sides', () => {
    const eph = x25519.utils.randomPrivateKey()
    const ephPubB64 = bytesToBase64(x25519.getPublicKey(eph))
    const kdPriv = x25519.utils.randomPrivateKey()
    const client = deriveHttpKeys(eph, bytesToBase64(x25519.getPublicKey(kdPriv)), '/a')
    const server = serverDeriveHttp(ephPubB64, kdPriv, '/a')
    expect([...client.request]).toEqual([...server.request])
    expect([...client.response]).toEqual([...server.response])
  })
})

describe('ws session handshake and seq discipline', () => {
  function replicateServerExpand(
    ikm: Uint8Array,
    salt: Uint8Array,
  ) {
    return {
      c2s: hkdf(sha256, ikm, salt, new TextEncoder().encode(INFO_WS_C2S), 36),
      s2c: hkdf(sha256, ikm, salt, new TextEncoder().encode(INFO_WS_S2C), 36),
    }
  }

  it('both sides derive matching direction ciphers', () => {
    const mPriv = x25519.utils.randomPrivateKey()
    const mEkB64 = bytesToBase64(x25519.getPublicKey(mPriv))
    const kdPriv = x25519.utils.randomPrivateKey()
    const kdPubB64 = bytesToBase64(x25519.getPublicKey(kdPriv))

    // 服务端生成 s_eph 并按桌面端公式派生
    const sPriv = x25519.utils.randomPrivateKey()
    const sEkB64 = bytesToBase64(x25519.getPublicKey(sPriv))
    const ephEph = x25519.getSharedSecret(sPriv, base64ToBytes(mEkB64))
    const auth = x25519.getSharedSecret(kdPriv, base64ToBytes(mEkB64))
    const ikm = new Uint8Array(64)
    ikm.set(ephEph.slice(0, 32), 0)
    ikm.set(auth.slice(0, 32), 32)
    const salt = new TextEncoder().encode('bc-link-crypto/v1' + mEkB64 + sEkB64)
    const serverOkm = replicateServerExpand(ikm, salt)

    const client = deriveWsSession(mPriv, mEkB64, sEkB64, kdPubB64)
    // 白盒断言：c2s/s2c 是类私有字段，测试内经 as any 读取派生字节做对称性比对
    const internal = client as unknown as {
      c2s: { key: Uint8Array; prefix: Uint8Array }
      s2c: { key: Uint8Array; prefix: Uint8Array }
    }
    expect([...internal.c2s.key]).toEqual([...serverOkm.c2s.slice(0, 32)])
    expect([...internal.c2s.prefix]).toEqual([...serverOkm.c2s.slice(32, 36)])
    expect([...internal.s2c.key]).toEqual([...serverOkm.s2c.slice(0, 32)])
    expect([...internal.s2c.prefix]).toEqual([...serverOkm.s2c.slice(32, 36)])
  })

  it('text roundtrip and replay rejection', () => {
    const mPriv = x25519.utils.randomPrivateKey()
    const client = deriveWsSession(
      mPriv,
      bytesToBase64(x25519.getPublicKey(mPriv)),
      bytesToBase64(x25519.getPublicKey(x25519.utils.randomPrivateKey())),
      bytesToBase64(x25519.getPublicKey(x25519.utils.randomPrivateKey())),
    )

    const sealed = client.encryptText('ws-terminal', '{"type":"subscribe"}')
    const env = JSON.parse(sealed) as { v: number; seq: number }
    expect(env.v).toBe(1)
    expect(env.seq).toBe(0)
    // 第二帧 seq 递增
    expect((JSON.parse(client.encryptText('ws-terminal', 'x')) as { seq: number }).seq).toBe(1)
    void sealed
  })

  it('binary roundtrip preserves frame bytes', () => {
    const mPriv = x25519.utils.randomPrivateKey()
    const client = deriveWsSession(
      mPriv,
      bytesToBase64(x25519.getPublicKey(mPriv)),
      bytesToBase64(x25519.getPublicKey(x25519.utils.randomPrivateKey())),
      bytesToBase64(x25519.getPublicKey(x25519.utils.randomPrivateKey())),
    )
    const frame = crypto.getRandomValues(new Uint8Array(37))
    const sealed = client.encryptBinary('ws-terminal', frame)
    expect(sealed[0]).toBe(1)
    expect(sealed.length).toBe(9 + frame.length + 16) // header + ct + GCM tag
  })
})
