import { describe, it, expect } from 'vitest'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'

/**
 * 回归测试：终端可滚动历史行数（bug：scrollback 硬编码 10000，
 * 后端队列 25000 事件的历史写入后最早部分被 xterm buffer 丢弃，
 * 只能滚动最近 10000 行）
 */

// Rust 端 channels.global_queue_capacity 默认值（事件数）
const BACKEND_QUEUE_EVENTS = 25000

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
  it('TERMINAL_SCROLLBACK 不得低于后端队列容量（防止历史无法完整滚动）', () => {
    expect(TERMINAL_SCROLLBACK).toBeGreaterThanOrEqual(BACKEND_QUEUE_EVENTS)
  })

  it('写入 25000 行后 buffer 完整保留（可滚动全部历史）', async () => {
    const { t, container } = createTerminal(TERMINAL_SCROLLBACK)
    const line = 'x'.repeat(10) + '\r\n'
    t.write(line.repeat(BACKEND_QUEUE_EVENTS))
    await flushRaf()
    // 25000 行 + 当前屏 24 行，全部保留
    expect(t.buffer.active.length).toBeGreaterThanOrEqual(BACKEND_QUEUE_EVENTS)
    t.dispose()
    container.remove()
  })

  it('scrollback 不足时最早历史被丢弃（旧 10000 配置下的症状）', async () => {
    const { t, container } = createTerminal(10000)
    const line = 'x'.repeat(10) + '\r\n'
    t.write(line.repeat(BACKEND_QUEUE_EVENTS))
    await flushRaf()
    // 只有 scrollback 10000 + 可见 24 行，写入 25000 行后仅保留最近 10024 行
    expect(t.buffer.active.length).toBeLessThanOrEqual(10000 + 24)
    t.dispose()
    container.remove()
  })
})
