/**
 * terminalRowClip 单元测试
 *
 * 覆盖：溢出空白 span 裁切、含文字 span 保留、亚像素容差、
 * 漂移消失后清除历史裁切、幂等（重复扫描不重复写样式）。
 * happy-dom 无真实布局，矩形以 getBoundingClientRect 覆写注入。
 */
import { describe, it, expect } from 'vitest'
import { scanRowBackgroundOverflow } from '@/utils/terminalRowClip'

const ROW_RIGHT = 389

function makeRow(): HTMLElement {
  const row = document.createElement('div')
  row.getBoundingClientRect = () =>
    ({ right: ROW_RIGHT, width: 389, left: 0 }) as DOMRect
  return row
}

function makeSpan(right: number, width: number, text: string): HTMLElement {
  const span = document.createElement('span')
  span.textContent = text
  span.getBoundingClientRect = () => ({ right, width }) as DOMRect
  return span
}

function makeRowsEl(rows: HTMLElement[]): HTMLElement {
  const rowsEl = document.createElement('div')
  for (const r of rows) rowsEl.appendChild(r)
  document.body.appendChild(rowsEl)
  return rowsEl
}

describe('scanRowBackgroundOverflow', () => {
  it('clips overflowing whitespace-only span at the row boundary', () => {
    const row = makeRow()
    row.appendChild(makeSpan(403.6, 21.6, '   '))
    const rowsEl = makeRowsEl([row])

    const changed = scanRowBackgroundOverflow(rowsEl)
    expect(changed).toBe(1)
    const span = row.querySelector('span') as HTMLElement
    expect(span.style.clipPath).toBe('inset(0 14.6px 0 0)')
    rowsEl.remove()
  })

  it('leaves overflowing text spans untouched (ink protection)', () => {
    const row = makeRow()
    row.appendChild(makeSpan(403.6, 21.6, '完'))
    const rowsEl = makeRowsEl([row])

    expect(scanRowBackgroundOverflow(rowsEl)).toBe(0)
    expect((row.querySelector('span') as HTMLElement).style.clipPath).toBe('')
    rowsEl.remove()
  })

  it('ignores in-bounds spans and sub-pixel overflow within epsilon', () => {
    const row = makeRow()
    row.appendChild(makeSpan(389, 21.6, '   '))
    row.appendChild(makeSpan(389.5, 10, '   '))
    const rowsEl = makeRowsEl([row])

    expect(scanRowBackgroundOverflow(rowsEl)).toBe(0)
    for (const s of row.querySelectorAll('span')) {
      expect((s as HTMLElement).style.clipPath).toBe('')
    }
    rowsEl.remove()
  })

  it('clears stale clips when drift disappears after re-layout', () => {
    const row = makeRow()
    const span = makeSpan(389, 21.6, '   ')
    span.style.clipPath = 'inset(0 14.6px 0 0)'
    row.appendChild(span)
    const rowsEl = makeRowsEl([row])

    expect(scanRowBackgroundOverflow(rowsEl)).toBe(1)
    expect(span.style.clipPath).toBe('')
    rowsEl.remove()
  })

  it('is idempotent: repeated scans do not rewrite styles', () => {
    const row = makeRow()
    row.appendChild(makeSpan(403.6, 21.6, '   '))
    const rowsEl = makeRowsEl([row])

    scanRowBackgroundOverflow(rowsEl)
    const span = row.querySelector('span') as HTMLElement
    const written = span.style.clipPath
    expect(scanRowBackgroundOverflow(rowsEl)).toBe(0)
    expect(span.style.clipPath).toBe(written)
    rowsEl.remove()
  })
})
