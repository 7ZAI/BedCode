/**
 * 宿主壳屏幕栈导航 行为契约测试
 *
 * 覆盖：压栈/回退/Tab 重置/同屏去重/参数携带/根屏保护/方向标记。
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { useShellNavigation, resetShellNavigation } from '@/shell/composables/useShellNavigation'

const nav = useShellNavigation()

beforeEach(() => {
  // 导航栈是模块级单例，用例间必须复位，否则互相污染
  resetShellNavigation()
})

describe('ShellNavigation 栈语义', () => {
  it('should_startOnHome_when_reset', () => {
    expect(nav.current.value.id).toBe('home')
    expect(nav.canBack.value).toBe(false)
  })

  it('should_pushScreenAndMarkForward_when_navigateToNewScreen', () => {
    nav.navigate('apps')

    expect(nav.current.value.id).toBe('apps')
    expect(nav.direction.value).toBe('forward')
    expect(nav.canBack.value).toBe(true)
  })

  it('should_notPushDuplicate_when_navigatingToSameScreenWithSameParams', () => {
    nav.openApp('app-1')
    nav.openApp('app-1')

    // 同一应用重复点击不应堆出两层栈（否则要按两次返回才能退出）
    expect(nav.stack.value).toHaveLength(2)
  })

  it('should_pushSeparateLayer_when_sameScreenWithDifferentParams', () => {
    nav.openApp('app-1')
    nav.openApp('app-2')

    expect(nav.stack.value).toHaveLength(3)
    expect(nav.params.value.appId).toBe('app-2')
  })

  it('should_popBackToExistingLayer_when_navigatingToScreenAlreadyInStack', () => {
    nav.openApp('app-1')
    nav.navigate('settings')
    expect(nav.stack.value).toHaveLength(3)

    nav.openApp('app-1')
    // 回退到该层而不是继续压栈；方向必须是 backward，否则动画反向
    expect(nav.stack.value).toHaveLength(2)
    expect(nav.current.value.id).toBe('app-run')
    expect(nav.direction.value).toBe('backward')
  })

  it('should_popOneLayer_when_backCalled', () => {
    nav.navigate('apps')
    nav.back()

    expect(nav.current.value.id).toBe('home')
    expect(nav.direction.value).toBe('backward')
  })

  it('should_stayOnHome_when_backCalledAtRoot', () => {
    nav.back()

    // 根屏回退无操作（否则会 pop 出空栈，current 落到 undefined）
    expect(nav.current.value.id).toBe('home')
    expect(nav.stack.value).toHaveLength(1)
  })

  it('should_resetToSingleLayer_when_switchTabCalled', () => {
    nav.openApp('app-1')
    nav.switchTab('apps')

    // Tab 切换是「回到该 Tab 的根」，不是叠加历史
    expect(nav.stack.value).toHaveLength(1)
    expect(nav.current.value.id).toBe('apps')
    expect(nav.params.value.appId).toBeUndefined()
  })

  it('should_goHome_when_goHomeCalledFromDeepStack', () => {
    nav.openApp('app-1')
    nav.navigate('settings')
    nav.goHome()

    expect(nav.stack.value).toHaveLength(1)
    expect(nav.current.value.id).toBe('home')
  })

  it('should_carryAppId_when_openDetailOrOpenApp', () => {
    nav.openDetail('app-9')
    expect(nav.current.value.id).toBe('app-detail')
    expect(nav.params.value.appId).toBe('app-9')

    resetShellNavigation()
    nav.openApp('app-9')
    expect(nav.current.value.id).toBe('app-run')
    expect(nav.params.value.appId).toBe('app-9')
  })

  it('should_markBackwardAndKeepReachability_when_openSwitcherThenBack', () => {
    nav.openSwitcher()
    expect(nav.current.value.id).toBe('switcher')

    nav.back()
    // 多任务是临时浮层：回退即消失，不留在历史里
    expect(nav.current.value.id).toBe('home')
    expect(nav.stack.value).toHaveLength(1)
  })
})
