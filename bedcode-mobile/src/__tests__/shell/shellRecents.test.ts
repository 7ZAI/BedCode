/**
 * 多任务（卡片轨道切换）行为契约测试（2026-10-14 Android 平台化 + 二改 + 三改）
 *
 * 被测：`src/shell/components/screens/ShellSwitcherScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | cards computed | 运行中应用在前（MRU），最近未运行在后 | DOM 顺序 = 运行中(MRU) → 未运行(recent 序) |
 * | C-S2 | enter | 点整张卡片先回主屏重置栈，再走统一 openShellApp 口径 | goHome + openShellApp(appId) |
 * | C-S3 | dismiss（运行中） | 关闭 = 停用实例 + 移出最近列表 | stop 调用 + 卡片消失 |
 * | C-S4 | dismiss（未运行） | 只移出最近列表，不调 stop | stop 不调用 + 卡片消失 |
 * | C-S5 | closeAll | 停止所有运行中 + 清空最近列表 | stop 每个 + 卡片清空 |
 * | C-S6 | 上滑关闭 | 轴锁后纵向上滑超阈值才关闭；下滑是取消；pointercancel 不关闭 | 达阈值 dismiss，否则不 dismiss |
 * | C-S7 | 空态 | 无卡片时给出原因与出口（页头返回 + 主动作「回到主屏」） | 空态在场，不渲染卡片与「全部关闭」 |
 * | C-S8 | 卡片轨道 | 卡片**并排不叠放**：轨道按「序号 × 步长」平移、样式表里不再 absolute 叠压 | 轨道 transform + 样式表锁 |
 * | C-S9 | 横向切卡 | 左滑下一张 / 右滑上一张；上一个-下一个按钮等效（拖拽替代路径） | activeIndex 变化 + 边界 disabled |
 * | C-S10 | 空态主动作 | 空态给一条出路，不把用户堵在只有页头返回键的空屏 | 点「回到主屏」→ goHome |
 * | C-S11 | 触控目标 | 当前卡片 × 的命中区 ≥44px（ui-ux-pro-max Touch Target Size / MOBILE.md） | 挂 min-w/min-h token，不再是 36px 定宽 |
 * | C-S12 | 位置播报 | 圆点是纯视觉，读屏靠「第 N 张，共 M 张」 | 切卡后 status 文本随之变化 |
 * | C-S13 | 手势让位 | 贴屏幕左右边缘起手不接管（不与系统返回手势抢同一条边） | 贴边横滑不切卡，中部起手照常切卡 |
 * | C-S14 | 指针捕获 | 确认是拖拽后才捕获指针：快滑出界仍能收尾；纯点击不捕获（click 留在卡片上） | 拖拽 setPointerCapture + 抬起释放；点击不捕获 |
 * | C-S15 | 整卡点击 | 卡片整块可点 = 进入 / 启动；卡片上不再有独立「进入」按钮 | 点卡片 goHome + open；无旧按钮文案 |
 * | C-S16 | 拖拽后不误触进入 | 拖拽抬手后浏览器补的 click 不得触发进入 | 上滑关闭后 mockOpenShellApp 未被调用 |
 * | C-S17 | 关闭按钮独占手势 | × 标 data-deck-ignore-drag，从它起手不算轨道手势 | × 带标记；整卡按钮不带 |
 * | C-S18 | 并排可点 | 并排后每张卡片都在无障碍树里、都能直接进入（不再有「后层只露边」） | 每张卡有进入按钮、无 aria-hidden |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
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
  useI18n: () => ({
    // 位置播报是唯一带参数且需要断言取值的 key（其余 key 原样回显，
    // 免得 aria-label 选择器带上参数后缀而失效）
    t: (key: string, params?: Record<string, unknown>) =>
      key === 'shell.switcher.position' && params
        ? `${String(params.index)}/${String(params.total)}`
        : key,
  }),
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

/** 屏宽固定值：贴边让位带（EDGE_GUARD）按屏宽判定，测试需要确定的视口 */
const VIEWPORT_WIDTH = 400

/** 指针捕获替身：happy-dom 不实现该 API（组件侧有降级分支），这里装上以验证捕获时机 */
const pointerCapture = { set: vi.fn(), release: vi.fn() }

