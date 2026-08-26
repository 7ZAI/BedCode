/**
 * Link Crypto — 局域网链路报文加密（移动端 TS 实现，issue 05）
 *
 * 与桌面端 `bedcode-desktop/src-tauri/src/server/link_crypto.rs` 协议互为镜像，
 * 以下字节级约定必须逐项一致（改动任何一侧必须同步另一侧并跑双端金样）：
 *
 * - HTTP 信封 JSON：{ v:1, n:<b64 12B>, ct:<b64 ct||tag> }
 * - HTTP AAD："v1" || dir(1B: req=0x01/resp=0x02) || u32be(pathLen) || path
 * - HTTP 密钥：shared = X25519(临时私钥, Kd公钥)；
 *   HKDF-SHA256(salt=pathASCII, info="bedcode-link-crypto/v1/http/request|response", 32B)
 * - WS 握手：IKM = ECDH(m,s) ‖ ECDH(m,Kd)，salt = "bc-link-crypto/v1" ‖ m_ek_b64 ‖ s_ek_b64；
 *   expand(c2s/s2c info, 36B) = key(32) + noncePrefix(4)
 * - WS 二进制帧：[ver=1][seq u64be][ct||tag]；nonce = prefix ‖ u64be(seq)；序号严格单调
 * - WS 文本帧信封：{ v:1, seq, n, ct }；WS AAD："v1" ‖ channelStr ‖ dir ‖ origin
 * - 算法库：@noble/curves(x25519) + @noble/ciphers(AES-256-GCM，密文尾部拼 tag) + @noble/hashes
 */

// x25519 自 @noble/curves 1.9 起从 ./x25519 子路径移入 ./ed25519（该路径在 1.8.x 同样可用）
import { x25519 } from '@noble/curves/ed25519'
import { gcm } from '@noble/ciphers/aes'
import { sha256 } from '@noble/hashes/sha256'
import { hkdf } from '@noble/hashes/hkdf'

// ==================== 协议常量（与桌面端逐字节一致） ====================

const HKDF_INFO_HTTP_REQUEST = 'bedcode-link-crypto/v1/http/request'
const HKDF_INFO_HTTP_RESPONSE = 'bedcode-link-crypto/v1/http/response'
const HKDF_INFO_WS_C2S = 'bedcode-link-crypto/v1/ws/c2s'
const HKDF_INFO_WS_S2C = 'bedcode-link-crypto/v1/ws/s2c'
const WS_TRANSCRIPT_PREFIX = 'bc-link-crypto/v1'

const DIR_REQUEST = 0x01
const DIR_RESPONSE = 0x02
const DIR_C2S = 0x01
const DIR_S2C = 0x02
const ORIGIN_TEXT = 0x01
const ORIGIN_BINARY = 0x02

const NONCE_LEN = 12
const WS_BINARY_HEADER_LEN = 9
const WS_FRAME_VERSION = 1

/** WS 通道标识字符串（与桌面端 TrafficChannel::as_str 一致） */
export type WsChannel = 'ws-terminal' | 'ws-event'

// ==================== Base64（std 字母表，UTF-8 安全） ====================

const B64_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'

export function bytesToBase64(bytes: Uint8Array): string {
  let out = ''
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i]
    const b1 = i + 1 < bytes.length ? bytes[i + 1] : 0
    const b2 = i + 2 < bytes.length ? bytes[i + 2] : 0
    out += B64_ALPHABET[b0 >> 2]
    out += B64_ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)]
    out += i + 1 < bytes.length ? B64_ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] : '='
    out += i + 2 < bytes.length ? B64_ALPHABET[b2 & 0x3f] : '='
  }
  return out
}

