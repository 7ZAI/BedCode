/**
 * 行级 LCS diff 纯函数单测（票据 04，保存前 diff 预览的真源逻辑）
 */
import { describe, expect, it } from 'vitest'
import { diffLines, diffStats } from '../utils/diff'

describe('diffLines', () => {
  it('identical content yields only context lines', () => {
    const text = 'a\nb\nc'
    const lines = diffLines(text, text)
    expect(lines).toEqual([
      { type: 'ctx', text: 'a' },
      { type: 'ctx', text: 'b' },
      { type: 'ctx', text: 'c' },
    ])
    expect(diffStats(lines)).toEqual({ added: 0, removed: 0 })
  })

  it('marks modified line as del followed by add', () => {
    const lines = diffLines('name: old\nkeep: 1', 'name: new\nkeep: 1')
    expect(lines).toEqual([
      { type: 'del', text: 'name: old' },
      { type: 'add', text: 'name: new' },
      { type: 'ctx', text: 'keep: 1' },
    ])
    expect(diffStats(lines)).toEqual({ added: 1, removed: 1 })
  })

  it('detects pure insertion and deletion', () => {
    expect(diffLines('a\nc', 'a\nb\nc')).toEqual([
      { type: 'ctx', text: 'a' },
      { type: 'add', text: 'b' },
      { type: 'ctx', text: 'c' },
    ])
    expect(diffLines('a\nb\nc', 'a\nc')).toEqual([
      { type: 'ctx', text: 'a' },
      { type: 'del', text: 'b' },
      { type: 'ctx', text: 'c' },
    ])
  })

  it('handles empty inputs on either side', () => {
    expect(diffLines('', 'new line')).toEqual([{ type: 'add', text: 'new line' }])
    expect(diffLines('old line', '')).toEqual([{ type: 'del', text: 'old line' }])
    expect(diffLines('', '')).toEqual([])
  })

  it('is consistent in both directions (same edit distance)', () => {
    const a = 'x\ny\nz\nw'
    const b = 'x\nY\nz\nW'
    const forward = diffStats(diffLines(a, b))
    const backward = diffStats(diffLines(b, a))
    expect(forward.added).toBe(backward.removed)
    expect(forward.removed).toBe(backward.added)
  })

  it('handles multiline frontmatter edits (skill editor shape)', () => {
    const base = '---\nname: ctx7\ndescription: old\n---\n\n# Usage\nrun it\n'
    const draft = '---\nname: ctx7\ndescription: new\n---\n\n# Usage\nrun it\nextra step\n'
    const lines = diffLines(base, draft)
    expect(lines.some((l) => l.type === 'del' && l.text === 'description: old')).toBe(true)
    expect(lines.some((l) => l.type === 'add' && l.text === 'description: new')).toBe(true)
    expect(lines.some((l) => l.type === 'add' && l.text === 'extra step')).toBe(true)
    expect(lines.some((l) => l.type === 'ctx' && l.text === 'name: ctx7')).toBe(true)
  })
})
