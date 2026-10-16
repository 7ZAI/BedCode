/**
 * 宿主页纯函数测试（票 2026-10-09）
 * 契约：地址解析正反例 / 状态文案 key 映射 / 会话活跃判定边界 / 未知状态兜底。
 */
import { describe, expect, it } from 'vitest'
import { formatLastConnected, isSessionActive, parseAddress, parseAddressWithDefault, sessionStatusKey, statusKey } from '../utils'

describe('parseAddress 地址:端口 解析', () => {
  it('解析 IPv4:端口', () => {
    expect(parseAddress('192.168.1.5:8765')).toEqual({ address: '192.168.1.5', port: 8765 })
  })
  it('解析主机名:端口', () => {
    expect(parseAddress('my-desktop:9000')).toEqual({ address: 'my-desktop', port: 9000 })
  })
  it('容忍首尾空白', () => {
    expect(parseAddress('  10.0.0.2:1234  ')).toEqual({ address: '10.0.0.2', port: 1234 })
  })
  it('缺端口 / 空串 / 非数字端口 → null（反例）', () => {
    expect(parseAddress('192.168.1.5')).toBeNull()
    expect(parseAddress('')).toBeNull()
    expect(parseAddress('   ')).toBeNull()
    expect(parseAddress('host:abc')).toBeNull()
    expect(parseAddress(':8765')).toBeNull()
  })
})

describe('statusKey 连接状态 → i18n key', () => {
  it('六种引擎状态映射齐备', () => {
    for (const s of ['disconnected', 'connecting', 'connected', 'pairing', 'paired', 'error']) {
      expect(statusKey(s)).toBe(`hub.status.${s}`)
    }
  })
  it('未知状态兜底 hub.status.unknown', () => {
    expect(statusKey('bogus')).toBe('hub.status.unknown')
  })
})

describe('parseAddressWithDefault 手动连接地址解析（旧 DevicesView 口径）', () => {
  it('未写端口 → 回落默认端口', () => {
    expect(parseAddressWithDefault('10.0.0.5', 9000)).toEqual({ address: '10.0.0.5', port: 9000 })
  })
  it('显式端口优先于默认端口', () => {
    expect(parseAddressWithDefault('10.0.0.5:8765', 9000)).toEqual({ address: '10.0.0.5', port: 8765 })
  })
  it('尾部冒号但无端口 → 默认端口（等价旧 parseInt("") → NaN → 回落）', () => {
    expect(parseAddressWithDefault('10.0.0.5:', 8765)).toEqual({ address: '10.0.0.5', port: 8765 })
  })
  it('容忍首尾空白', () => {
    expect(parseAddressWithDefault('  desk-1:7000  ', 8765)).toEqual({ address: 'desk-1', port: 7000 })
  })
  it('空串 / 空主机 / 非法端口 → null（反例）', () => {
    expect(parseAddressWithDefault('', 8765)).toBeNull()
    expect(parseAddressWithDefault('   ', 8765)).toBeNull()
    expect(parseAddressWithDefault(':8765', 8765)).toBeNull()
    expect(parseAddressWithDefault('host:abc', 8765)).toBeNull()
    expect(parseAddressWithDefault('host:0', 8765)).toBeNull()
    expect(parseAddressWithDefault('host:70000', 8765)).toBeNull()
  })
})

describe('formatLastConnected 相对时间（旧 formatLastConnected 口径）', () => {
  /** t 替身：回显 key 并带上参数，便于断言分支与参数 */
  const t = (key: string, params?: Record<string, unknown>) =>
    params ? `${key}:${JSON.stringify(params)}` : key
  const ago = (ms: number) => new Date(Date.now() - ms).toISOString()

  it('未给 / 非法时间 → 空串（不渲染元信息行）', () => {
    expect(formatLastConnected(undefined, t)).toBe('')
    expect(formatLastConnected('not-a-date', t)).toBe('')
  })
  it('1 分钟内 → 刚刚', () => {
    expect(formatLastConnected(ago(30 * 1000), t)).toBe('hub.timeJustNow')
  })
  it('分钟 / 小时 / 天 三档带计数', () => {
    expect(formatLastConnected(ago(5 * 60 * 1000), t)).toBe('hub.timeMinutesAgo:{"count":5}')
    expect(formatLastConnected(ago(3 * 3600 * 1000), t)).toBe('hub.timeHoursAgo:{"count":3}')
    expect(formatLastConnected(ago(2 * 24 * 3600 * 1000), t)).toBe('hub.timeDaysAgo:{"count":2}')
  })
  it('超过 30 天 → 绝对日期 YYYY-MM-DD', () => {
    expect(formatLastConnected(ago(40 * 24 * 3600 * 1000), t)).toMatch(/^\d{4}-\d{2}-\d{2}$/)
  })
})

describe('sessionStatusKey / isSessionActive 会话状态', () => {
  it('运行中/等待输入 → 活跃，其余 → 已停止文案', () => {
    expect(sessionStatusKey('running')).toBe('hub.statusRunning')
    expect(sessionStatusKey('waitingInput')).toBe('hub.statusWaiting')
    expect(sessionStatusKey('stopped')).toBe('hub.statusStopped')
    expect(sessionStatusKey(undefined as unknown as string)).toBe('hub.statusStopped')
  })
  it('isSessionActive 仅运行中/等待输入为真（停止/未知/未定义为假）', () => {
    expect(isSessionActive('running')).toBe(true)
    expect(isSessionActive('waitingInput')).toBe(true)
    expect(isSessionActive('stopped')).toBe(false)
    expect(isSessionActive('')).toBe(false)
  })
})