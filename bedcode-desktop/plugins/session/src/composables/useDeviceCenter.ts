/**
 * 设备与配对域取数与编排（票 14）
 *
 * 取数红线（spec D2）：只经 PluginContext（commands / storage / i18n / events）与
 * 插件命令通道（宿主 `plugin_invoke` → 本插件 WASM `command.invoke`）——
 * **禁止**直调宿主领域命令（`generate_pairing_code` / `list_paired_devices` 等，
 * 那层门面是宿主 UI 的兼容接缝，插件工程契约测试 C4 强校验）。
 *
 * 职责边界（spec D3）：配对编排（生成 / 展示 / 失效清除）、设备列表组织
 * （在线判定 + 排序）、网络信息（端口 + 本地 IPv4 + 用户选择的 host）全在本模块；
 * 语义真源仍在内核（配对码状态机在插件、配对记录在内核 `pairings` 表经原语读写）。
 *
 * 宿主事件（device-connected / device-disconnected / pairing-code-generated /
 * qr-token-consumed）经 `context.events.on` 订阅——该通道由宿主 `pluginEvents`
 * 同时桥接 Tauri `listen()`，是本插件感知后端事件推进的唯一路径。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

/** 配对码信息（插件命令面回执，与宿主 `PairingCodeInfo` 逐字同形） */
export interface PairingCodeInfo {
  code: string
  created_at: string
  expires_in: number
}

/** 已配对设备（插件命令面 `session.devices.paired-list` 元素，camelCase） */
export interface PairedDeviceInfo {
  id: string
  deviceName: string
  deviceFingerprint: string
  address?: string | null
  pairedAt: string
  lastSeen?: string | null
  connectCount: number
}

/** 网络信息（端口 + 本机可访问 IPv4 列表） */
export interface LocalNetworkInfo {
  port: number
  addresses: string[]
}

/** QR 连接信息（扫码载荷三要素 + 剩余秒） */
export interface QrConnectionInfo {
  host: string
  port: number
  token: string
  remainingSecs: number
}

/** 插件存储键：用户选择的对外 IP（宿主 `AppConfig.network.qr_host` 的插件侧承接） */
const QR_HOST_STORAGE_KEY = 'pairing.qrHost'

/** 宿主事件载荷（仅取 fingerprint） */
interface DeviceEventPayload {
  fingerprint?: string
}

/** 后端推送的配对码事件载荷 */
interface PairingCodeEventPayload {
  code: string
  expires_in: number
  device_name?: string
}

