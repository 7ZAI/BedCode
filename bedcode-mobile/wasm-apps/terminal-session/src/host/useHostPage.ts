/**
 * 宿主页连接 / 会话 / 配对编排 composable（票 2026-10-09：旧宿主主流程下沉）
 *
 * 数据面全部走宿主投影与插件命令（零宿主 Tauri 直连）：
 * - 连接引擎面：mobileApi（connectDevice / connectionStatus / onConnectionEvent /
 *   connectionHistory / mDNS 原始事实 / 生物凭证状态）
 * - 配对 / 会话：插件命令面（terminal-session.request-pairing / verify-pairing-code /
 *   list-sessions / start-session / stop-session / remove-session）
 * - 配对事件：context.events（ws_pairing_request / ws_paired / ws_auth_failed 白名单）
 *
 * 归属（§5.1）：派生状态（发现列表归约 / 会话展示形状）在本 composable 内自持，
 * 宿主只投引擎事实；凭据零过境（C4），本域不接触任何 token。
 * 类型注意：mobileApi 的 Ref 来自宿主共享运行时（SDK 声明），直接透传其类型，
 * 不做跨 vue 拷贝的本地 Ref 强转。
 *
 * 机制对齐（旧前端机制 → 新界面，见 ./errors.ts 与 host/i18n.ts）：
 * - 国际化：文案一律 `context.i18n.t`（域内键 `hub.*`）；错误槽位存「i18n key 或原始文案」，
 *   渲染一律 `t(...)`（旧 DevicesView 同口径）
 * - 错误码：`./errors.ts` —— 连接错误分类表（timeout/refused/unreachable/other）+
 *   `ensureCommandOk`（`{code,message}`，code!==0 抛 message||fallbackKey）
 * - 日志：`context.logger`，带 `[host]` 上下文前缀；catch 内必须落日志（禁止静默 catch）
 */

