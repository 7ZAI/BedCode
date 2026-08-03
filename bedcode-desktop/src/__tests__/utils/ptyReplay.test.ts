import { describe, it, expect } from 'vitest'
import { pendingReplayEvents, advanceWatermark } from '@/utils/ptyReplay'

describe('ptyReplay 去重', () => {
  it('实时流与历史回放共享水位：同一 index 只写入终端一次', () => {
    const writes: number[] = []
    const cacheAppends: number[] = []
    const write = (ev: { index: number }) => {
      writes.push(ev.index)
      cacheAppends.push(ev.index)
    }

    let watermark = 0

    // 1. 历史回放 fetch 期间，实时流先到达 1,2,3（terminal 已就绪，直接写入并推进水位）
    const live = [{ index: 1 }, { index: 2 }, { index: 3 }]
    for (const ev of live) {
      if (ev.index <= watermark) continue
      write(ev)
      watermark = advanceWatermark(watermark, ev.index)
    }

    // 2. 历史快照返回 1..5（包含已实时送达的 1,2,3）
    const history = [{ index: 1 }, { index: 2 }, { index: 3 }, { index: 4 }, { index: 5 }]
    const pending = pendingReplayEvents(history, watermark)
    for (const ev of pending.events) write(ev)
    watermark = Math.max(pending.nextWatermark, 5)

    // 每个 index 恰好写入一次：无重复、无缺失
    expect(writes).toEqual([1, 2, 3, 4, 5])
    expect(cacheAppends).toEqual([1, 2, 3, 4, 5])
    expect(watermark).toBe(5)
  })

  it('历史快照陈旧（maxSeq 落后于实时水位）时水位不倒退', () => {
    let watermark = 0
    // 实时流推进到 10
    for (const idx of [1, 2, 3, 10]) {
      watermark = advanceWatermark(watermark, idx)
    }
    // 历史快照只到 8，且不含 9/10
    const history = [{ index: 1 }, { index: 2 }, { index: 3 }, { index: 8 }]
    const pending = pendingReplayEvents(history, watermark)
    expect(pending.events).toEqual([])
    // 最终水位取 max，不允许退回 8
    expect(Math.max(pending.nextWatermark, 8)).toBe(10)
  })

  it('pendingReplayEvents 推进水位到历史最大值', () => {
    const { events, nextWatermark } = pendingReplayEvents(
      [{ index: 10 }, { index: 11 }],
      9,
    )
    expect(events.map((e) => e.index)).toEqual([10, 11])
    expect(nextWatermark).toBe(11)
  })

  it('pendingReplayEvents 过滤掉已写入的事件', () => {
    const { events } = pendingReplayEvents(
      [{ index: 1 }, { index: 2 }, { index: 3 }],
      2,
    )
    expect(events.map((e) => e.index)).toEqual([3])
  })

  it('advanceWatermark 单调不减', () => {
    expect(advanceWatermark(5, 3)).toBe(5)
    expect(advanceWatermark(5, 7)).toBe(7)
  })
})
