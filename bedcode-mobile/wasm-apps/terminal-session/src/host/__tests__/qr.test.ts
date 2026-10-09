/**
 * 二维码载荷解析 行为契约测试
 * （票 2026-10-09：旧宿主 ScanPanel 的 `{host,port,token}` 契约迁入插件）
 *
 * 被测：`src/host/qr.ts::parseQrText`。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-QR-1 | 成功分支 | `{host,port,token}` 齐备且端口可归正整数 | `{ok:true, payload}` |
 * | C-QR-2 | try/catch | JSON 非法 / 非对象（数组、数字、null）/ 非字符串 | `malformed` |
 * | C-QR-3 | 字段校验 | host / token 缺失或空白；端口缺失/0/负/非数字/越界/小数 | `incomplete` |
 * | C-QR-4 | 归一 | host 去首尾空白；数字字符串端口转数字 | 归一后的载荷 |
 * | C-QR-5 | 健壮性 | 额外字段不参与判定，不影响成功 | 仍成功且载荷只含三字段 |
 */
import { describe, it, expect } from 'vitest'
import { parseQrText } from '../qr'

describe('C-QR-1/C-QR-4 合法载荷', () => {
  it('should_returnPayload_when_allFieldsPresent', () => {
    const result = parseQrText(JSON.stringify({ host: '192.168.1.5', port: 8765, token: 'abc123' }))
    expect(result).toEqual({ ok: true, payload: { host: '192.168.1.5', port: 8765, token: 'abc123' } })
  })

  it('should_normalizeHostAndNumericPort_when_whitespaceOrStringPort', () => {
    const result = parseQrText(JSON.stringify({ host: '  desktop.local  ', port: '8765', token: 't' }))
    expect(result).toEqual({
      ok: true,
      payload: { host: 'desktop.local', port: 8765, token: 't' },
    })
  })

  it('should_ignoreExtraFields_when_payloadHasMoreKeys', () => {
    const result = parseQrText(
      JSON.stringify({ host: 'h', port: 1, token: 't', version: 2, note: 'x' }),
    )
    expect(result.ok).toBe(true)
    if (result.ok) expect(Object.keys(result.payload).sort()).toEqual(['host', 'port', 'token'])
  })
})

describe('C-QR-2 malformed（JSON / 结构非法）', () => {
  const malformedInputs: Array<{ label: string; input: unknown }> = [
    { label: '非 JSON 文本', input: 'not-json-at-all' },
    { label: '空串', input: '' },
    { label: '仅空白', input: '   ' },
    { label: '非字符串', input: { host: 'h', port: 1, token: 't' } },
    { label: 'undefined', input: undefined },
    { label: 'JSON 数组', input: '[{"host":"h","port":1,"token":"t"}]' },
    { label: 'JSON 数字', input: '42' },
    { label: 'JSON null', input: 'null' },
  ]

  for (const c of malformedInputs) {
    it(`should_rejectAsMalformed_when_${c.label}`, () => {
      expect(parseQrText(c.input)).toEqual({ ok: false, reason: 'malformed' })
    })
  }
})

describe('C-QR-3 incomplete（字段缺失 / 越界）', () => {
  const incompletePayloads: Array<{ label: string; payload: Record<string, unknown> }> = [
    { label: '缺 host', payload: { port: 8765, token: 't' } },
    { label: 'host 空串', payload: { host: '', port: 8765, token: 't' } },
    { label: 'host 仅空白', payload: { host: '   ', port: 8765, token: 't' } },
    { label: 'host 非字符串', payload: { host: 5, port: 8765, token: 't' } },
    { label: '缺 port', payload: { host: 'h', token: 't' } },
    { label: 'port 为 0', payload: { host: 'h', port: 0, token: 't' } },
    { label: 'port 负数', payload: { host: 'h', port: -1, token: 't' } },
    { label: 'port 非数字', payload: { host: 'h', port: 'abc', token: 't' } },
    { label: 'port 小数', payload: { host: 'h', port: 87.65, token: 't' } },
    { label: 'port 越界', payload: { host: 'h', port: 70000, token: 't' } },
    { label: '缺 token', payload: { host: 'h', port: 8765 } },
    { label: 'token 空串', payload: { host: 'h', port: 8765, token: '' } },
    { label: 'token 非字符串', payload: { host: 'h', port: 8765, token: 1 } },
  ]

  for (const c of incompletePayloads) {
    it(`should_rejectAsIncomplete_when_${c.label}`, () => {
      expect(parseQrText(JSON.stringify(c.payload))).toEqual({ ok: false, reason: 'incomplete' })
    })
  }
})
