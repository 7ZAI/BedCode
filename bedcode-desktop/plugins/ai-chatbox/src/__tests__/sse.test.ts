/**
 * SseBuffer 单测：跨 chunk 断行缓冲、多事件一包、CRLF、残留缓冲、flush 语义
 */
import { describe, it, expect } from 'vitest'
import { SseBuffer } from '../adapters/sse'

function sse(payload: string): string {
  return `data: ${payload}\n\n`
}

describe('SseBuffer', () => {
  it('单事件整包解析', () => {
    const buf = new SseBuffer()
    expect(buf.push(sse('{"a":1}'))).toEqual(['{"a":1}'])
  })

  it('跨 chunk 断行：事件被切碎到多次 push 仍完整产出', () => {
    const buf = new SseBuffer()
    const event = sse('{"choices":[{"delta":{"content":"跨块"}}]}')
    const mid = 10
    expect(buf.push(event.slice(0, mid))).toEqual([])
    expect(buf.push(event.slice(mid))).toEqual(['{"choices":[{"delta":{"content":"跨块"}}]}'])
  })

  it('一包多事件：全部产出且按序', () => {
    const buf = new SseBuffer()
    expect(buf.push(sse('{"a":1}') + sse('{"b":2}'))).toEqual(['{"a":1}', '{"b":2}'])
  })

  it('CRLF 行尾：\r\n 事件分隔符正常识别', () => {
    const buf = new SseBuffer()
    expect(buf.push('data: {"a":1}\r\n\r\n')).toEqual(['{"a":1}'])
  })

  it('data: 前缀剥离 + 空白 data 忽略', () => {
    const buf = new SseBuffer()
    expect(buf.push('data:\n\ndata: x\n\n')).toEqual(['x'])
  })

  it('多行 data 按 \\n 拼接（SSE 规范）', () => {
    const buf = new SseBuffer()
    expect(buf.push('data: {"a":\ndata: 1}\n\n')).toEqual(['{"a":\n1}'])
  })

  it('event: 行（无 data）的事件跳过', () => {
    const buf = new SseBuffer()
    expect(buf.push('event: ping\n\n')).toEqual([])
  })

  it('残留未闭合缓冲保留到下次 push', () => {
    const buf = new SseBuffer()
    expect(buf.push('data: {"a":')).toEqual([])
    expect(buf.push('1}\n\n')).toEqual(['{"a":1}'])
  })

  it('flush：残留完整事件作为最后一条产出', () => {
    const buf = new SseBuffer()
    // 首条完整事件在 push 时即产出，第二条留在缓冲内等 flush
    expect(buf.push('data: {"a":1}\n\ndata: {"b":2}')).toEqual(['{"a":1}'])
    expect(buf.flush()).toEqual(['{"b":2}'])
    // flush 后缓冲清空，后续 push 从空开始
    expect(buf.push('data: {"c":3}\n\n')).toEqual(['{"c":3}'])
  })

  it('flush：空缓冲不产出', () => {
    const buf = new SseBuffer()
    expect(buf.flush()).toEqual([])
  })

  it('[DONE] 行原样透传（终结语义由 adapter 判定）', () => {
    const buf = new SseBuffer()
    expect(buf.push('data: [DONE]\n\n')).toEqual(['[DONE]'])
  })
})