export function useDeviceCenter(context: PluginContext) {
  // ==================== 配对码 ====================
  const pairingCode = ref<PairingCodeInfo | null>(null)
  const remainingSeconds = ref(0)
  const isGenerating = ref(false)

  // ==================== QR ====================
  const qrData = ref<QrConnectionInfo | null>(null)
  const qrRemainingSeconds = ref(0)
  const isQrLoading = ref(false)
  const hasQr = computed(() => qrData.value !== null && qrRemainingSeconds.value > 0)

  // ==================== 设备列表 ====================
  const devices = ref<PairedDeviceInfo[]>([])
  const isLoading = ref(false)
  /** 实时在线设备指纹（来自 device-connected / device-disconnected 事件） */
  const onlineFingerprints = ref<Set<string>>(new Set())

  // ==================== 网络信息 ====================
  const port = ref(0)
  const addresses = ref<string[]>([])
  const selectedHost = ref<string | null>(null)
  const ipOptions = computed(() => addresses.value.map((ip) => ({ value: ip, label: ip })))

  const onlineDevices = computed<PairedDeviceInfo[]>(() =>
    devices.value.filter((d) => onlineFingerprints.value.has(d.deviceFingerprint)),
  )
  const offlineDevices = computed<PairedDeviceInfo[]>(() =>
    devices.value.filter((d) => !onlineFingerprints.value.has(d.deviceFingerprint)),
  )

  let codeTimer: ReturnType<typeof setInterval> | null = null
  let qrTimer: ReturnType<typeof setInterval> | null = null
  const disposables: { dispose(): void }[] = []

  // ==================== 网络信息 ====================

  /**
   * 读取网络信息：端口取内核运行端口，地址列表取宿主网卡枚举；
   * 用户选择的 host 落插件存储（缺省自动选第一个可用 IPv4，与宿主原行为一致）
   */
  async function loadNetwork(): Promise<void> {
    const info = (await context.commands.execute('session.network.info', {})) as
      | LocalNetworkInfo
      | undefined
    port.value = info?.port ?? 0
    addresses.value = Array.isArray(info?.addresses) ? info.addresses : []

    const stored = await context.storage.get<string>(QR_HOST_STORAGE_KEY)
    if (stored) {
      selectedHost.value = stored
    } else if (addresses.value.length > 0) {
      selectedHost.value = addresses.value[0]
      await context.storage.set(QR_HOST_STORAGE_KEY, selectedHost.value)
    }
  }

  /** 选择对外 IP（写入插件存储；QR 载荷随之变化） */
  async function selectHost(ip: string): Promise<void> {
    if (!ip) return
    selectedHost.value = ip
    await context.storage.set(QR_HOST_STORAGE_KEY, ip)
  }

  // ==================== 配对码 ====================

  function stopCodeCountdown(): void {
    if (codeTimer !== null) {
      clearInterval(codeTimer)
      codeTimer = null
    }
  }

  /** 配对码倒计时；到期清除后端状态（宿主原语义：过期即清） */
  function startCodeCountdown(): void {
    stopCodeCountdown()
    codeTimer = setInterval(() => {
      if (remainingSeconds.value > 0) {
        remainingSeconds.value--
        return
      }
      stopCodeCountdown()
      void clearCode()
    }, 1000)
  }

  /** 生成配对码（TTL 由插件后端按认证域设置读取） */
  async function generateCode(): Promise<PairingCodeInfo | null> {
    isGenerating.value = true
    try {
      const info = (await context.commands.execute('session.pairing.generate', {})) as
        | PairingCodeInfo
        | null
      pairingCode.value = info ?? null
      if (info) {
        remainingSeconds.value = info.expires_in
        startCodeCountdown()
      }
      return info ?? null
    } finally {
      isGenerating.value = false
    }
  }

  /** 恢复当前有效配对码（进入页面时调用）→ 是否有活跃码 */
  async function restoreCode(): Promise<boolean> {
    const info = (await context.commands.execute('session.pairing.status', {})) as
      | PairingCodeInfo
      | null
    if (info && info.code) {
      pairingCode.value = info
      remainingSeconds.value = info.expires_in
      startCodeCountdown()
      return true
    }
    return false
  }

  /** 清除配对码（用户取消 / 过期 / 设备已接入） */
  async function clearCode(): Promise<void> {
    stopCodeCountdown()
    pairingCode.value = null
    remainingSeconds.value = 0
    await context.commands.execute('session.pairing.clear', {})
  }

  // ==================== QR ====================

  function stopQrCountdown(): void {
    if (qrTimer !== null) {
      clearInterval(qrTimer)
      qrTimer = null
    }
  }

  function startQrCountdown(): void {
    stopQrCountdown()
    qrTimer = setInterval(() => {
      if (qrRemainingSeconds.value > 0) {
        qrRemainingSeconds.value--
        return
      }
      stopQrCountdown()
      qrData.value = null
    }, 1000)
  }

  /** 生成二维码连接信息（token 生成 + host/port 组装，端口取内核运行端口） */
  async function generateQr(host?: string): Promise<void> {
    isQrLoading.value = true
    try {
      const info = (await context.commands.execute('session.qr.generate', {
        host: host ?? selectedHost.value ?? undefined,
      })) as QrConnectionInfo | null
      if (info) {
        qrData.value = info
        qrRemainingSeconds.value = info.remainingSecs
        startQrCountdown()
      } else {
        qrData.value = null
      }
    } finally {
      isQrLoading.value = false
    }
  }

  /** 恢复现有二维码（不重新生成）→ 是否恢复成功 */
  async function restoreQr(host?: string): Promise<boolean> {
    const info = (await context.commands.execute('session.qr.info', {
      host: host ?? selectedHost.value ?? undefined,
    })) as QrConnectionInfo | null
    if (info && info.token) {
      qrData.value = info
      qrRemainingSeconds.value = info.remainingSecs
      startQrCountdown()
      return true
    }
    return false
  }

  /** 清除二维码 */
  async function clearQr(): Promise<void> {
    stopQrCountdown()
    qrData.value = null
    qrRemainingSeconds.value = 0
    await context.commands.execute('session.qr.clear', {})
  }

  /** 扫码载荷（与宿主逐字段一致：host / port / token） */
  function qrPayload(): string | null {
    const data = qrData.value
    if (!data) return null
    return JSON.stringify({ host: data.host, port: data.port, token: data.token })
  }

  // ==================== 设备列表 ====================

  /** 已配对设备列表（活跃记录；排序与展示组织在本插件侧） */
  async function loadDevices(): Promise<void> {
    isLoading.value = true
    try {
      const list = (await context.commands.execute('session.devices.paired-list', {})) as
        | PairedDeviceInfo[]
        | undefined
      devices.value = Array.isArray(list) ? list : []
    } finally {
      isLoading.value = false
    }
  }

  /** 移除设备（撤销信任：真源内核 `pairings` 表经插件后端写，行为与今天一致） */
  async function removeDevice(deviceId: string): Promise<void> {
    await context.commands.execute('session.devices.revoke', { id: deviceId })
    await loadDevices()
  }

  // ==================== 宿主事件订阅 ====================

  /**
   * 订阅后端事件（一次性注册，deactivate 时统一释放）：
   * - device-connected → 记在线指纹 + 刷新配对列表 + 清掉已使用的配对码
   * - device-disconnected → 摘掉在线指纹
   * - pairing-code-generated → 移动端请求配对时后端生成的码，直接展示 + 提示
   * - qr-token-consumed → 自动重新生成二维码（连接成功提示由宿主全局通知负责）
   */
  function subscribeHostEvents(onPairingRequest?: (code: string) => void): void {
    disposables.push(
      context.events.on('device-connected', (payload: DeviceEventPayload) => {
        const fp = payload?.fingerprint
        if (fp) {
          onlineFingerprints.value = new Set([...onlineFingerprints.value, fp])
        }
        void loadDevices()
        if (pairingCode.value) {
          void clearCode()
        }
      }),
      context.events.on('device-disconnected', (payload: DeviceEventPayload) => {
        const fp = payload?.fingerprint
        if (!fp) return
        const next = new Set(onlineFingerprints.value)
        next.delete(fp)
        onlineFingerprints.value = next
      }),
      context.events.on('pairing-code-generated', (payload: PairingCodeEventPayload) => {
        if (!payload?.code) return
        pairingCode.value = {
          code: payload.code,
          expires_in: payload.expires_in,
          created_at: new Date().toISOString(),
        }
        remainingSeconds.value = payload.expires_in
        startCodeCountdown()
        onPairingRequest?.(payload.code)
      }),
      context.events.on('qr-token-consumed', () => {
        void generateQr()
      }),
    )
  }

  /** 释放事件订阅与倒计时（deactivate 时调用，避免插件停用后仍持有定时器） */
  function dispose(): void {
    stopCodeCountdown()
    stopQrCountdown()
    for (const d of disposables.splice(0)) {
      d.dispose()
    }
  }

  return {
    // 配对码
    pairingCode: pairingCode as Ref<PairingCodeInfo | null>,
    remainingSeconds,
    isGenerating,
    loadNetwork,
    selectHost,
    generateCode,
    restoreCode,
    clearCode,
    // QR
    qrData: qrData as Ref<QrConnectionInfo | null>,
    qrRemainingSeconds,
    isQrLoading,
    hasQr: hasQr as ComputedRef<boolean>,
    generateQr,
    restoreQr,
    clearQr,
    qrPayload,
    // 设备
    devices: devices as Ref<PairedDeviceInfo[]>,
    isLoading,
    onlineDevices: onlineDevices as ComputedRef<PairedDeviceInfo[]>,
    offlineDevices: offlineDevices as ComputedRef<PairedDeviceInfo[]>,
    loadDevices,
    removeDevice,
    // 网络
    port,
    addresses,
    selectedHost: selectedHost as Ref<string | null>,
    ipOptions: ipOptions as ComputedRef<{ value: string; label: string }[]>,
    // 生命周期
    subscribeHostEvents,
    dispose,
  }
}

export type DeviceCenter = ReturnType<typeof useDeviceCenter>
