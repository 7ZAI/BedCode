/**
 * 页签面板停靠位 行为契约测试
 *
 * 被测：`src/app/panelDock.ts`（dockClassOf / enterSideOf）
 * 附：`src/app/app.css` + `AppRoot.vue` 的停靠形态锁（重叠回归防线）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-PD1 | dockClassOf 当前页分支 | id === active | `--active` |
 * | C-PD2 | dockClassOf 相对位置分支 | 已过页 / 未到页 | `--left` / `--right`（正反例各一） |
 * | C-PD3 | dockClassOf 入场分支 | 新挂载面板带 enterFrom | 按入场侧停靠，不受「它就是当前页」影响 |
 * | C-PD4 | dockClassOf 入场反例 | enterFrom 缺省（已挂载面板） | 回到相对位置判定，不误停入场侧 |
 * | C-PD5 | enterSideOf | 目标在当前页右侧 / 左侧 | `right` / `left` |
 * | C-PD6 | neighborTabId 在界 | left→下一页 / right→上一页 | 目标页签 id |
 * | C-PD7 | neighborTabId 越界（边界） | 首屏 right / 末页 left | null（调用方保持当前页） |
 * | C-PD8 | 形态锁（app.css） | 同屏只允许一页可见 | 无半屏停靠（±24%）、无 opacity 半透；停靠位为 ±100% + visibility |
 * | C-PD9 | 形态锁（AppRoot） | 停靠面板不得溢出到头部 / 导航 | 面板容器带 overflow-hidden |
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  ACTIVE_DOCK,
  LEFT_DOCK,
  RIGHT_DOCK,
  dockClassOf,
  enterSideOf,
  neighborTabId,
} from '../panelDock'
import type { AppTab } from '../useAppTabs'

const ORDER: AppTab[] = ['connection', 'sessions', 'toolbox', 'settings']

describe('C-PD1 当前页停靠', () => {
  it('should_dockActive_when_panelIsCurrentTab', () => {
    expect(
      dockClassOf({ id: 'toolbox', active: 'toolbox', order: ORDER }),
    ).toBe(ACTIVE_DOCK)
  })
})

describe('C-PD2 相对位置停靠', () => {
  it('should_dockLeft_when_panelPassedCurrentTab', () => {
    expect(dockClassOf({ id: 'connection', active: 'toolbox', order: ORDER })).toBe(LEFT_DOCK)
  })

  it('should_dockRight_when_panelNotYetReached', () => {
    expect(dockClassOf({ id: 'settings', active: 'toolbox', order: ORDER })).toBe(RIGHT_DOCK)
  })

  it('should_dockRight_when_activeIsFirstTab', () => {
    // 边界：当前页就是第一页时，其余全部停右侧
    expect(dockClassOf({ id: 'settings', active: 'connection', order: ORDER })).toBe(RIGHT_DOCK)
  })
})

describe('C-PD3/C-PD4 入场停靠', () => {
  it('should_dockByEnterSide_when_panelJustMounted', () => {
    // 新面板此刻就是当前页：若按「当前页 → 居中」渲染，新元素没有过渡起点，
    // 切页会变成硬切。入场侧必须压过当前页判定。
    expect(
      dockClassOf({ id: 'sessions', active: 'sessions', order: ORDER, enterFrom: 'right' }),
    ).toBe(RIGHT_DOCK)
    expect(
      dockClassOf({ id: 'sessions', active: 'sessions', order: ORDER, enterFrom: 'left' }),
    ).toBe(LEFT_DOCK)
  })

  it('should_fallBackToRelativeDock_when_noEnterSide', () => {
    // 反例：常驻停靠（无入场）不得被误判成入场侧
    expect(
      dockClassOf({ id: 'settings', active: 'sessions', order: ORDER, enterFrom: null }),
    ).toBe(RIGHT_DOCK)
  })
})

describe('C-PD5 入场侧判定', () => {
  it('should_enterFromRight_when_targetIsAfterActive', () => {
    expect(enterSideOf('settings', 'connection', ORDER)).toBe('right')
  })

  it('should_enterFromLeft_when_targetIsBeforeActive', () => {
    expect(enterSideOf('connection', 'settings', ORDER)).toBe('left')
  })
})

describe('C-PD6/C-PD7 相邻页签与越界', () => {
  it('should_stepForward_when_swipingLeft', () => {
    expect(neighborTabId(ORDER, 0, 'left')).toBe('sessions')
  })

  it('should_stepBackward_when_swipingRight_fromMiddle', () => {
    expect(neighborTabId(ORDER, 2, 'right')).toBe('sessions')
  })

  it('should_returnNull_when_swipingRightOnFirstTab', () => {
    // 边界：首屏再向前是越界，必须落到 null（否则内层区上交的滑动会跳到不存在的页）
    expect(neighborTabId(ORDER, 0, 'right')).toBeNull()
  })

  it('should_returnNull_when_swipingLeftOnLastTab', () => {
    expect(neighborTabId(ORDER, ORDER.length - 1, 'left')).toBeNull()
  })

  it('should_stayInsideSubset_when_someTabsUnavailable', () => {
    // 可用集只有两页时，取邻不得跳到被过滤掉的页签
    const subset: AppTab[] = ['connection', 'sessions']

    expect(neighborTabId(subset, 0, 'left')).toBe('sessions')
    expect(neighborTabId(subset, 1, 'left')).toBeNull()
  })
})

describe('C-PD8/C-PD9 停靠形态锁（重叠回归防线）', () => {
  const css = readFileSync(join(process.cwd(), 'src/../wasm-apps/bedcode/src/app/app.css'), 'utf8')
  const appRoot = readFileSync(
    join(process.cwd(), 'src/../wasm-apps/bedcode/src/app/AppRoot.vue'),
    'utf8',
  )

  it('should_parkNonCurrentPanelsFullyOffScreen_when_cssScanned', () => {
    expect(css, '停靠位必须完全在屏外（半屏停靠会让两页同时可见）').toContain(
      'transform: translateX(-100%)',
    )
    expect(css).toContain('transform: translateX(100%)')
    // 半屏叠放是「页面重叠」的根因：锁死不得回接
    expect(css, '不得回接 ±24% 半屏停靠').not.toContain('translateX(-24%)')
    expect(css).not.toContain('translateX(24%)')
    expect(css, '不得再用 opacity 半透叠放').not.toContain('opacity: 0.55')
    // 非当前页隐藏，动画走完才移出命中区
    expect(css).toContain('visibility: hidden')
  })

  it('should_clipPanelsInsideContainer_when_appRootScanned', () => {
    // 没有裁切，停靠面板会溢出容器压到头部与导航之上
    expect(appRoot, '面板容器必须裁切').toContain('overflow-hidden')
  })
})