export function base64ToBytes(text: string): Uint8Array {
  const clean = text.trim()
  let padding = 0
  if (clean.endsWith('==')) padding = 2
  else if (clean.endsWith('=')) padding = 1
  const len = (clean.length / 4) * 3 - padding
  const out = new Uint8Array(len)
  let p = 0
  let buffer = 0
  let bits = 0
  for (const ch of clean) {
    if (ch === '=') break
    const idx = B64_ALPHABET.indexOf(ch)
    if (idx < 0) throw new Error(`base64 invalid char: ${ch}`)
    buffer = (buffer << 6) | idx
    bits += 6
    if (bits >= 8) {
      bits -= 8
      if (p < len) out[p++] = (buffer >> bits) & 0xff
    }
  }
  return out
}

function utf8(text: string): Uint8Array {
  return new TextEncoder().encode(text)
}

function randomBytes(len: number): Uint8Array {
  const out = new Uint8Array(len)
  crypto.getRandomValues(out)
  return out
}

// ==================== HTTP 单发加密 ====================

export interface EphemeralKeyPair {
  priv: Uint8Array
  pubB64: string
}

/** 每请求全新的临时 X25519 密钥对 */
export function generateEphemeral(): EphemeralKeyPair {
  const priv = x25519.utils.randomPrivateKey()
  return { priv, pubB64: bytesToBase64(x25519.getPublicKey(priv)) }
}

export interface HttpTrafficKeys {
  request: Uint8Array
  response: Uint8Array
}

/** 由临时私钥与 pin 的 Kd 公钥派生两方向会话密钥 */
export function deriveHttpKeys(ephemeralPriv: Uint8Array, kdPublicB64: string, path: string): HttpTrafficKeys {
  const kdPub = base64ToBytes(kdPublicB64)
  if (kdPub.length !== 32) throw new Error(`kd public key length ${kdPub.length} != 32`)
  const shared = x25519.getSharedSecret(ephemeralPriv, kdPub)
  const salt = utf8(path)
  return {
    request: hkdf(sha256, shared, salt, utf8(HKDF_INFO_HTTP_REQUEST), 32),
    response: hkdf(sha256, shared, salt, utf8(HKDF_INFO_HTTP_RESPONSE), 32),
  }
}

function httpAad(direction: number, path: string): Uint8Array {
  const pathBytes = utf8(path)
  const aad = new Uint8Array(7 + pathBytes.length)
  aad.set(utf8('v1'), 0)
  aad[2] = direction
  new DataView(aad.buffer).setUint32(3, pathBytes.length, false)
  aad.set(pathBytes, 7)
  return aad
}

function aesGcmEncrypt(key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array, aad: Uint8Array): Uint8Array {
  return gcm(key, nonce, aad).encrypt(plaintext)
}

function aesGcmDecrypt(key: Uint8Array, nonce: Uint8Array, ciphertext: Uint8Array, aad: Uint8Array): Uint8Array {
  return gcm(key, nonce, aad).decrypt(ciphertext)
}

export interface SealedHttpRequest {
  /** X-BedCode-Crypto 头值："v1 <ek_b64>" */
  negotiation: string
  /** 信封 JSON 文本（作为请求体发送） */
  envelope: string
  /** 响应解密所需的会话密钥（请求作用域） */
  keys: HttpTrafficKeys
}

/** 加密一条请求：生成全新临时密钥对 + 随机 nonce（spec §3 无状态单发） */
export function encryptRequest(kdPublicB64: string, path: string, plaintext: string): SealedHttpRequest {
  const eph = generateEphemeral()
  const keys = deriveHttpKeys(eph.priv, kdPublicB64, path)
  const nonce = randomBytes(NONCE_LEN)
  const ciphertext = aesGcmEncrypt(keys.request, nonce, utf8(plaintext), httpAad(DIR_REQUEST, path))
  const envelope = JSON.stringify({ v: 1, n: bytesToBase64(nonce), ct: bytesToBase64(ciphertext) })
  return { negotiation: `v1 ${eph.pubB64}`, envelope, keys }
}

export type HttpResponseCrypto =
  | { kind: 'decrypted'; text: string }
  | { kind: 'plaintext' }
  | { kind: 'downgrade' }

/** 解密响应体：桌面端带标记头且我方有请求作用域密钥 → 解信封；
 *  我方预期加密而响应明文 → downgrade（调用方按 strict 配置裁决） */
