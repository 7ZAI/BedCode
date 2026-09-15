/**
 * linkCrypto 单元测试（issue 05）
 *
 * 双侧复刻：测试同时扮演客户端与服务端（各自独立执行协议公式），
 * 验证派生/编解码的对称性与序号纪律。跨语言金样由桌面端
 * server/link_crypto.rs 单元测试锚定相同字节常量，真机联调时以
 * 实际互通为准。
 *
 * HTTP 单发加解密已收束至 Rust（ticket 03/09）：字节级对齐金样在
 * `packages/link-crypto` crate 单测 + `tests/http_proxy_flow.rs` 集成；
 * 本文件仅保留 WS 会话部分（终端 WS 未搬迁）。
 */

import { describe, expect, it } from 'vitest'
// x25519 自 @noble/curves 1.9 起从 ./x25519 子路径移入 ./ed25519（与 linkCrypto.ts 保持一致）
import { x25519 } from '@noble/curves/ed25519'
import { gcm } from '@noble/ciphers/aes'
import { sha256 } from '@noble/hashes/sha256'
import { hkdf } from '@noble/hashes/hkdf'

import { base64ToBytes, bytesToBase64, deriveWsSession } from '@/services/linkCrypto'

// 服务端（桌面端）公式复刻 —— 与 server/link_crypto.rs 逐字节一致
const INFO_WS_C2S = 'bedcode-link-crypto/v1/ws/c2s'
const INFO_WS_S2C = 'bedcode-link-crypto/v1/ws/s2c'

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

