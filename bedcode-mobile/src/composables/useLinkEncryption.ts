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

const STORAGE_KEY = 'link-encryption'
const PIN_PUBLIC_KEY = 'link_kd_public_b64'
const PIN_FINGERPRINT = 'link_kd_fingerprint'

export interface LinkEncryptionSettings {
  enabled: boolean
  strictMode: boolean
}

function loadSettings(): LinkEncryptionSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { enabled: false, strictMode: false }
    const parsed = JSON.parse(raw) as Partial<LinkEncryptionSettings>
    return {
      enabled: parsed.enabled === true,
      strictMode: parsed.strictMode === true,
    }
  } catch {
    return { enabled: false, strictMode: false }
  }
}

const settings = ref<LinkEncryptionSettings>(loadSettings())

function persist() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value))
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
  return { settings, setEnabled, setStrictMode }
}

/** 主开关 + 对应通道子开关均开、且已持有 pin → 该通道参与加密 */
export function isChannelEncryptionActive(channel: 'http' | 'ws'): boolean {
  if (!settings.value.enabled) return false
  // 子开关（encryptHttp/encryptWsTerminal/encryptWsEvent）默认全开，
  // 移动端 v1 未提供粒度收窄 UI，主开关即总闸（spec §6 桌面侧才暴露子开关）
  void channel
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
 * 由 useHttpApi 统一拦截调用，调用方无需感知。
 */
export function notePinFromAuthData(data: unknown): void {
  if (!data || typeof data !== 'object') return
  const kdPublic = (data as Record<string, unknown>).kdPublicB64
  const kdFingerprint = (data as Record<string, unknown>).kdFingerprint
  if (typeof kdPublic === 'string' && kdPublic.length > 0) {
    localStorage.setItem(PIN_PUBLIC_KEY, kdPublic)
    if (typeof kdFingerprint === 'string' && kdFingerprint.length > 0) {
      localStorage.setItem(PIN_FINGERPRINT, kdFingerprint)
    }
  }
}
