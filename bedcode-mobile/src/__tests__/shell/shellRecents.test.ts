/**
 * 多任务（卡片叠放切换）行为契约测试（2026-10-14 Android 平台化 + 二改）
 *
 * 被测：`src/shell/components/screens/ShellSwitcherScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | cards computed | 运行中应用在前（MRU），最近未运行在后 | DOM 顺序 = 运行中(MRU) → 未运行(recent 序) |
 * | C-S2 | enter | 点「进入」先回主屏重置栈，再走统一 openShellApp 口径 | goHome + openShellApp(appId) |
 * | C-S3 | dismiss（运行中） | 关闭 = 停用实例 + 移出最近列表 | stop 调用 + 卡片消失 |
 * | C-S4 | dismiss（未运行） | 只移出最近列表，不调 stop | stop 不调用 + 卡片消失 |
 * | C-S5 | closeAll | 停止所有运行中 + 清空最近列表 | stop 每个 + 卡片清空 |
 * | C-S6 | 上滑关闭 | 轴锁后纵向上滑超阈值才关闭；下滑是取消；pointercancel 不关闭 | 达阈值 dismiss，否则不 dismiss |
 * | C-S7 | 空态 | 无卡片时给出原因与出口 | 空态在场，不渲染卡片与「全部关闭」 |
 * | C-S8 | 叠放与切换 | 前层卡片 aria-current；后层只露边、不含按钮；点后层提到前层 | 前层切换 + 后层零按钮 |
 * | C-S9 | 横向切卡 | 左滑下一张 / 右滑上一张；上一个-下一个按钮等效（拖拽替代路径） | activeIndex 变化 + 边界 disabled |
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
const mockBack = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
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
  useShellNavigation: () => ({ goHome: mockGoHome, back: mockBack }),
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

/** 卡片顺序（DOM 顺序 = 叠放顺序，前层在第一张） */
function cardIds(wrapper: ReturnType<typeof mount>): (string | undefined)[] {
  return wrapper.findAll('article').map((card) => card.attributes('data-app-id'))
}

/** 前层卡片（完整可操作的那张） */
function activeCard(wrapper: ReturnType<typeof mount>) {
  return wrapper.find('article[aria-current="true"]')
}

function activeId(wrapper: ReturnType<typeof mount>): string | undefined {
  return activeCard(wrapper).attributes('data-app-id')
}

/** 以触屏指针手势拖拽叠放区（起点在卡片外，避免命中按钮） */
async function drag(
  wrapper: ReturnType<typeof mount>,
  from: { x: number; y: number },
  to: { x: number; y: number },
  withMove = true,
) {
  const deck = wrapper.find('.shell-deck')
  await deck.trigger('pointerdown', { clientX: from.x, clientY: from.y, pointerType: 'touch', button: 0 })
  if (withMove) {
    // 先越过轴锁阈值，再落到目标位移（首次 move 只用来锁轴）
    await deck.trigger('pointermove', { clientX: (from.x + to.x) / 2, clientY: (from.y + to.y) / 2 })
    await deck.trigger('pointermove', { clientX: to.x, clientY: to.y })
  }
  await deck.trigger('pointerup', { clientX: to.x, clientY: to.y })
}

beforeEach(() => {
  clearRecent()
  localStorage.clear()
  apps.value = []
  mockStop.mockClear()
  mockOpenShellApp.mockClear()
  mockGoHome.mockClear()
  mockBack.mockClear()
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
      app('d', 'stopped', 'AppD'), // 从未打开：不应出现在叠放里
    ]

    const wrapper = mount(ShellSwitcherScreen)

    expect(cardIds(wrapper)).toEqual(['a', 'c', 'b'])
    expect(activeId(wrapper), '前层是最该看到的（MRU 首位）').toBe('a')
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
  it('should_resetStackAndOpenThroughUnifiedEntry_when_enterTapped', async () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    const enter = activeCard(wrapper).findAll('button').find((b) => b.text().includes('shell.switcher.enter'))!
    await enter.trigger('click')

    expect(mockGoHome, '进入前先回主屏，避免叠放屏留在历史里').toHaveBeenCalledTimes(1)
    expect(mockOpenShellApp).toHaveBeenCalledWith('a')
  })
})

