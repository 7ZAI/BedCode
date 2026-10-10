/**
 * dev-shell 宿主壳屏幕栈 行为契约测试
 * -----------------------------------------------------------------------------
 * 契约来源：dev-shell/src/shell/composables/useShellNavigation.ts
 *
 * 覆盖：同屏同参不重复压栈、已存在屏回退到该层、切 Tab 重置为单栈、
 *       根屏回退无操作、方向标记、专用入口（openApp/openDetail/openSwitcher/
 *       openPermissions）的落点与参数。
 *
 * 导航是全局单例：每个用例先 reset。
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { resetShellNavigation, useShellNavigation } from '../../src/shell/composables/useShellNavigation'

const nav = useShellNavigation()

/** 当前栈的屏幕 id 序列 */
function stackIds(): string[] {
  return nav.stack.value.map((s) => s.id)
}

beforeEach(() => {
  resetShellNavigation()
})

describe('压栈与去重', () => {
  it('should_startAtHome_when_notNavigated', () => {
    expect(nav.current.value.id).toBe('home')
    expect(stackIds()).toEqual(['home'])
    expect(nav.canBack.value).toBe(false)
  })

  it('should_notPushDuplicate_when_sameScreenAndParams', () => {
    nav.navigate('app-run', { appId: 'a1' })
    nav.navigate('app-run', { appId: 'a1' })

    expect(stackIds()).toEqual(['home', 'app-run'])
  })

  it('should_pushDistinctApp_when_sameScreenDifferentParams', () => {
    nav.navigate('app-run', { appId: 'a1' })
    nav.navigate('app-run', { appId: 'a2' })

    // 同屏不同参是两个应用，必须各自成层
    expect(stackIds()).toEqual(['home', 'app-run', 'app-run'])
    expect(nav.params.value.appId).toBe('a2')
  })

  it('should_popBackToExistingScreen_when_revisitingWithSameParams', () => {
    nav.navigate('app-run', { appId: 'a1' })
    nav.navigate('apps')
    nav.direction.value // 读取不改变状态，仅确认可访问

    nav.navigate('app-run', { appId: 'a1' })

    expect(stackIds()).toEqual(['home', 'app-run'])
    expect(nav.direction.value).toBe('backward')
  })

  it('should_markForward_when_newScreenPushed', () => {
    nav.navigate('apps')

    expect(nav.direction.value).toBe('forward')
  })
})

describe('回退与 Tab 切换', () => {
  it('should_noop_when_backAtRoot', () => {
    nav.back()

    expect(stackIds()).toEqual(['home'])
    expect(nav.canBack.value).toBe(false)
  })

  it('should_popAndMarkBackward_when_backFromDeepStack', () => {
    nav.navigate('app-detail', { appId: 'a1' })
    nav.navigate('permissions')

    nav.back()

    expect(stackIds()).toEqual(['home', 'app-detail'])
    expect(nav.direction.value).toBe('backward')
    expect(nav.canBack.value).toBe(true)
  })

  it('should_resetToSingleScreen_when_switchTab', () => {
    nav.navigate('app-run', { appId: 'a1' })
    nav.navigate('settings')

    nav.switchTab('home')

    expect(stackIds()).toEqual(['home'])
    expect(nav.direction.value).toBe('forward')
  })

  it('should_exitAppsAndOverlays_when_goHome', () => {
    nav.openApp('a1')
    nav.openSwitcher()

    nav.goHome()

    expect(stackIds()).toEqual(['home'])
  })
})

describe('专用入口', () => {
  it('should_openRunScreenWithAppId_when_openApp', () => {
    nav.openApp('demo.app')

    expect(nav.current.value.id).toBe('app-run')
    expect(nav.params.value.appId).toBe('demo.app')
  })

  it('should_openDetailScreenWithAppId_when_openDetail', () => {
    nav.openDetail('demo.app')

    expect(nav.current.value.id).toBe('app-detail')
    expect(nav.params.value.appId).toBe('demo.app')
  })

  it('should_openSwitcherWithoutParams_when_called', () => {
    nav.openSwitcher()

    expect(nav.current.value.id).toBe('switcher')
    expect(nav.params.value.appId).toBeUndefined()
  })

  it('should_openPermissions_when_called', () => {
    nav.openPermissions()

    expect(nav.current.value.id).toBe('permissions')
  })
})