/** 在原型上装 / 卸指针捕获替身（严格模式：未预期调用也应能被断言） */
function installPointerCapture(): void {
  pointerCapture.set = vi.fn()
  pointerCapture.release = vi.fn()
  for (const [name, impl] of [
    ['setPointerCapture', pointerCapture.set],
    ['releasePointerCapture', pointerCapture.release],
  ] as const) {
    Object.defineProperty(Element.prototype, name, { configurable: true, writable: true, value: impl })
  }
}

function uninstallPointerCapture(): void {
  for (const name of ['setPointerCapture', 'releasePointerCapture']) {
    delete (Element.prototype as unknown as Record<string, unknown>)[name]
  }
}

beforeEach(() => {
  clearRecent()
  localStorage.clear()
  apps.value = []
  mockStop.mockClear()
  mockOpenShellApp.mockClear()
  mockGoHome.mockClear()
  mockBack.mockClear()
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: VIEWPORT_WIDTH })
})

afterEach(() => {
  uninstallPointerCapture()
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
    // 未运行卡片的点击语义是「启动」而不是「进入」，靠可访问名区分
    expect(wrapper.find('button[aria-label="shell.switcher.launchApp"]').exists()).toBe(true)
  })
})

describe('C-S2/C-S15 进入应用（整卡点击）', () => {
  it('should_resetStackAndOpenThroughUnifiedEntry_when_cardTapped', async () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await activeCard(wrapper).find('button[aria-label="shell.switcher.enterApp"]').trigger('click')

    expect(mockGoHome, '进入前先回主屏，避免叠放屏留在历史里').toHaveBeenCalledTimes(1)
    expect(mockOpenShellApp).toHaveBeenCalledWith('a')
  })

  it('should_notRenderStandaloneEnterButton_when_frontCardRendered', () => {
    // 交互对齐 Android recents：点卡片进入，不再叠一个「进入」按钮
    apps.value = [app('a', 'running', 'AppA')]
    const card = activeCard(mount(ShellSwitcherScreen))

    expect(card.findAll('button'), '前层卡片只有「整卡进入」与「× 关闭」两个控件').toHaveLength(2)
    expect(card.text(), '不再渲染独立的「进入 / 启动」按钮文案').not.toContain('shell.switcher.enter')
    expect(card.text()).not.toContain('shell.switcher.launch')
  })

  it('should_coverWholeCardWithTappableTarget_when_frontCardRendered', () => {
    apps.value = [app('a', 'running', 'AppA')]

    const card = activeCard(mount(ShellSwitcherScreen))

    // 整卡按钮是卡片的直接子元素且铺满（flex-1），点卡片任意位置都算进入
    const tapTarget = card.find('button[aria-label="shell.switcher.enterApp"]')
    expect(tapTarget.classes()).toContain('flex-1')
    expect(tapTarget.element.parentElement).toBe(card.element)
    // 触屏上必须给按压反馈（hover-only 等于没有反馈）
    expect(tapTarget.classes(), '整卡点击要有按压反馈').toContain('active:opacity-80')
  })
})

describe('C-S16 拖拽后不误触进入', () => {
  it('should_notEnter_when_swipeUpDismissEndsWithClick', async () => {
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 200, y: 300 }, { x: 200, y: 150 })
    // 抬手后浏览器补的那次 click 落在整卡按钮上：必须被吞掉，否则上滑关闭顺手打开了应用
    await activeCard(wrapper).find('button[aria-label="shell.switcher.enterApp"]').trigger('click')

    expect(mockStop, '上滑超阈值应关闭').toHaveBeenCalledWith('a')
    expect(mockOpenShellApp, '拖拽余波 click 不得进入').not.toHaveBeenCalled()
  })

  it('should_switchWithoutEntering_when_swipedHorizontally', async () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 200, y: 300 }, { x: 120, y: 300 })
    await activeCard(wrapper).find('button[aria-label="shell.switcher.enterApp"]').trigger('click')

    expect(activeId(wrapper), '横滑只切卡').toBe('b')
    expect(mockOpenShellApp, '拖拽余波 click 不得进入').not.toHaveBeenCalled()
  })
})