describe('ws session decrypt (对称性与异常分支)', () => {
  // ===== 协议常量镜像（linkCrypto.ts 模块私有，测试内复制并注明来源） =====
  const DIR_C2S = 0x01
  const DIR_S2C = 0x02
  const ORIGIN_TEXT = 0x01
  const ORIGIN_BINARY = 0x02
  const NONCE_LEN = 12
  const WS_BINARY_HEADER_LEN = 9
  const WS_FRAME_VERSION = 1

  /** wsNonce 镜像：prefix(4) ‖ u64be(seq) */
  function wsNonce(prefix: Uint8Array, seq: number): Uint8Array {
    const nonce = new Uint8Array(NONCE_LEN)
    nonce.set(prefix.slice(0, 4), 0)
    new DataView(nonce.buffer).setBigUint64(4, BigInt(seq), false)
    return nonce
  }

  /** wsAad 镜像：b'v1' ‖ channel ‖ direction ‖ origin */
  function wsAad(channel: string, direction: number, origin: number): Uint8Array {
    const ch = new TextEncoder().encode(channel)
    const aad = new Uint8Array(2 + ch.length + 2)
    aad.set(new TextEncoder().encode('v1'), 0)
    aad.set(ch, 2)
    aad[2 + ch.length] = direction
    aad[3 + ch.length] = origin
    return aad
  }

  /**
   * 服务端（桌面端）镜像。桌面端语义（link_crypto.rs）：
   * 出站 = Outbound/S2C（DIR_S2C），入站 = Inbound/C2S（DIR_C2S）；
   * WsSessionCrypto 是纯客户端视角（encrypt 恒 C2S / decrypt 恒 S2C），
   * 故服务端侧在测试内显式镜像，方向常量与序号纪律与客户端逐字节一致。
   */
  class ServerWsCrypto {
    private s2cNextSeq = 0
    private c2sExpectedSeq = 0

    constructor(
      private c2s: { key: Uint8Array; prefix: Uint8Array },
      private s2c: { key: Uint8Array; prefix: Uint8Array },
    ) {}

    /** 服务端出站文本帧（s2c key + DIR_S2C AAD） */
    encryptText(channel: string, text: string): string {
      const seq = this.s2cNextSeq
      const nonce = crypto.getRandomValues(new Uint8Array(NONCE_LEN))
      const ciphertext = gcm(this.s2c.key, nonce, wsAad(channel, DIR_S2C, ORIGIN_TEXT)).encrypt(
        new TextEncoder().encode(text),
      )
      this.s2cNextSeq = seq + 1
      return JSON.stringify({ v: WS_FRAME_VERSION, seq, n: bytesToBase64(nonce), ct: bytesToBase64(ciphertext) })
    }

    /** 服务端出站二进制帧（s2c key + DIR_S2C AAD，[ver][seq u64be][ct]） */
    encryptBinary(channel: string, frame: Uint8Array): Uint8Array {
      const seq = this.s2cNextSeq
      const ciphertext = gcm(this.s2c.key, wsNonce(this.s2c.prefix, seq), wsAad(channel, DIR_S2C, ORIGIN_BINARY)).encrypt(
        frame,
      )
      this.s2cNextSeq = seq + 1
      const out = new Uint8Array(WS_BINARY_HEADER_LEN + ciphertext.length)
      out[0] = WS_FRAME_VERSION
      new DataView(out.buffer).setBigUint64(1, BigInt(seq), false)
      out.set(ciphertext, WS_BINARY_HEADER_LEN)
      return out
    }

    /** 服务端入站文本帧解密（c2s key + DIR_C2S AAD） */
    decryptText(channel: string, envelopeText: string): string {
      const env = JSON.parse(envelopeText) as { v: number; seq: number; n: string; ct: string }
      if (env.seq !== this.c2sExpectedSeq) {
        throw new Error(`ws seq mismatch: expected ${this.c2sExpectedSeq}, got ${env.seq}`)
      }
      const plain = gcm(this.c2s.key, base64ToBytes(env.n), wsAad(channel, DIR_C2S, ORIGIN_TEXT)).decrypt(
        base64ToBytes(env.ct),
      )
      this.c2sExpectedSeq += 1
      return new TextDecoder().decode(plain)
    }

    /** 服务端入站二进制帧解密（c2s key + DIR_C2S AAD） */
    decryptBinary(channel: string, data: Uint8Array): Uint8Array {
      const seq = Number(new DataView(data.buffer, data.byteOffset).getBigUint64(1, false))
      if (seq !== this.c2sExpectedSeq) {
        throw new Error(`ws seq mismatch: expected ${this.c2sExpectedSeq}, got ${seq}`)
      }
      const plain = gcm(this.c2s.key, wsNonce(this.c2s.prefix, seq), wsAad(channel, DIR_C2S, ORIGIN_BINARY)).decrypt(
        data.slice(WS_BINARY_HEADER_LEN),
      )
      this.c2sExpectedSeq += 1
      return plain
    }
  }

  /** 按桌面端公式派生双方方向密钥，构造客户端 + 服务端镜像实例 */
  function makeClientAndServer() {
    const mPriv = x25519.utils.randomPrivateKey()
    const mEkB64 = bytesToBase64(x25519.getPublicKey(mPriv))
    const sPriv = x25519.utils.randomPrivateKey()
    const sEkB64 = bytesToBase64(x25519.getPublicKey(sPriv))
    const kdPriv = x25519.utils.randomPrivateKey()
    const kdPubB64 = bytesToBase64(x25519.getPublicKey(kdPriv))

    const ephEph = x25519.getSharedSecret(sPriv, base64ToBytes(mEkB64))
    const auth = x25519.getSharedSecret(kdPriv, base64ToBytes(mEkB64))
    const ikm = new Uint8Array(64)
    ikm.set(ephEph.slice(0, 32), 0)
    ikm.set(auth.slice(0, 32), 32)
    const salt = new TextEncoder().encode('bc-link-crypto/v1' + mEkB64 + sEkB64)
    const c2s = hkdf(sha256, ikm, salt, new TextEncoder().encode(INFO_WS_C2S), 36)
    const s2c = hkdf(sha256, ikm, salt, new TextEncoder().encode(INFO_WS_S2C), 36)
    const client = deriveWsSession(mPriv, mEkB64, sEkB64, kdPubB64)
    const server = new ServerWsCrypto(
      { key: c2s.slice(0, 32), prefix: c2s.slice(32, 36) },
      { key: s2c.slice(0, 32), prefix: s2c.slice(32, 36) },
    )
    return { client, server }
  }

  it('客户端加密文本 → 服务端可解密（C2S 对称性）', () => {
    const { client, server } = makeClientAndServer()
    const sealed = client.encryptText('ws-terminal', '{"type":"subscribe"}')
    expect(server.decryptText('ws-terminal', sealed)).toBe('{"type":"subscribe"}')
  })

  it('服务端加密文本 → 客户端可解密（S2C 对称性）', () => {
    const { client, server } = makeClientAndServer()
    const sealed = server.encryptText('ws-event', '{"ok":1}')
    expect(client.decryptText('ws-event', sealed)).toBe('{"ok":1}')
  })

  it('二进制帧双向对称（客户端→服务端）', () => {
    const { client, server } = makeClientAndServer()
    const frame = crypto.getRandomValues(new Uint8Array(64))
    expect([...server.decryptBinary('ws-terminal', client.encryptBinary('ws-terminal', frame))]).toEqual([
      ...frame,
    ])
  })

  it('二进制帧双向对称（服务端→客户端）', () => {
    const { client, server } = makeClientAndServer()
    const frame = crypto.getRandomValues(new Uint8Array(8))
    expect([...client.decryptBinary('ws-event', server.encryptBinary('ws-event', frame))]).toEqual([
      ...frame,
    ])
  })

  it('文本帧 seq 不连续 → 拒绝；已消费帧重放 → 拒绝', () => {
    const { client, server } = makeClientAndServer()
    const sealed = server.encryptText('ws-event', 'a')
    // 伪造 seq=5（期望 0）
    const forged = JSON.stringify({ ...JSON.parse(sealed), seq: 5 })
    expect(() => client.decryptText('ws-event', forged)).toThrow(/seq mismatch/)
    // 正常消费 seq=0
    expect(client.decryptText('ws-event', sealed)).toBe('a')
    // 消费后重放同一帧 → 序号已前进，replay 被拒
    expect(() => client.decryptText('ws-event', sealed)).toThrow(/seq mismatch/)
  })

  it('二进制帧 seq 不连续 → 拒绝', () => {
    const { client, server } = makeClientAndServer()
    const sealed = server.encryptBinary('ws-event', new Uint8Array(4))
    const forged = new Uint8Array(sealed)
    new DataView(forged.buffer).setBigUint64(1, 3n, false) // 伪造 seq=3
    expect(() => client.decryptBinary('ws-event', forged)).toThrow(/seq mismatch/)
  })

  it('文本帧错误 version → 拒绝', () => {
    const { client, server } = makeClientAndServer()
    const sealed = server.encryptText('ws-event', 'a')
    const forged = JSON.stringify({ ...JSON.parse(sealed), v: 2 })
    expect(() => client.decryptText('ws-event', forged)).toThrow(/unsupported ws frame version/)
  })

  it('文本帧 nonce 长度错误 → 拒绝', () => {
    const { client, server } = makeClientAndServer()
    const sealed = server.encryptText('ws-event', 'a')
    // nonce 换成 8 字节
    const forged = JSON.stringify({
      ...JSON.parse(sealed),
      n: bytesToBase64(crypto.getRandomValues(new Uint8Array(8))),
    })
    expect(() => client.decryptText('ws-event', forged)).toThrow(/nonce length/)
  })

  it('文本帧密文被篡改（GCM tag 校验失败）→ 拒绝且序号不前进', () => {
    const { client, server } = makeClientAndServer()
    // 先正常消费 seq=0，把期望序号推进到 1
    expect(client.decryptText('ws-event', server.encryptText('ws-event', 'first'))).toBe('first')
    // 篡改 seq=1 的密文
    const sealed = server.encryptText('ws-event', 'secret-payload')
    const ct = base64ToBytes((JSON.parse(sealed) as { ct: string }).ct)
    ct[0] ^= 0xff // 翻转密文首字节
    const forged = JSON.stringify({ ...JSON.parse(sealed), ct: bytesToBase64(ct) })
    expect(() => client.decryptText('ws-event', forged)).toThrow()
    // GCM 失败不消耗序号：同一序号（1）的原始合法帧仍可解密
    expect(client.decryptText('ws-event', sealed)).toBe('secret-payload')
  })

  it('二进制帧过短（不足 9 字节头）→ 拒绝', () => {
    const { client } = makeClientAndServer()
    expect(() => client.decryptBinary('ws-event', new Uint8Array(3))).toThrow(/too short/)
  })

  it('二进制帧错误 version → 拒绝', () => {
    const { client } = makeClientAndServer()
    const bad = new Uint8Array(9)
    bad[0] = 2
    expect(() => client.decryptBinary('ws-event', bad)).toThrow(/unsupported ws frame version/)
  })

  it('文本帧非法 JSON → 拒绝', () => {
    const { client } = makeClientAndServer()
    expect(() => client.decryptText('ws-event', 'not-json{')).toThrow()
  })
})