describe('C-S3/C-S4 关闭单张卡片', () => {
  it('should_stopAndDropFromRecents_when_runningCardClosed', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await activeCard(wrapper).find('button[aria-label="shell.switcher.closeCard"]').trigger('click')

    expect(mockStop, '关闭运行中卡片 = 停用实例').toHaveBeenCalledWith('a')
    expect(localStorage.getItem('bedcode.shell.recentApps') ?? '[]', '关闭后不得以未运行身份复活').toBe('[]')
  })

  it('should_onlyDropFromRecents_when_dormantCardClosed', async () => {
    pushRecent('a')
    apps.value = [app('a', 'stopped', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await activeCard(wrapper).find('button[aria-label="shell.switcher.closeCard"]').trigger('click')

    expect(mockStop, '未运行的应用没有实例可停').not.toHaveBeenCalled()
    expect(localStorage.getItem('bedcode.shell.recentApps') ?? '[]').toBe('[]')
  })

  it('should_promoteNeighbour_when_activeCardClosed', async () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)
    expect(activeId(wrapper)).toBe('a')

    await activeCard(wrapper).find('button[aria-label="shell.switcher.closeCard"]').trigger('click')
    await wrapper.vm.$nextTick()

    expect(activeId(wrapper), '前层被关掉后邻居补位').toBe('b')
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

    const deck = wrapper.find('.shell-deck')
    await deck.trigger('pointerdown', { clientX: 100, clientY: 200, pointerType: 'touch', button: 0 })
    await deck.trigger('pointermove', { clientX: 102, clientY: 190 })
    await deck.trigger('pointermove', { clientX: 104, clientY: 160 })
    await wrapper.vm.$nextTick()

    // 拖拽中卡片跟随位移（反馈是手势可发现性的前提，不能「松手才知道生效」）
    expect(activeCard(wrapper).attributes('style')).toContain('-40px')
    expect(mockStop).not.toHaveBeenCalled()
  })

  it('should_dismiss_when_swipeUpBeyondThreshold', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 100, y: 300 }, { x: 102, y: 180 })

    expect(mockStop, '超过阈值（-120 ≤ -72）应关闭').toHaveBeenCalledWith('a')
  })

  it('should_notDismiss_when_swipeUpBelowThreshold', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 100, y: 300 }, { x: 101, y: 270 })

    expect(mockStop, '未达阈值（-30 > -72）不得关闭').not.toHaveBeenCalled()
  })

  it('should_treatDownwardReleaseAsCancel_when_swipeBackDown', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    // 上滑超过阈值后又下拖回起点以下：以下滑方向为「取消」，不得关闭
    const deck = wrapper.find('.shell-deck')
    await deck.trigger('pointerdown', { clientX: 100, clientY: 300, pointerType: 'touch', button: 0 })
    await deck.trigger('pointermove', { clientX: 101, clientY: 200 })
    await deck.trigger('pointermove', { clientX: 102, clientY: 360 })
    await deck.trigger('pointerup', { clientX: 102, clientY: 360 })

    expect(mockStop).not.toHaveBeenCalled()
  })

  it('should_notDismiss_when_pointerCancelled', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    const deck = wrapper.find('.shell-deck')
    await deck.trigger('pointerdown', { clientX: 100, clientY: 300, pointerType: 'touch', button: 0 })
    await deck.trigger('pointercancel')
    await deck.trigger('pointerup', { clientX: 100, clientY: 100 })

    expect(mockStop, '取消后不得用旧起点继续判定').not.toHaveBeenCalled()
  })
})