import { computed, inject, ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'
import { connectionErrorKey, ensureCommandOk, type CommandResult } from './errors'

/** 插件命令 id 前缀（manifest contributes.commands） */
const CMD = 'terminal-session.'

/** 错误对象 → 日志文本（旧前端 `logger.error('[域] 动作失败:', e)` 的文本化口径） */
function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

/** 配对流程状态机（引擎事实 + 本域推进） */
export type PairingStage = 'idle' | 'requested' | 'verifying'

/** 建连目标（最小形状，与 SDK MobileDeviceTarget 一致） */
export interface HostDeviceTarget {
  address: string
  port: number
  name?: string
}

export function useHostPage(provided?: PluginContext) {
  const context = provided ?? inject<PluginContext>('pluginContext')
  if (!context) throw new Error('[host] pluginContext not provided')
  // 闭包捕获收窄后的类型（异步闭包内 TS 不保留窄化）
  const ctx = context
  const api = getMobileApi()
  /** 统一日志入口（与宿主 frontendLogger 同源机制：单参、带 [域] 上下文前缀） */
  const logger = ctx.logger

  // ── 连接引擎态（SDK 类型透传） ──
  const connectionStatus = api.connectionStatus
  const currentDevice = api.currentDevice
  const isConnecting = api.isConnecting
  const isAuthenticated = computed(() => api.isConnected.value)
  const connectionError = ref('')
  const reconnectBanner = ref('')
  const connectingName = ref('')
  const deviceError = ref('')

  /** 连接目标设备：探测 + 建 WS 事件通道（引擎动作）。失败按 errors.ts 分类表落 i18n key */
  async function connectDevice(device: HostDeviceTarget): Promise<void> {
    deviceError.value = ''
    connectingName.value = device.name || device.address
    try {
      await api.connectDevice(device)
    } catch (e) {
      deviceError.value = connectionErrorKey(errText(e))
      logger.error(
        `[host] connect failed: address=${device.address} port=${device.port} ${errText(e)}`,
      )
      throw e
    } finally {
      connectingName.value = ''
    }
  }

  function disconnect(): Promise<void> {
    return api.disconnect()
  }

  // ── mDNS 原始事实（SDK 类型透传） ──
  const mdnsServices = api.mdnsServices
  const mdnsScanning = api.mdnsScanning

  async function startScan(keepResults = false): Promise<void> {
    try {
      await api.mdnsStart({ keepResults })
    } catch (e) {
      // 引擎启动失败：UI 保持当前扫描态（旧 DevicesView 同口径，不弹错误），但必须可观测
      logger.error(`[host] mdns start failed: ${errText(e)}`)
    }
  }
  async function stopScan(): Promise<void> {
    try {
      await api.mdnsStop()
    } catch (e) {
      logger.warn(`[host] mdns stop failed: ${errText(e)}`)
    }
  }

  // ── 连接历史（引擎事实） ──
  const connectionHistory = api.connectionHistory
  async function loadHistory(force = false): Promise<void> {
    await api.loadConnectionHistory(force)
  }
  async function clearHistory(): Promise<void> {
    await api.clearConnectionHistory()
  }

  // ── 配对编排（插件命令面；事件驱动进度） ──
  const pairingStage = ref<PairingStage>('idle')
  const pairingError = ref('')
  const pairingCode = ref('')

  async function requestPairing(): Promise<void> {
    pairingError.value = ''
    await ctx.commands.execute(`${CMD}request-pairing`)
    pairingStage.value = 'requested'
  }

  async function verifyPairingCode(code: string): Promise<boolean> {
    pairingError.value = ''
    pairingStage.value = 'verifying'
    try {
      const res = ensureCommandOk(
        (await ctx.commands.execute(`${CMD}verify-pairing-code`, { code })) as {
          accepted?: boolean
        },
        'hub.pairingFailed',
      )
      if (!res?.accepted) {
        // 业务拒绝：指令面受理（{accepted:false}）而不是抛错 —— 记 warn 并落可展示文案
        pairingError.value = 'hub.pairingFailed'
        pairingStage.value = 'idle'
        logger.warn('[host] pairing code rejected by host')
        return false
      }
      pairingStage.value = 'idle'
      pairingCode.value = ''
      return true
    } finally {
      if (pairingStage.value === 'verifying') pairingStage.value = 'idle'
    }
  }

  /** 扫码认证（插件命令面；token 来自桌面端二维码，凭据仍留宿主） */
  async function authenticateWithQr(token: string): Promise<boolean> {
    const res = ensureCommandOk(
      (await ctx.commands.execute(`${CMD}authenticate-with-qr`, { token })) as {
        accepted?: boolean
      },
      'hub.qrFailed',
    )
    if (!res?.accepted) logger.warn('[host] qr auth rejected by host')
    return !!res?.accepted
  }

  /** 生物登录（插件命令面；凭据仍留宿主） */
  async function authenticateWithBiometric(): Promise<boolean> {
    const res = ensureCommandOk(
      (await ctx.commands.execute(`${CMD}authenticate-with-biometric`)) as {
        accepted?: boolean
      },
      'hub.pairingFailed',
    )
    if (!res?.accepted) logger.warn('[host] biometric auth rejected by host')
    return !!res?.accepted
  }

  /** 生物凭证状态（引擎窄读投影；材料零过境） */
  const biometric = ref<{ deviceSupported: boolean; deviceReason: number; hasKey: boolean } | null>(
    null,
  )
  async function refreshBiometric(): Promise<void> {
    try {
      biometric.value = await api.getBiometricKeyStatus()
    } catch (e) {
      // 状态读取失败：置空即隐藏生物入口（不阻断主流程），但失败必须可观测
      biometric.value = null
      logger.warn(`[host] biometric status probe failed: ${errText(e)}`)
    }
  }
  async function bindBiometric(): Promise<boolean> {
    const ok = await api.bindBiometricCredential()
    if (!ok) logger.warn('[host] bind biometric credential rejected')
    return ok
  }
  async function unbindBiometric(): Promise<boolean> {
    const ok = await api.unbindBiometricCredential()
    if (!ok) logger.warn('[host] unbind biometric credential rejected')
    return ok
  }

  // ── 会话 / 会话配置（宿主连接域投影 + 插件命令面） ──
  const sessions = api.activeSessions
  const sessionConfigs = api.sessionConfigs

  async function refreshSessions(): Promise<void> {
    await api.loadActiveSessions()
    await api.loadSessionConfigs()
  }

  /** 从会话配置启动会话；成功返回新会话 id，失败抛 `message || 'hub.startFailed'` */
  async function startSession(configId: string): Promise<string | null> {
    const res = ensureCommandOk(
      (await ctx.commands.execute(`${CMD}start-session`, { configId })) as CommandResult<{
        sessionId?: string
      }>,
      'hub.startFailed',
    )
    return res?.data?.sessionId ?? null
  }

  async function stopSession(sessionId: string): Promise<void> {
    ensureCommandOk(
      (await ctx.commands.execute(`${CMD}stop-session`, { sessionId })) as CommandResult,
      'hub.stopFailed',
    )
  }

  async function removeSession(sessionId: string): Promise<void> {
    ensureCommandOk(
      (await ctx.commands.execute(`${CMD}remove-session`, { sessionId })) as CommandResult,
      'hub.removeFailed',
    )
  }

  // ── 事件订阅（白名单） ──
  function subscribe(): Disposable[] {
    const disposables: Disposable[] = []

    // 配对进度事件（host-events 逐字透传，载荷形状与退役前一致）
    disposables.push(
      ctx.events.on('ws_pairing_request', () => {
        pairingStage.value = 'requested'
      }),
    )
    disposables.push(
      ctx.events.on('ws_pairing_verified', () => {
        pairingStage.value = 'idle'
        pairingCode.value = ''
      }),
    )
    disposables.push(
      ctx.events.on('ws_paired', () => {
        pairingStage.value = 'idle'
        pairingCode.value = ''
        pairingError.value = ''
      }),
    )
    disposables.push(
      ctx.events.on('ws_auth_failed', (payload: any) => {
        pairingError.value = payload?.reason || payload?.message || 'hub.pairingFailed'
      }),
    )

    // 连接生命周期事件（引擎事实）：横幅 + 认证拒绝提示
    disposables.push(
      api.onConnectionEvent((event) => {
        switch (event.type) {
          case 'reconnecting':
            reconnectBanner.value = String(event.retry ?? 0)
            break
          case 'reconnected':
          case 'unexpected_disconnect':
          case 'reconnect_failed':
            reconnectBanner.value = ''
            break
          case 'reauth_rejected':
            pairingError.value = 'hub.pairingFailed'
            break
          default:
            break
        }
      }),
    )

    return disposables
  }

  return {
    /** 插件上下文（命令 / 事件 / 组件内透传） */
    context,
    api,
    /** 统一日志入口（组件 catch 内必须用：禁止静默 catch） */
    logger,
    // state
    connectionStatus,
    isAuthenticated,
    currentDevice,
    isConnecting,
    connectionError,
    reconnectBanner,
    connectingName,
    // 错误槽位：i18n key 或原始文案，渲染一律 `t(...)`（旧前端既定口径）
    deviceError,
    mdnsServices,
    mdnsScanning,
    connectionHistory,
    sessions,
    sessionConfigs,
    pairingStage,
    pairingError,
    pairingCode,
    biometric,
    // actions
    connectDevice,
    disconnect,
    startScan,
    stopScan,
    loadHistory,
    clearHistory,
    requestPairing,
    verifyPairingCode,
    authenticateWithQr,
    authenticateWithBiometric,
    refreshBiometric,
    bindBiometric,
    unbindBiometric,
    refreshSessions,
    startSession,
    stopSession,
    removeSession,
    subscribe,
    t: context.i18n.t.bind(context.i18n),
    toast: context.dialogs,
  }
}