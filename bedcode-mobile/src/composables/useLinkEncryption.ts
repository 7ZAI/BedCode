/**
 * 链路加密配置与 pin 状态（issue 05/06）
 *
 * 配置域镜像桌面端 trafficEncryption 的移动端子集（spec §6）：
 * - enabled：主开关（默认 false，opt-in；无 pin 时开启会被 UI 引导先配对）
 * - strictMode：预期加密而遭降级时断连报错（默认 false → 明文续跑 + 提示）
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
  /** HTTP REST 载荷加密（issue 08：粒度收窄，默认开） */
  encryptHttp: boolean
  /** WS 终端通道帧加密（默认开） */
  encryptWsTerminal: boolean
  /** WS 事件通道帧加密（默认开） */
  encryptWsEvent: boolean
}

/** 默认值：功能整体关（opt-in），子开关全开——用户只需打开主开关即获全通道覆盖 */
const DEFAULTS: LinkEncryptionSettings = {
  enabled: false,
  strictMode: false,
  encryptHttp: true,
  encryptWsTerminal: true,
  encryptWsEvent: true,
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
      encryptWsTerminal: parsed.encryptWsTerminal !== false,
      encryptWsEvent: parsed.encryptWsEvent !== false,
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
  /** 通道子开关统一入口（issue 08：设置页粒度收窄；通道名与判定函数一致用 kebab-case） */
  function setChannel(channel: LinkCryptoChannel, value: boolean) {
    const key = (
      { http: 'encryptHttp', 'ws-terminal': 'encryptWsTerminal', 'ws-event': 'encryptWsEvent' } as const
    )[channel]
    settings.value[key] = value
    persist()
  }
  return { settings, setEnabled, setStrictMode, setChannel }
}

/**
 * 把当前开关与 pin 推送到 Rust 侧（issue 09）
 *
 * 常驻事件 WS 建连在 Rust 侧（connection/event_ws.rs），而本模块状态存于
 * WebView localStorage——经 set_link_crypto_context 命令桥接。调用时机：
 * 设置变更 / pin 刷新 / 应用启动。失败静默（老宿主无此命令时事件 WS
 * 保持明文，与默认关行为一致，不阻断 UI）。
 */
export async function syncLinkCryptoContextToNative(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_link_crypto_context', {
      enabled: settings.value.enabled,
      strictMode: settings.value.strictMode,
      encryptWsEvent: settings.value.encryptWsEvent,
      kdPublicB64: getPinnedKey(),
    })
  } catch (e) {
    logger.warn('[LinkEncryption] sync to native failed (non-fatal):', e)
  }
}

/** 链路加密参与判定的通道（issue 08：与桌面端三子开关一一对应） */
export type LinkCryptoChannel = 'http' | 'ws-terminal' | 'ws-event'

/** 主开关 + 对应通道子开关均开、且已持有 pin → 该通道参与加密 */
export function isChannelEncryptionActive(channel: LinkCryptoChannel): boolean {
  if (!settings.value.enabled) return false
  if (channel === 'http' && !settings.value.encryptHttp) return false
  if (channel === 'ws-terminal' && !settings.value.encryptWsTerminal) return false
  if (channel === 'ws-event' && !settings.value.encryptWsEvent) return false
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
 * 从 auth 响应数据中提取并刷新 pin（qr-connect / verify / reauth 均携带）。
 * 由 useHttpApi 统一拦截调用，调用方无需感知；
 * 配对/重认证主路径为 Rust invoke（wsVerifyPairingCode 等），经事件
 * `ws_link_crypto_pin`（initLinkCryptoPinSync）落地，此处保留供 HTTP 通道接入。
 */
export function notePinFromAuthData(data: unknown): void {
  if (!data || typeof data !== 'object') return
  applyPin(
    (data as Record<string, unknown>).kdPublicB64,
    (data as Record<string, unknown>).kdFingerprint,
  )
}

/** pin 写入统一入口：公钥必存（加密协商信任锚），指纹随带（仅展示用途） */
function applyPin(kdPublicB64: unknown, kdFingerprint: unknown): void {
  if (typeof kdPublicB64 !== 'string' || kdPublicB64.length === 0) return
  // 公钥必须可解为 32 字节：畸形值写入会让 HTTP 侧每次 deriveHttpKeys 抛错
  // 返回 LINK_ENCRYPTION_SEAL_FAILED、事件 WS 侧解码失败断连，pin 只能靠
  // 重新配对恢复——信任锚不应被污染，写入前校验
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
