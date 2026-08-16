/**
 * SDK Select 面板定位纯函数单测（接缝 1）
 *
 * 规则表驱动：下方足够向下、下方不足上方足够向上、两侧都不足收缩 maxHeight、
 * 水平 clamp 各象限。纯函数无 DOM，直接按规则断言。
 */
import { describe, it, expect } from 'vitest'
import { computeSelectPosition, SELECT_GAP, SELECT_MAX_PANEL_HEIGHT } from '../src/ui/select-position'

// 典型面板：设计高度 240；触发器宽 200、高 32，位于页面中部
const TRIGGER = { top: 400, bottom: 432, left: 100, width: 200 }
const VIEWPORT = { width: 1200, height: 800 }
const PANEL = 200

describe('computeSelectPosition', () => {
  it('下方空间足够 → 向下展开（现状行为不变）', () => {
    const pos = computeSelectPosition(TRIGGER, VIEWPORT, PANEL)
    expect(pos.top).toBe(TRIGGER.bottom + SELECT_GAP)
    expect(pos.left).toBe(TRIGGER.left)
    expect(pos.maxHeight).toBe(PANEL)
  })

  it('下方不足但上方足够 → 向上展开', () => {
    // 触发器贴底：bottom = 780，下方仅 800-780-4=16 < 200；上方充足
    const trigger = { top: 748, bottom: 780, left: 100, width: 200 }
    const pos = computeSelectPosition(trigger, VIEWPORT, PANEL)
    expect(pos.top).toBe(trigger.top - SELECT_GAP - PANEL)
    expect(pos.maxHeight).toBe(PANEL)
    // 面板完全落在视口内
    expect(pos.top + PANEL).toBeLessThanOrEqual(VIEWPORT.height)
    expect(pos.top).toBeGreaterThanOrEqual(0)
  })

  it('两侧都不足且下方空间更大 → 贴下方展开并收缩 maxHeight', () => {
    // 视口高 300：下方可用 300-132-4=164、上方 100-4=96，均放不下 200 面板
    const viewport = { width: 1200, height: 300 }
    const trigger = { top: 100, bottom: 132, left: 100, width: 200 }
    const pos = computeSelectPosition(trigger, viewport, PANEL)
    // 下方更大 → 向下并收缩
    expect(pos.top).toBe(trigger.bottom + SELECT_GAP)
    expect(pos.maxHeight).toBe(300 - trigger.bottom - 2 * SELECT_GAP)
    expect(pos.maxHeight).toBeLessThan(PANEL)
  })

  it('两侧都不足且上方空间更大 → 向上展开并收缩 maxHeight', () => {
    const viewport = { width: 1200, height: 300 }
    const trigger = { top: 200, bottom: 232, left: 100, width: 200 }
    const pos = computeSelectPosition(trigger, viewport, PANEL)
    // 上方可用 200-4=196 > 下方 300-232-4=64 → 向上并收缩；
    // maxHeight 再让出一个间距（面板底边与触发器保留 4px 间隙），
    // top 被 SELECT_GAP 下界 clamp 到视口边缘留白
    expect(pos.maxHeight).toBe(200 - 2 * SELECT_GAP)
    expect(pos.top).toBe(SELECT_GAP)
    expect(pos.top).toBeGreaterThanOrEqual(0)
  })

  it('面板高度超过设计上限 → 以 SELECT_MAX_PANEL_HEIGHT 截断', () => {
    const pos = computeSelectPosition(TRIGGER, VIEWPORT, 9999)
    expect(pos.maxHeight).toBe(SELECT_MAX_PANEL_HEIGHT)
  })

  it('水平 clamp：触发器贴近右侧 → 面板不超出视口右缘', () => {
    const trigger = { top: 400, bottom: 432, left: 1150, width: 200 }
    const pos = computeSelectPosition(trigger, VIEWPORT, PANEL)
    expect(pos.left).toBe(VIEWPORT.width - trigger.width - SELECT_GAP)
    expect(pos.left + trigger.width).toBeLessThanOrEqual(VIEWPORT.width)
  })

  it('水平 clamp：触发器超出左缘 → 面板贴左缘', () => {
    const trigger = { top: 400, bottom: 432, left: -50, width: 200 }
    const pos = computeSelectPosition(trigger, VIEWPORT, PANEL)
    expect(pos.left).toBe(SELECT_GAP)
  })

  it('水平 clamp：常规位置保持与触发器对齐', () => {
    const pos = computeSelectPosition(TRIGGER, VIEWPORT, PANEL)
    expect(pos.left).toBe(TRIGGER.left)
  })
})