describe('C-S17 关闭按钮独占手势', () => {
  it('should_markCloseButtonAsDragExempt_when_frontCardRendered', () => {
    apps.value = [app('a', 'running', 'AppA')]

    const card = activeCard(mount(ShellSwitcherScreen))

    // 整卡是「进入」按钮，从它上面起手仍要能上滑关闭；只有 × 排除在拖拽之外
    expect(card.find('button[aria-label="shell.switcher.closeCard"]').attributes('data-deck-ignore-drag'))
      .toBe('')
    expect(card.find('button[aria-label="shell.switcher.enterApp"]').attributes('data-deck-ignore-drag'))
      .toBeUndefined()
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
    // 底部系统条退役后，返回出口由页头承担（空态另有主动作，见 C-S10）
    apps.value = []
    const wrapper = mount(ShellSwitcherScreen)

    await wrapper.find('button[aria-label="shell.switcher.back"]').trigger('click')

    expect(mockBack).toHaveBeenCalledTimes(1)
  })
})

describe('C-S10 空态主动作', () => {
  it('should_goHome_when_emptyStateActionTapped', async () => {
    // 空态只留文案没有动作 = 用户被堵在空屏里（ui-ux-pro-max Empty States：文案 + 动作）
    apps.value = []
    const wrapper = mount(ShellSwitcherScreen)

    const action = wrapper.findAll('button').find((b) => b.text().includes('shell.switcher.goHome'))
    expect(action, '空态要有主动作').toBeTruthy()

    await action!.trigger('click')

    expect(mockGoHome).toHaveBeenCalledTimes(1)
  })

  it('should_notRenderEmptyAction_when_cardsExist', () => {
    apps.value = [app('a', 'running', 'AppA')]
    const text = mount(ShellSwitcherScreen).text()

    expect(text, '有卡片时不渲染空态动作').not.toContain('shell.switcher.goHome')
  })
})

describe('C-S11 触控目标（× 关闭）', () => {
  it('should_keepCloseButtonHitAreaAtLeast44px_when_frontCardRendered', () => {
    apps.value = [app('a', 'running', 'AppA')]

    const close = activeCard(mount(ShellSwitcherScreen)).find(
      'button[aria-label="shell.switcher.closeCard"]',
    )
    const classes = close.classes()

    expect(classes, '命中区按 44px 最小触控目标给').toContain('min-h-[var(--mobile-touch-target-min)]')
    expect(classes).toContain('min-w-[var(--mobile-touch-target-min)]')
    expect(classes.join(' '), '不得退回 36px 定宽').not.toContain('w-9')
  })
})

describe('C-S12 位置播报（读屏）', () => {
  it('should_announceFrontCardPosition_when_cardsStacked', async () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    expect(wrapper.find('[role="status"]').text(), '圆点是纯视觉，位置靠这句').toBe('1/2')

    await wrapper.find('button[aria-label="shell.switcher.next"]').trigger('click')

    expect(wrapper.find('[role="status"]').text(), '切卡后位置播报要跟着变').toBe('2/2')
  })
})

describe('C-S13 手势让位（不与系统返回手势抢边）', () => {
  function twoCards() {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    return mount(ShellSwitcherScreen)
  }

  it('should_ignoreSwipe_when_startedAtLeftEdge', async () => {
    const wrapper = twoCards()

    await drag(wrapper, { x: 8, y: 300 }, { x: 80, y: 300 })

    expect(activeId(wrapper), '贴边起手交给系统返回手势，叠放不切卡').toBe('a')
  })

  it('should_ignoreSwipe_when_startedAtRightEdge', async () => {
    const wrapper = twoCards()

    await drag(wrapper, { x: VIEWPORT_WIDTH - 8, y: 300 }, { x: VIEWPORT_WIDTH - 80, y: 300 })

    expect(activeId(wrapper)).toBe('a')
  })

  it('should_switchCard_when_startedInsideGuard', async () => {
    // 反例护栏：让位带之外的同一手势必须照常切卡，否则手势整体失效
    const wrapper = twoCards()

    await drag(wrapper, { x: 150, y: 300 }, { x: 80, y: 300 })

    expect(activeId(wrapper)).toBe('b')
  })
})

describe('C-S14 指针捕获（拖拽收尾）', () => {
  it('should_captureAndReleasePointer_when_dragPassesAxisLock', async () => {
    installPointerCapture()
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 200, y: 300 }, { x: 120, y: 300 })

    // 捕获后手指滑出叠放区仍能收到 move/up，否则卡片会停在偏移位
    expect(pointerCapture.set).toHaveBeenCalledTimes(1)
    expect(pointerCapture.release, '抬起即释放').toHaveBeenCalledTimes(1)
  })

  it('should_notCapturePointer_when_tapWithoutMove', async () => {
    // 纯点击不捕获：捕获会把 click 改派到轨道容器，卡片上的点击就落空了
    installPointerCapture()
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    await drag(wrapper, { x: 200, y: 300 }, { x: 202, y: 302 }, false)

    expect(pointerCapture.set).not.toHaveBeenCalled()
    await wrapper.findAll('article')[1].find('button[aria-label="shell.switcher.enterApp"]').trigger('click')
    expect(mockOpenShellApp, '未拖拽的点击照常进入').toHaveBeenCalledWith('b')
  })
})

