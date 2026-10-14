/**
 * 多任务（Android 卡片式切换）行为契约测试（2026-10-14 Android 平台化）
 *
 * 被测：`src/shell/components/screens/ShellSwitcherScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | cards computed | 运行中应用在前（MRU），最近未运行在后 | 卡片顺序 = 运行中(MRU) → 未运行(recent 序) |
 * | C-S2 | enter | 点卡片进入（未运行的先启动，走统一 openShellApp 口径） | openShellApp(appId) |
 * | C-S3 | dismiss（运行中） | 关闭 = 停用实例 + 移出最近列表 | stop 调用 + 卡片消失 |
 * | C-S4 | dismiss（未运行） | 只移出最近列表，不调 stop | stop 不调用 + 卡片消失 |
 * | C-S5 | closeAll | 停止所有运行中 + 清空最近列表 | stop 每个 + 卡片清空 |
 * | C-S6 | onPointerUp | 上滑超过阈值才关闭；未达阈值是「取消」 | 达阈值 dismiss，未达阈值不 dismiss |
 * | C-S7 | 空态分支 | 无卡片时给出原因与出口 | 渲染空态，不渲染卡片与「全部关闭」 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'
import { clearRecent, pushRecent } from '@/shell/composables/useShellRecent'
import type { ShellApp } from '@/shell/types'

// ==================== 替身 ====================

const apps = ref<ShellApp[]>([])
const mockStop = vi.fn(async () => true)
const mockOpenShellApp = vi.fn(async () => true)
const mockGoHome = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, unknown>) => (params ? `${key}` : key) }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({ apps: computed(() => apps.value), stop: mockStop }),
}))
vi.mock('@/shell/composables/useShellOpen', () => ({
  useShellOpen: () => ({
    open: (...args: unknown[]) => mockOpenShellApp(...(args as [string])),
  }),
  openShellApp: (...args: unknown[]) => mockOpenShellApp(...(args as [string])),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({ goHome: mockGoHome }),
}))

import ShellSwitcherScreen from '@/shell/components/screens/ShellSwitcherScreen.vue'

function app(id: string, state: ShellApp['state'], name = id): ShellApp {
  return {
    id,
    name,
    version: '1.0.0',
    state,
    permissions: [{ key: 'fs:read', granted: true }],
    contributions: {},
  }
}

/** 卡片标题（读卡片头部的应用名，顺序即轨道顺序） */
function cardNames(wrapper: ReturnType<typeof mount>): string[] {
  return wrapper
    .findAll('article')
    .map((card) => card.find('span.block').text())
    .filter((text) => text !== '')
}

beforeEach(() => {
  clearRecent()
  localStorage.clear()
  apps.value = []
  mockStop.mockClear()
  mockOpenShellApp.mockClear()
  mockGoHome.mockClear()
})

describe('C-S1 卡片集合与顺序', () => {
  it('should_putRunningFirstByMru_thenDormantRecents', () => {
    // 最近使用序：b（最新）→ a；运行中只有 a 与 c（c 无 recent 记录 → 排运行组末尾）
    pushRecent('b')
    pushRecent('a')
    apps.value = [
      app('a', 'running', 'AppA'),
      app('c', 'running', 'AppC'),
      app('b', 'stopped', 'AppB'),
      app('d', 'stopped', 'AppD'), // 从未打开：不应出现在卡片轨道
    ]

    const wrapper = mount(ShellSwitcherScreen)

    expect(cardNames(wrapper)).toEqual(['AppA', 'AppC', 'AppB'])
  })

  it('should_showDormantCard_when_appStoppedButRecent', () => {
    pushRecent('a')
    apps.value = [app('a', 'stopped', 'AppA')]

    const wrapper = mount(ShellSwitcherScreen)

    expect(wrapper.text(), '未运行卡片要写明状态').toContain('shell.switcher.dormant')
    expect(wrapper.text()).toContain('shell.switcher.launch')
  })
})

describe('C-S2 进入应用', () => {
  it('should_openThroughUnifiedEntry_when_cardButtonTapped', async () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    const enter = wrapper.findAll('button').find((b) => b.text().includes('shell.switcher.enter'))!
    await enter.trigger('click')

    expect(mockOpenShellApp).toHaveBeenCalledWith('a')
  })
})

