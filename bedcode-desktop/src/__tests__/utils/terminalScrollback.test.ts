import { describe, it, expect } from 'vitest'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'

/**
 * 回归测试：终端可滚动历史行数
 *
 * 设计变更（区别于旧版 xterm 30000 对齐后端 25000）：后端
 * channels.global_queue_capacity 放宽到 50000 事件（主要为移动端提供深回放），
 * 桌面端 xterm scrollback 有意解耦为浅层 10000 行渲染缓冲。一个后端事件 = 单次
 * PTY 读取（常为几字节：一次按键回显、一行输出片段），远不足一行，故 50000
 * 事件展开后远少于 50000 可见行，桌面端仅保留最近 10000 行不会丢失移动端所需的
 * 深历史（深历史存于后端队列，按需回放，不常驻桌面展开）。
 */

// Rust 端 channels.global_queue_capacity 默认值（事件数）
const BACKEND_QUEUE_EVENTS = 50000

function createTerminal(scrollback: number) {
  const container = document.createElement('div')
  container.style.width = '800px'
  container.style.height = '480px'
  document.body.appendChild(container)
  const t = new Terminal({ scrollback, cols: 80, rows: 24, fontSize: 14, allowProposedApi: true })
  t.open(container)
  return { t, container }
}

function flushRaf() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
  })
}

describe('terminal scrollback', () => {
  it('桌面端 scrollback 有意解耦：浅层 10000 行，小于后端队列 50000 事件', () => {
    expect(TERMINAL_SCROLLBACK).toBe(10000)
    expect(BACKEND_QUEUE_EVENTS).toBe(50000)
    // 有意解耦：桌面渲染缓冲小于后端事件容量（深历史由后端持有、供移动端回放）
    expect(TERMINAL_SCROLLBACK).toBeLessThan(BACKEND_QUEUE_EVENTS)
  })

  it('桌面端写入 50000 行后仅保留最近约 10000 行（有意浅层，非 bug）', async () => {
    const { t, container } = createTerminal(TERMINAL_SCROLLBACK)
    const line = 'x'.repeat(10) + '\r\n'
    t.write(line.repeat(BACKEND_QUEUE_EVENTS))
    await flushRaf()
    // scrollback 10000 + 当前屏 24 行，写入远超此量的 50000 行后仅保留最近部分
    expect(t.buffer.active.length).toBeLessThanOrEqual(TERMINAL_SCROLLBACK + 24)
    expect(t.buffer.active.length).toBeGreaterThan(0)
    t.dispose()
    container.remove()
  })

  it('scrollback 10000 浅层行为：写入 20000 行后最早历史按预期被丢弃', async () => {
    const { t, container } = createTerminal(10000)
    const line = 'x'.repeat(10) + '\r\n'
    t.write(line.repeat(20000))
    await flushRaf()
    // 仅保留 scrollback 10000 + 可见 24 行，旧的「10000 配置症状」现属设计预期
    expect(t.buffer.active.length).toBeLessThanOrEqual(10000 + 24)
    t.dispose()
    container.remove()
  })
})