export function decryptResponse(
  keys: HttpTrafficKeys | null,
  responseHasCryptoHeader: boolean,
  bodyText: string,
  path: string,
): HttpResponseCrypto {
  if (!responseHasCryptoHeader) {
    return keys ? { kind: 'downgrade' } : { kind: 'plaintext' }
  }
  if (!keys) throw new Error('crypto response but no request-scoped keys')
  const env = JSON.parse(bodyText) as { v: number; n: string; ct: string }
  if (env.v !== 1) throw new Error(`unsupported envelope version ${env.v}`)
  const nonce = base64ToBytes(env.n)
  if (nonce.length !== NONCE_LEN) throw new Error(`nonce length ${nonce.length} != 12`)
  const ciphertext = base64ToBytes(env.ct)
  const plain = aesGcmDecrypt(keys.response, nonce, ciphertext, httpAad(DIR_RESPONSE, path))
  return { kind: 'decrypted', text: new TextDecoder().decode(plain) }
}

// ==================== WS 会话加密 ====================

export interface WsDirectionCipher {
  key: Uint8Array
  prefix: Uint8Array
}

function deriveWsDirection(
  transcriptSalt: Uint8Array,
  ikm: Uint8Array,
  info: string,
): WsDirectionCipher {
  const okm = hkdf(sha256, ikm, transcriptSalt, utf8(info), 36)
  return { key: okm.slice(0, 32), prefix: okm.slice(32, 36) }
}

/**
 * 客户端侧握手派生（收到 auth_ok.crypto 回执后调用）。
 * @param myEphemeralPriv 发起 auth 时用的临时私钥
 * @param myEkB64 发起时发出的临时公钥 base64（transcript 绑定，必须用原始串）
 * @param serverEkB64 回执携带的服务端临时公钥
 */
export function deriveWsSession(
  myEphemeralPriv: Uint8Array,
  myEkB64: string,
  serverEkB64: string,
  kdPublicB64: string,
): WsSessionCrypto {
  const sPub = base64ToBytes(serverEkB64)
  const kdPub = base64ToBytes(kdPublicB64)
  if (sPub.length !== 32 || kdPub.length !== 32) throw new Error('handshake public key length != 32')
  const ephEph = x25519.getSharedSecret(myEphemeralPriv, sPub)
  const auth = x25519.getSharedSecret(myEphemeralPriv, kdPub)
  const ikm = new Uint8Array(64)
  ikm.set(ephEph.slice(0, 32), 0)
  ikm.set(auth.slice(0, 32), 32)
  const salt = utf8(WS_TRANSCRIPT_PREFIX + myEkB64 + serverEkB64)
  return new WsSessionCrypto(
    deriveWsDirection(salt, ikm, HKDF_INFO_WS_C2S),
    deriveWsDirection(salt, ikm, HKDF_INFO_WS_S2C),
  )
}

function wsAad(channel: WsChannel, direction: number, origin: number): Uint8Array {
  const ch = utf8(channel)
  const aad = new Uint8Array(2 + ch.length + 2)
  aad.set(utf8('v1'), 0)
  aad.set(ch, 2)
  aad[2 + ch.length] = direction
  aad[3 + ch.length] = origin
  return aad
}

interface TextEnvelope {
  v: number
  seq: number
  n: string
  ct: string
}

/**
 * 一条 WS 连接的客户端侧密码状态（发送/接收序号严格单调；
 * 重连即重新握手派生新实例，序号归零）
 */
export class WsSessionCrypto {
  private c2sNextSeq = 0
  private s2cExpectedSeq = 0

  constructor(
    private c2s: WsDirectionCipher,
    private s2c: WsDirectionCipher,
  ) {}