describe('C-S7 空态', () => {
  it('should_showEmptyStateWithExit_when_noCards', () => {
    apps.value = [app('a', 'stopped', 'AppA')] // 未运行且不在最近列表 → 无卡片
    const wrapper = mount(ShellSwitcherScreen)

    expect(wrapper.findAll('article')).toHaveLength(0)
    expect(wrapper.text()).toContain('shell.switcher.empty')
    expect(wrapper.text(), '空态不渲染「全部关闭」').not.toContain('shell.switcher.closeAll')
  })

  it('should_exitThroughHeaderBack_when_noSystemBar', async () => {
    // 底部系统条退役后，返回出口由页头承担（空态不再需要单独一个「回主屏」按钮）
    apps.value = []
    const wrapper = mount(ShellSwitcherScreen)

    await wrapper.find('button[aria-label="shell.switcher.back"]').trigger('click')

    expect(mockBack).toHaveBeenCalledTimes(1)
  })
})

describe('C-S8 叠放与前层切换', () => {
  it('should_markOnlyFrontCardInteractive_when_multipleCards', () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    const cards = wrapper.findAll('article')
    expect(cards).toHaveLength(2)
    expect(cards[0].attributes('aria-current')).toBe('true')
    expect(cards[1].attributes('aria-current')).toBeUndefined()
    // 后层只承担叠放视觉：不含任何按钮（否则键盘焦点与读屏会命中被遮挡的控件）
    expect(cards[1].findAll('button')).toHaveLength(0)
    expect(cards[0].findAll('button').length).toBeGreaterThan(0)
    // 后层从无障碍树摘除（详细内容始终在前层）
    expect(cards[1].attributes('aria-hidden')).toBe('true')
  })

  it('should_promoteBackCard_when_backCardTapped', async () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    await wrapper.findAll('article')[1].trigger('click')

    expect(activeId(wrapper)).toBe('b')
  })

  it('should_applyArithmeticOffsets_when_cardsStacked', () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    const styles = wrapper.findAll('article').map((card) => card.attributes('style') ?? '')
    // 等差叠放：前层在中心（+0px），后层偏移一个步长（20px）且略缩
    expect(styles[0]).toContain('calc(-50% + 0px)')
    expect(styles[0]).toContain('scale(1)')
    expect(styles[1]).toContain('calc(-50% + 20px)')
    expect(styles[1]).toContain('scale(0.96)')
  })
})

describe('C-S9 横向切卡（含拖拽替代路径）', () => {
  function twoCards() {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    return mount(ShellSwitcherScreen)
  }

  it('should_switchToNext_when_swipedLeft', async () => {
    const wrapper = twoCards()

    await drag(wrapper, { x: 200, y: 300 }, { x: 120, y: 300 })

    expect(activeId(wrapper)).toBe('b')
    expect(mockStop, '切卡不是关闭').not.toHaveBeenCalled()
  })

  it('should_switchBack_when_swipedRight', async () => {
    const wrapper = twoCards()
    await drag(wrapper, { x: 200, y: 300 }, { x: 120, y: 300 })
    expect(activeId(wrapper)).toBe('b')

    await drag(wrapper, { x: 120, y: 300 }, { x: 200, y: 300 })

    expect(activeId(wrapper)).toBe('a')
  })

  it('should_stepThroughButtons_when_keyboardOrTap', async () => {
    const wrapper = twoCards()
    const prev = wrapper.find('button[aria-label="shell.switcher.prev"]')
    const next = wrapper.find('button[aria-label="shell.switcher.next"]')

    expect(prev.attributes('disabled'), '第一张时「上一个」不可用').toBeDefined()
    await next.trigger('click')
    expect(activeId(wrapper)).toBe('b')
    expect(next.attributes('disabled'), '最后一张时「下一个」不可用').toBeDefined()

    await prev.trigger('click')
    expect(activeId(wrapper)).toBe('a')
  })
})
