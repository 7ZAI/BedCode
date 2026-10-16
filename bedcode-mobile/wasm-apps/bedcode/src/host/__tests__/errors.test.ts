/**
 * 宿主页域错误处理机制 行为契约测试
 * （票 2026-10-09：旧宿主机制在新前端的对齐 —— 错误码分类 / 命令归一 / i18n 落点）
 *
 * 被测：`src/host/errors.ts`。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-ERR-1 | classifyConnectionError | 分类顺序不可换（timeout → refused → unreachable → other），兼容中文「超时」 | 各关键词命中对应 kind |
 * | C-ERR-2 | classifyConnectionError | 非字符串/空值不炸且归 other | undefined/''/数字 → other |
 * | C-ERR-3 | connectionErrorKey | 分类 → 域内 `hub.*` toast key（表驱动，禁止各处拼文案） | 4 类各对应一个 key |
 * | C-ERR-4 | ensureCommandOk | `code` 为 0 或未给出视为成功，返回原结果（形状不变） | 不抛；auth 形状原样透传 |
 * | C-ERR-5 | ensureCommandOk | `code!==0` → 抛 `message`（服务端文案优先） | 抛 message |
 * | C-ERR-6 | ensureCommandOk | `code!==0` 且无 message → 抛 fallbackKey | 抛 fallbackKey |
 * | C-ERR-7 | 机制 × i18n 联动 | 机制引用的每个 key 必须双语在场（防漏译/写错键） | zh-CN 与 en 都能解析 |
 */
import { describe, it, expect } from 'vitest'
import {
  CONNECTION_ERROR_KEYS,
  classifyConnectionError,
  connectionErrorKey,
  ensureCommandOk,
} from '../errors'
import { messagesEn, messagesZhCN } from '../i18n'

/** 取 `hub.*` 键的值（键形如 'hub.timeoutToast'）；不存在返回 undefined */
function hubValue(locale: Record<string, unknown>, key: string): unknown {
  return key
    .split('.')
    .reduce<unknown>((node, part) => {
      if (!node || typeof node !== 'object') return undefined
      return (node as Record<string, unknown>)[part]
    }, locale)
}

describe('C-ERR-1/C-ERR-2 连接错误分类', () => {
  const cases: Array<{ input: unknown; expected: string }> = [
    { input: 'connect timeout after 5s', expected: 'timeout' },
    { input: '连接超时', expected: 'timeout' },
    { input: 'connection refused', expected: 'refused' },
    { input: 'handshake rejected', expected: 'refused' },
    { input: 'host unreachable', expected: 'unreachable' },
    { input: 'network error', expected: 'unreachable' },
    { input: 'boom', expected: 'other' },
    // 顺序契约：同时含 timeout 与 refused 时按 timeout 归类（与旧 DevicesView if/else 一致）
    { input: 'timeout then refused', expected: 'timeout' },
    // 边界：空串 / 非字符串 / undefined 不得抛错
    { input: '', expected: 'other' },
    { input: 0, expected: 'other' },
    { input: undefined, expected: 'other' },
  ]

  for (const c of cases) {
    it(`should_classifyAs_${c.expected}_when_input=${JSON.stringify(c.input)}`, () => {
      expect(classifyConnectionError(c.input)).toBe(c.expected)
    })
  }
})

describe('C-ERR-3 分类 → 域内 i18n key', () => {
  it('should_mapEachKindToHubKey_when_classified', () => {
    expect(connectionErrorKey('timeout')).toBe('hub.timeoutToast')
    expect(connectionErrorKey('refused')).toBe('hub.refusedToast')
    expect(connectionErrorKey('unreachable')).toBe('hub.unreachableToast')
    expect(connectionErrorKey('unrecognized failure')).toBe('hub.connectFailedToast')
  })

  it('should_coverEveryKindInTable_when_kindEnumerated', () => {
    // 表驱动契约：新增/改名分类时，key 表必须同步（否则 toast 会退化成 key 文本）
    expect(Object.keys(CONNECTION_ERROR_KEYS).sort()).toEqual([
      'other',
      'refused',
      'timeout',
      'unreachable',
    ])
  })
})

describe('C-ERR-4 命令成功路径', () => {
  it('should_notThrowAndPassThroughShape_when_codeIsZeroOrAbsent', () => {
    expect(ensureCommandOk({ code: 0, message: 'ok', data: { sessionId: 's1' } }, 'hub.startFailed')).toEqual({
      code: 0,
      message: 'ok',
      data: { sessionId: 's1' },
    })
    // auth 类命令无 code：形状原样透传（调用方读 accepted）
    expect(ensureCommandOk({ accepted: true }, 'hub.pairingFailed')).toEqual({ accepted: true })
    // 无返回（前端 handler）按成功处理
    expect(ensureCommandOk(undefined, 'hub.startFailed')).toBeUndefined()
    expect(ensureCommandOk(null, 'hub.startFailed')).toBeUndefined()
  })
})

describe('C-ERR-5/C-ERR-6 命令失败路径', () => {
  it('should_throwServerMessage_when_codeNonZeroAndMessageGiven', () => {
    expect(() => ensureCommandOk({ code: 1001, message: 'invalid token' }, 'hub.startFailed')).toThrow(
      'invalid token',
    )
  })

  it('should_throwFallbackKey_when_codeNonZeroWithoutMessage', () => {
    expect(() => ensureCommandOk({ code: -1 }, 'hub.stopFailed')).toThrow('hub.stopFailed')
    expect(() => ensureCommandOk({ code: -1, message: '' }, 'hub.removeFailed')).toThrow(
      'hub.removeFailed',
    )
  })
})