  /** 加密出站文本帧（控制/业务 JSON），返回信封 JSON 文本 */
  encryptText(channel: WsChannel, text: string): string {
    const seq = this.c2sNextSeq
    const nonce = randomBytes(NONCE_LEN)
    const ciphertext = aesGcmEncrypt(this.c2s.key, nonce, utf8(text), wsAad(channel, DIR_C2S, ORIGIN_TEXT))
    this.c2sNextSeq = seq + 1
    return JSON.stringify({ v: WS_FRAME_VERSION, seq, n: bytesToBase64(nonce), ct: bytesToBase64(ciphertext) })
  }

  /** 解密入站文本帧；seq 不连续 / GCM 校验失败即抛错（调用方断连） */
  decryptText(channel: WsChannel, envelopeText: string): string {
    const env = JSON.parse(envelopeText) as TextEnvelope
    if (env.v !== WS_FRAME_VERSION) throw new Error(`unsupported ws frame version ${env.v}`)
    if (env.seq !== this.s2cExpectedSeq) {
      throw new Error(`ws seq mismatch: expected ${this.s2cExpectedSeq}, got ${env.seq}`)
    }
    const nonce = base64ToBytes(env.n)
    if (nonce.length !== NONCE_LEN) throw new Error(`nonce length ${nonce.length} != 12`)
    const plain = aesGcmDecrypt(
      this.s2c.key,
      nonce,
      base64ToBytes(env.ct),
      wsAad(channel, DIR_S2C, ORIGIN_TEXT),
    )
    this.s2cExpectedSeq += 1
    return new TextDecoder().decode(plain)
  }

  /** 加密出站二进制帧（背压 ack）→ [ver][seq u64be][ct] */
  encryptBinary(channel: WsChannel, frame: Uint8Array): Uint8Array {
    const seq = this.c2sNextSeq
    const nonce = wsNonce(this.c2s.prefix, seq)
    const ciphertext = aesGcmEncrypt(this.c2s.key, nonce, frame, wsAad(channel, DIR_C2S, ORIGIN_BINARY))
    this.c2sNextSeq = seq + 1
    const out = new Uint8Array(WS_BINARY_HEADER_LEN + ciphertext.length)
    out[0] = WS_FRAME_VERSION
    new DataView(out.buffer).setBigUint64(1, BigInt(seq), false)
    out.set(ciphertext, WS_BINARY_HEADER_LEN)
    return out
  }

  /** 解密入站二进制帧（TBv2 输出流）；seq 纪律同文本帧 */
  decryptBinary(channel: WsChannel, data: Uint8Array): Uint8Array {
    if (data.length < WS_BINARY_HEADER_LEN) throw new Error(`ws binary frame too short: ${data.length}`)
    if (data[0] !== WS_FRAME_VERSION) throw new Error(`unsupported ws frame version ${data[0]}`)
    const seq = Number(new DataView(data.buffer, data.byteOffset).getBigUint64(1, false))
    if (seq !== this.s2cExpectedSeq) {
      throw new Error(`ws seq mismatch: expected ${this.s2cExpectedSeq}, got ${seq}`)
    }
    const plain = aesGcmDecrypt(
      this.s2c.key,
      wsNonce(this.s2c.prefix, seq),
      data.slice(WS_BINARY_HEADER_LEN),
      wsAad(channel, DIR_S2C, ORIGIN_BINARY),
    )
    this.s2cExpectedSeq += 1
    return plain
  }
}

function wsNonce(prefix: Uint8Array, seq: number): Uint8Array {
  const nonce = new Uint8Array(NONCE_LEN)
  nonce.set(prefix.slice(0, 4), 0)
  const view = new DataView(nonce.buffer)
  view.setBigUint64(4, BigInt(seq), false)
  return nonce
}

/** 从 auth_ok 控制帧提取加密协商回执（无则 null = 服务端未接受协商） */
export function parseCryptoEcho(authOkMsg: { crypto?: { v?: number; ek?: string } }): { v: number; ek: string } | null {
  const c = authOkMsg.crypto
  if (!c || typeof c.ek !== 'string' || c.ek.length === 0) return null
  return { v: typeof c.v === 'number' ? c.v : 1, ek: c.ek }
}
