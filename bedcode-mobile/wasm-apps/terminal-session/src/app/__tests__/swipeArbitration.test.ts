/**
 * 页签横滑仲裁 行为契约测试
 * （票 2026-10-10：全量 UI 下沉 —— app 域页签容器）
 *
 * 被测：`src/app/swipeArbitration.ts`。
 * 模块级单例，测前必须复位，避免用例间串味。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-SA1 | delegateSwipe 无外层分支 | 未注册外层容器 | 返回 false，不抛 |
 * | C-SA2 | delegateSwipe 有外层分支 | 外层容器在场 | 调用外层回调（方向透传）并返回 true |
 * | C-SA3 | setSwipeDelegate(null) | 外层卸载后注销 | 再次上交返回 false |
 * | C-SA4 | hasSwipeDelegate | 外层在场判定 | true / false 随注册注销翻转 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { delegateSwipe, hasSwipeDelegate, setSwipeDelegate } from '../swipeArbitration'

beforeEach(() => {
  // 单例复位：用例间必须互不污染
  setSwipeDelegate(null)
})

describe('C-SA1 无外层容器', () => {
  it('should_returnFalse_when_noDelegateRegistered', () => {
    expect(hasSwipeDelegate()).toBe(false)
    expect(delegateSwipe('left')).toBe(false)
  })
})

describe('C-SA2 有外层容器', () => {
  it('should_forwardDirectionToDelegate_when_registered', () => {
    const delegate = vi.fn()
    setSwipeDelegate(delegate)

    const consumed = delegateSwipe('left')

    expect(consumed).toBe(true)
    expect(delegate).toHaveBeenCalledTimes(1)
    expect(delegate).toHaveBeenCalledWith('left')
  })

  it('should_forwardRightDirection_when_swipingRight', () => {
    const delegate = vi.fn()
    setSwipeDelegate(delegate)

    delegateSwipe('right')

    expect(delegate).toHaveBeenCalledWith('right')
  })

  it('should_useLatestDelegate_when_reRegistered', () => {
    // 外层容器重挂载（切 app 再回来）后应走新回调，不得仍调旧的
    const stale = vi.fn()
    const fresh = vi.fn()
    setSwipeDelegate(stale)
    setSwipeDelegate(fresh)

    delegateSwipe('left')

    expect(stale).not.toHaveBeenCalled()
    expect(fresh).toHaveBeenCalledTimes(1)
  })
})

describe('C-SA3/C-SA4 外层卸载', () => {
  it('should_returnFalseAgain_when_delegateCleared', () => {
    const delegate = vi.fn()
    setSwipeDelegate(delegate)
    expect(hasSwipeDelegate()).toBe(true)

    setSwipeDelegate(null)

    expect(hasSwipeDelegate()).toBe(false)
    expect(delegateSwipe('left')).toBe(false)
    expect(delegate).not.toHaveBeenCalled()
  })
})