describe('C-ERR-7 机制引用的 key 双语在场', () => {
  const MECHANISM_KEYS = [
    ...Object.values(CONNECTION_ERROR_KEYS),
    // useHostPage / errors 机制引用的错误槽位与兜底 key
    'hub.connectFailed',
    'hub.pairingFailed',
    'hub.loadFailed',
    'hub.startFailed',
    'hub.stopFailed',
    'hub.removeFailed',
    'hub.manualInvalid',
    'hub.historyClearFailed',
    'hub.qrFailed',
    // 连接页流程（DevicesSection：状态小字 / 配对与扫码失败文案）
    'hub.connectionStatus.connecting',
    'hub.connectionStatus.connectingPlain',
    'hub.connectionStatus.connected',
    'hub.connectionStatus.pairing',
    'hub.connectionStatus.paired',
    'hub.connectionStatus.disconnected',
    'hub.pairingTimeout',
    'hub.userCancelled',
    'hub.codeVerifyFailed',
    'hub.scanQrExpired',
    'hub.scanTimeout',
    'hub.biometricNotBoundOnDesktop',
    // 配对码 / 生物认证弹窗
    'hub.pairingCodeTitle',
    'hub.pairingCodeHint',
    'hub.pairingClear',
    'hub.pairingConfirm',
    'hub.pairingVerifying',
    'hub.pairingRequesting',
    'hub.switchToBiometric',
    'hub.switchToPairing',
    'hub.biometricNotBound',
    'hub.authBiometric',
    'hub.authBiometricDesc',
    'hub.biometricVerify',
    'hub.biometricAuthenticating',
    'hub.biometricFailed',
    // 扫码面板（ScanPanel）
    'hub.scanHint',
    'hub.scanFailedTitle',
    'hub.scanBack',
    'hub.scanRescan',
    'hub.scanTorch',
    'hub.scanAlbum',
    'hub.scanCameraFailed',
    'hub.scanCameraPermissionHint',
    'hub.scanTorchUnsupported',
    'hub.scanAlbumNoQr',
    'hub.scanInvalid',
    'hub.scanInvalidData',
    // mDNS 发现面板
    'hub.discoverScanning',
    'hub.discoverNoDevices',
    'hub.discoverNoDevicesHint',
    'hub.discoverDeviceFound',
    'hub.discoverConnectToDevice',
    'hub.discoverConnected',
    'hub.discoverStopScan',
    'hub.discoverRestartScan',
    // 连接历史（确认 + 相对时间）
    'hub.historyClearConfirm',
    'hub.timeJustNow',
    'hub.timeMinutesAgo',
    'hub.timeHoursAgo',
    'hub.timeDaysAgo',
    // 会话页（区块 / 二次确认 / 准备中）
    'hub.sessionTitle',
    'hub.refreshConfig',
    'hub.maxReached',
    'hub.confirmStop',
    'hub.confirmStopMsg',
    'hub.confirmDelete',
    'hub.confirmDeleteMsg',
    'hub.startFailedNoId',
    'hub.preparingTerminal',
    'hub.stopSession',
    'hub.deleteSession',
    'hub.configProjectDir',
    'hub.configCommand',
    'hub.noSessionsHint',
    // 自持弹窗按钮（不依宿主 common.* 命名空间）
    'hub.actionCancel',
    'hub.actionConfirm',
    'hub.actionClose',
  ]

  it('should_resolveEveryMechanismKeyInBothLocales_when_localesLoaded', () => {
    for (const key of MECHANISM_KEYS) {
      const zh = hubValue(messagesZhCN as unknown as Record<string, unknown>, key)
      const en = hubValue(messagesEn as unknown as Record<string, unknown>, key)
      expect(typeof zh, `zh-CN 缺 ${key}`).toBe('string')
      expect(typeof en, `en 缺 ${key}`).toBe('string')
      expect(String(zh).length, `zh-CN ${key} 文案为空`).toBeGreaterThan(0)
      expect(String(en).length, `en ${key} 文案为空`).toBeGreaterThan(0)
    }
  })

  it('should_keepParamPlaceholderIdenticalAcrossLocales_when_keyTakesParams', () => {
    // 带插值的 key 双语文案必须含同一占位符（漏写 = 用户看到未替换花括号）
    const paramKeys: [string, string][] = [
      ['hub.connectFailedToast', '{error}'],
      ['hub.startFailed', '{error}'],
      ['hub.connectionStatus.connecting', '{name}'],
      ['hub.discoverDeviceFound', '{count}'],
      ['hub.timeMinutesAgo', '{count}'],
      ['hub.timeHoursAgo', '{count}'],
      ['hub.timeDaysAgo', '{count}'],
      ['hub.confirmStopMsg', '{name}'],
      ['hub.confirmDeleteMsg', '{name}'],
      ['hub.maxReached', '{max}'],
    ]
    for (const [key, placeholder] of paramKeys) {
      const zh = String(hubValue(messagesZhCN as unknown as Record<string, unknown>, key) ?? '')
      const en = String(hubValue(messagesEn as unknown as Record<string, unknown>, key) ?? '')
      expect(zh, `zh-CN ${key} 缺占位符`).toContain(placeholder)
      expect(en, `en ${key} 缺占位符`).toContain(placeholder)
    }
  })
})
