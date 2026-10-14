/**
 * 宿主页纯函数测试（票 2026-10-09）
 * 契约：地址解析正反例 / 状态文案 key 映射 / 会话活跃判定边界 / 未知状态兜底。
 */
import { describe, expect, it } from 'vitest'
import { isSessionActive, parseAddress, sessionStatusKey, statusKey } from '../utils'

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