describe('C-S8 卡片轨道（不叠放）', () => {
  function twoCards() {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    return mount(ShellSwitcherScreen)
  }

  it('should_translateTrackByOneStepPerCard_when_activeCardChanges', async () => {
    const wrapper = twoCards()

    // 轨道平移 = 序号 × 步长（步长真源在 shell.css：卡宽 + 间距），卡片自身不再偏移
    expect(wrapper.find('.shell-deck-track').attributes('style')).toContain(
      'translate3d(calc(-1 * var(--deck-step) * 0 + 0px), 0, 0)',
    )

    await wrapper.find('button[aria-label="shell.switcher.next"]').trigger('click')

    expect(wrapper.find('.shell-deck-track').attributes('style')).toContain(
      'translate3d(calc(-1 * var(--deck-step) * 1 + 0px), 0, 0)',
    )
  })

  it('should_followFingerWhenDraggingTrack_when_swipedHorizontally', async () => {
    const wrapper = twoCards()

    // 拖拽中跟手（抬起才吸附），因此断言放在抬手之前
    const deck = wrapper.find('.shell-deck')
    await deck.trigger('pointerdown', { clientX: 200, clientY: 300, pointerType: 'touch', button: 0 })
    await deck.trigger('pointermove', { clientX: 175, clientY: 300 })
    await deck.trigger('pointermove', { clientX: 150, clientY: 300 })

    expect(wrapper.find('.shell-deck-track').attributes('style')).toContain('-50px')
  })

  it('should_notStackCards_when_cardsLaidOut', () => {
    // 样式表锁：卡片回到常规流并排（position: absolute 叠压已退役），
    // 轨道是 flex 行 + 等量两端留白，屏宽不够时只会被裁切而不会互相叠压
    const css = readFileSync(join(process.cwd(), 'src/shell/styles/shell.css'), 'utf8')
    const cardBlock = css.match(/\.shell-deck-card\s*\{[^}]*\}/)?.[0] ?? ''

    expect(cardBlock, '卡片不再 absolute 叠放').not.toContain('position: absolute')
    expect(cardBlock, '卡片按轨道宽度排布').toContain('flex: 0 0 var(--deck-card-w)')
    expect(css).toContain('.shell-deck-track')
  })

  it('should_keepCurrentCardMarked_when_cardsLaidOut', () => {
    const wrapper = twoCards()

    const cards = wrapper.findAll('article')
    expect(cards).toHaveLength(2)
    expect(cards[0].attributes('aria-current'), '当前卡片在轨道中间').toBe('true')
    expect(cards[1].attributes('aria-current')).toBeUndefined()
  })
})

describe('C-S18 并排后每张卡片都可进入', () => {
  it('should_exposeEveryCardToScreenReaders_when_multipleCards', () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    const cards = wrapper.findAll('article')
    // 并排布局没有「后层只露边」这回事：每张都在无障碍树里，各带一个进入按钮
    expect(cards.map((c) => c.attributes('aria-hidden'))).toEqual([undefined, undefined])
    expect(cards[0].find('button[aria-label="shell.switcher.enterApp"]').exists()).toBe(true)
    expect(cards[1].find('button[aria-label="shell.switcher.enterApp"]').exists()).toBe(true)
    // × 只挂在当前卡片上（焦点卡才有清除键）
    expect(cards[0].find('button[aria-label="shell.switcher.closeCard"]').exists()).toBe(true)
    expect(cards[1].find('button[aria-label="shell.switcher.closeCard"]').exists()).toBe(false)
  })

  it('should_enterTappedApp_when_nonActiveCardTapped', async () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB')]
    const wrapper = mount(ShellSwitcherScreen)

    // 点哪张开哪张（Android recents）：不需要先把它滑到中间
    await wrapper.findAll('article')[1].find('button[aria-label="shell.switcher.enterApp"]').trigger('click')

    expect(mockGoHome).toHaveBeenCalledTimes(1)
    expect(mockOpenShellApp).toHaveBeenCalledWith('b')
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
