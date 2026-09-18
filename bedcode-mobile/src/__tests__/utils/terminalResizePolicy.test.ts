/**
 * 终端网格 resize 触发策略测试
 *
 * 被测契约（resolveGridResize）：
 * - C-001 列偏差 ≤1 且行不变 → null（不 resize，避免无谓 resize 事件）
 * - C-002 列偏差 ≤1 且行变化 → 保持当前列 + 采用目标行（真实变化必须生效，
 *         但 ±1 列漂移不得写进网格：列一变 xterm 即走 Buffer._reflow 整缓冲重排，
 *         代价随 scrollback 行数线性上升——键盘避让场景的真机问题）
 * - C-003 列偏差 >1 → 采用目标列（真实宽度/字号变化）
 * - C-004 行任意变化立即生效（双向同权，含 ±1）
 * - C-005 与当前网格完全一致 → null
 */
import { describe, it, expect } from 'vitest'
import { resolveGridResize, RESIZE_GRID_TOLERANCE, ATLAS_PREHEAT_DELAY_MS } from '@/utils/terminalResizePolicy'

describe('resolveGridResize', () => {
  describe('列 ±1 漂移（测量误差，不得写进网格）', () => {
    it('列 -1 且行不变 → 不 resize', () => {
      expect(resolveGridResize(80, 24, 79, 24)).toBeNull()
    })

    it('列 +1 且行不变 → 不 resize', () => {
      expect(resolveGridResize(80, 24, 81, 24)).toBeNull()
    })

    it('列 ±1 且行变化 → 保持当前列 + 采用目标行（关键：避免整缓冲重排）', () => {
      expect(resolveGridResize(96, 52, 97, 31)).toEqual({ cols: 96, rows: 31 })
      expect(resolveGridResize(96, 31, 95, 52)).toEqual({ cols: 96, rows: 52 })
    })
  })

  describe('行变化立即生效（双向同权）', () => {
    it('行缩小 1 → 采用目标行', () => {
      expect(resolveGridResize(80, 24, 80, 23)).toEqual({ cols: 80, rows: 23 })
    })

    it('行增大 1 → 采用目标行（网格贴底对齐时行偏少会在顶部露出空带）', () => {
      expect(resolveGridResize(80, 24, 80, 25)).toEqual({ cols: 80, rows: 25 })
    })

    it('行缩小 2 → 采用目标行', () => {
      expect(resolveGridResize(80, 24, 80, 22)).toEqual({ cols: 80, rows: 22 })
    })
  })

  describe('列偏差 > 1（真实宽度/字号变化）', () => {
    it('列缩小 2 → 采用目标列', () => {
      expect(resolveGridResize(80, 24, 78, 24)).toEqual({ cols: 78, rows: 24 })
    })

    it('列增大 2 → 采用目标列', () => {
      expect(resolveGridResize(80, 24, 82, 24)).toEqual({ cols: 82, rows: 24 })
    })

    it('列与行同时真实变化 → 两者都采用', () => {
      expect(resolveGridResize(80, 24, 90, 30)).toEqual({ cols: 90, rows: 30 })
    })
  })

  describe('无变化', () => {
    it('行列完全一致 → 不 resize', () => {
      expect(resolveGridResize(80, 24, 80, 24)).toBeNull()
    })
  })

  describe('常量', () => {
    it('列漂移阈值为 1', () => {
      expect(RESIZE_GRID_TOLERANCE).toBe(1)
    })

    it('atlas 预热延迟为 700ms', () => {
      expect(ATLAS_PREHEAT_DELAY_MS).toBe(700)
    })
  })
})