describe('C-S3/C-S4 关闭单张卡片', () => {
  it('should_stopAndDropFromRecents_when_runningCardClosed', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await wrapper.find('button[aria-label="shell.switcher.closeCard"]').trigger('click')

    expect(mockStop, '关闭运行中卡片 = 停用实例').toHaveBeenCalledWith('a')
    expect(localStorage.getItem('bedcode.shell.recentApps') ?? '[]', '关闭后不得以未运行身份复活').toBe('[]')
  })

  it('should_onlyDropFromRecents_when_dormantCardClosed', async () => {
    pushRecent('a')
    apps.value = [app('a', 'stopped', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await wrapper.find('button[aria-label="shell.switcher.closeCard"]').trigger('click')

    expect(mockStop, '未运行的应用没有实例可停').not.toHaveBeenCalled()
    expect(localStorage.getItem('bedcode.shell.recentApps') ?? '[]').toBe('[]')
  })
})

describe('C-S5 全部关闭', () => {
  it('should_stopEveryRunningAndClearRecents_when_closeAllTapped', async () => {
    pushRecent('b')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'stopped', 'AppB'), app('c', 'running', 'AppC')]
    const wrapper = mount(ShellSwitcherScreen)

    const closeAll = wrapper.findAll('button').find((b) => b.text().includes('shell.switcher.closeAll'))!
    await closeAll.trigger('click')

    expect(mockStop.mock.calls.map((c) => c[0]).sort()).toEqual(['a', 'c'])
    expect(localStorage.getItem('bedcode.shell.recentApps') ?? '[]').toBe('[]')
  })
})

describe('C-S6 上滑关闭手势', () => {
  it('should_followFinger_when_draggingUp', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)
    const card = wrapper.find('article')

    await card.trigger('pointerdown', { clientY: 200, pointerType: 'touch', button: 0 })
    await card.trigger('pointermove', { clientY: 160 })

    // 拖拽中卡片跟随位移（反馈是手势可发现性的前提，不能「松手才知道生效」）
    expect(card.attributes('style')).toContain('translateY(-40px)')
    expect(mockStop).not.toHaveBeenCalled()
  })

  it('should_dismiss_when_swipeUpBeyondThreshold', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)
    const card = wrapper.find('article')

    await card.trigger('pointerdown', { clientY: 200, pointerType: 'touch', button: 0 })
    await card.trigger('pointerup', { clientY: 100 })

    expect(mockStop, '超过阈值（-100 ≤ -72）应关闭').toHaveBeenCalledWith('a')
  })

  it('should_notDismiss_when_swipeUpBelowThreshold', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)
    const card = wrapper.find('article')

    await card.trigger('pointerdown', { clientY: 200, pointerType: 'touch', button: 0 })
    await card.trigger('pointerup', { clientY: 170 })

    expect(mockStop, '未达阈值（-30 > -72）不得关闭').not.toHaveBeenCalled()
  })

  it('should_treatDownwardDragAsCancel_when_pointerReleasedBelowStart', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)
    const card = wrapper.find('article')

    // 先上滑超过阈值，再下拖到起点以下：以下滑方向为「取消」，不得关闭
    await card.trigger('pointerdown', { clientY: 200, pointerType: 'touch', button: 0 })
    await card.trigger('pointermove', { clientY: 100 })
    await card.trigger('pointerup', { clientY: 260 })

    expect(mockStop).not.toHaveBeenCalled()
  })

  it('should_notDismiss_when_pointerCancelled', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)
    const card = wrapper.find('article')

    await card.trigger('pointerdown', { clientY: 200, pointerType: 'touch', button: 0 })
    await card.trigger('pointercancel')

    expect(mockStop).not.toHaveBeenCalled()
  })
})

describe('C-S7 空态', () => {
  it('should_showEmptyStateWithExit_when_noCards', async () => {
    apps.value = [app('a', 'stopped', 'AppA')] // 未运行且不在最近列表 → 无卡片
    const wrapper = mount(ShellSwitcherScreen)

    expect(wrapper.findAll('article')).toHaveLength(0)
    expect(wrapper.text()).toContain('shell.switcher.empty')
    expect(wrapper.text(), '空态不渲染「全部关闭」').not.toContain('shell.switcher.closeAll')

    const back = wrapper.findAll('button').find((b) => b.text().includes('shell.switcher.backHome'))!
    await back.trigger('click')
    expect(mockGoHome).toHaveBeenCalled()
  })
})
