/**
 * 链路加密配置与 pin 状态（issue 05/06）
 *
 * 配置域镜像桌面端 trafficEncryption 的移动端 HTTP 子集（spec §6）：
 * - enabled：主开关（默认 false，opt-in；无 pin 时开启会被 UI 引导先配对）
 * - strictMode：预期加密而遭降级时断连报错（默认 false → 明文续跑 + 提示）
 * - encryptHttp：HTTP REST 载荷加密（默认开）
 *
 * WS 通道（ws-terminal / ws-event）子开关已随桌面端插件端点帧加密退役
 * （`TrafficChannel::WsPlugin => false`，插件端点帧永不加解密）一并删除——
 * 仅剩 HTTP 信封加密。
 *
 * pin（桌面端身份公钥+指纹）随认证凭据持久化，仅随重新配对/重认证刷新；
 * 协商失败不清除 pin —— 防主动降级攻击抹除信任锚。
 *
 * 存储沿用本端既有 localStorage 惯例（与 auth_session_token 等同域）。
 */

import { ref } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { base64ToBytes } from '@/services/linkCrypto'

const STORAGE_KEY = 'link-encryption'
const PIN_PUBLIC_KEY = 'link_kd_public_b64'
const PIN_FINGERPRINT = 'link_kd_fingerprint'

export interface LinkEncryptionSettings {
  enabled: boolean
  strictMode: boolean
  /** HTTP REST 载荷加密（默认开） */
  encryptHttp: boolean
}

/** 默认值：功能整体关（opt-in），HTTP 载荷加密默认开——用户只需打开主开关即生效 */
const DEFAULTS: LinkEncryptionSettings = {
  enabled: false,
  strictMode: false,
  encryptHttp: true,
}

function loadSettings(): LinkEncryptionSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULTS }
    const parsed = JSON.parse(raw) as Partial<LinkEncryptionSettings>
    return {
      enabled: parsed.enabled === true,
      strictMode: parsed.strictMode === true,
      encryptHttp: parsed.encryptHttp !== false,
    }
  } catch {
    return { ...DEFAULTS }
  }
}

const settings = ref<LinkEncryptionSettings>(loadSettings())

function persist() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value))
  void syncLinkCryptoContextToNative()
}

/** 配置快照（响应式） */
export function useLinkEncryptionSettings() {
  function setEnabled(value: boolean) {
    settings.value.enabled = value
    persist()
  }
  function setStrictMode(value: boolean) {
    settings.value.strictMode = value
    persist()
  }
  /**
   * HTTP 载荷加密子开关（WS 通道子开关已随桌面端插件端点加密退役，
   * 插件端点帧永不加解密；通道维度仅剩 http）
   */
  function setChannel(channel: LinkCryptoChannel, value: boolean) {
    if (channel === 'http') settings.value.encryptHttp = value
    persist()
  }
  return { settings, setEnabled, setStrictMode, setChannel }
}

/**
 * 把当前开关与 pin 推送到 Rust 侧（issue 09）
 *
 * HTTP 信封加密由 Rust 侧 http_proxy 裁决，而本模块状态存于 WebView
 * localStorage——经 set_link_crypto_context 命令桥接。调用时机：
 * 设置变更 / pin 刷新 / 应用启动。失败静默（老宿主无此命令时 HTTP 载荷
 * 保持明文，与默认关行为一致，不阻断 UI）。WS 通道加密已退役，不再推送。
 */
export async function syncLinkCryptoContextToNative(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_link_crypto_context', {
      enabled: settings.value.enabled,
      strictMode: settings.value.strictMode,
      encryptHttp: settings.value.encryptHttp,
      kdPublicB64: getPinnedKey(),
    })
  } catch (e) {
    logger.warn('[LinkEncryption] sync to native failed (non-fatal):', e)
  }
}

/**
 * 链路加密参与判定的通道（WS 通道已随桌面端插件端点帧加密退役，仅剩 HTTP
 * 信封加密；类型保留通道维度以兼容断言与既有调用面）
 */
export type LinkCryptoChannel = 'http'

/** 主开关 + HTTP 子开关均开、且已持有 pin → HTTP 载荷参与加密 */
export function isChannelEncryptionActive(channel: LinkCryptoChannel): boolean {
  if (!settings.value.enabled) return false
  if (channel === 'http' && !settings.value.encryptHttp) return false
  return !!getPinnedKey()
}

/** 已 pin 的桌面端身份公钥（base64）；未配对/未下发为 null */
export function getPinnedKey(): string | null {
  return localStorage.getItem(PIN_PUBLIC_KEY)
}

/** 已 pin 的指纹（SHA-256 前 16 hex） */
export function getPinnedFingerprint(): string | null {
  return localStorage.getItem(PIN_FINGERPRINT)
}

/**
 * pin 写入统一入口：公钥必存（加密协商信任锚），指纹随带（仅展示用途）
 */
function applyPin(kdPublicB64: unknown, kdFingerprint: unknown): void {
  if (typeof kdPublicB64 !== 'string' || kdPublicB64.length === 0) return
  // 公钥必须可解为 32 字节：畸形值写入会让 HTTP 侧每次 deriveHttpKeys 抛错
  // 返回 LINK_ENCRYPTION_SEAL_FAILED，pin 只能靠重新配对恢复——信任锚不应被
  // 污染，写入前校验
  let decoded: Uint8Array | null = null
  try {
    decoded = base64ToBytes(kdPublicB64)
  } catch {
    logger.error('[LinkEncryption] invalid kdPublicB64 (not base64), pin rejected')
    return
  }
  if (decoded.length !== 32) {
    logger.error(`[LinkEncryption] invalid kdPublicB64 (length ${decoded.length} != 32), pin rejected`)
    return
  }
  localStorage.setItem(PIN_PUBLIC_KEY, kdPublicB64)
  if (typeof kdFingerprint === 'string' && kdFingerprint.length > 0) {
    localStorage.setItem(PIN_FINGERPRINT, kdFingerprint)
  } else {
    // 指纹缺失（桌面端身份重生成/换机时 kdFingerprint 为 None）→ 清除旧指纹：
    // 展示「新公钥+旧指纹」会把人工核对的防中间人锚点变成 false-positive 信任
    localStorage.removeItem(PIN_FINGERPRINT)
  }
  // pin 刷新即推送 Rust 侧：事件 WS 重连协商依赖最新 pin（issue 09）
  void syncLinkCryptoContextToNative()
}

/**
 * 监听 Rust 侧认证成功时广播的 pin（配对码 / QR / reauth / 生物认证统一出口）。
 * 修复 pin 断链：前端认证走 Rust invoke，auth 响应携带的桌面端身份公钥此前
 * 无处落地，导致设置页误报“请先完成设备配对”、加密开关形同虚设。
 * 返回 unlisten，App 启动时注册一次即可。
 */
export async function initLinkCryptoPinSync(): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event')
  return listen<{ kdPublicB64?: string | null; kdFingerprint?: string | null }>(
    'ws_link_crypto_pin',
    (event) => {
      applyPin(event.payload?.kdPublicB64 ?? null, event.payload?.kdFingerprint ?? null)
    },
  )
}
