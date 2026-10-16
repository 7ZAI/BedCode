/**
 * 通知中心与通知栏 行为契约测试（2026-10-14 Android 平台化）
 *
 * 被测：
 *   · `src/shell/composables/useShellNotifications.ts`（平台通知的存储与生命周期）
 *   · `src/shell/components/ShellNotificationShade.vue`（独立通知面：待处理 + 通知 + 清除）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-N1 | pushNotification | 新条目置顶且 id 唯一 | 列表首项是最新推入，两次推入 id 不同 |
 * | C-N2 | pushNotification | at 缺省取当前时间；sticky 缺省 false | 时间接近 now；非驻留 |
 * | C-N3 | dismissNotification | 按 id 移除 | 目标消失，其余保留 |
 * | C-N4 | dismissNotification | 未知 id | 列表不变（不抛错） |
 * | C-N5 | clearNotifications | 只清非驻留 | 驻留项保留 |
 * | C-N6 | pushNotification 上限 | 超过 50 条丢最旧非驻留 | 长度 = 50 且最旧的已不在 |
 * | C-N7 | 通知栏渲染 | 待处理来自应用状态派生（needsApproval），不是独立生命周期 | 批准（状态变化）后卡片消失 |
 * | C-N8 | 通知栏计数 | 计数 = 通知条数 + 待审批数 | 头部/角标口径一致 |
 * | C-N9 | 通知栏动作 | 单条关闭 / 清除全部 / 去审批 / 打开应用 | 分别落到 dismiss / clear / openDetail / openShellApp |
 * | C-N10 | 滑动删除 | 水平滑超阈值删除（带滑出动画）；短滑回弹；垂直滚动让位；驻留项豁免 | 超阈值→移除；短/垂直/sticky→保留 |
 * | C-N11 | 滑动指示层 | 卡片滑开露出的删除层不用破坏色实心大块（太抢眼），底色由 token 混色派生 | 层挂 .notify-swipe-reveal；shell.css 里该类用 color-mix + --mobile-error |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { mount } from '@vue/test-utils'
import { computed, nextTick, ref } from 'vue'
import {
  resetShellNotifications,
  useShellNotifications,
} from '@/shell/composables/useShellNotifications'
import type { ShellApp } from '@/shell/types'

// ==================== 替身（只替跨进程/第三方边界） ====================

/** 应用清单：由各用例注入（通知栏的待处理分区从它派生） */
const apps = ref<ShellApp[]>([])

const mockCloseShade = vi.fn()
const mockOpenDetail = vi.fn()
const mockOpenShellApp = vi.fn(async () => true)
const shadeOpen = ref(false)

vi.mock('vue-i18n', () => ({
  // 回显 key + 参数：参数断言（条数 / 应用名）才可验证，而不是只断言渲染了某个 key
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}))
// 相对时间走模块级 i18n 实例（组件内非 setup 上下文）：同样回显 key
vi.mock('@/locales', () => ({
  default: {
    global: {
      t: (key: string, params?: Record<string, unknown>) =>
        params ? `${key}:${JSON.stringify(params)}` : key,
    },
  },
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({ apps: computed(() => apps.value) }),
}))
vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({
    shadeOpen: computed(() => shadeOpen.value),
    closeShade: mockCloseShade,
  }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({ openDetail: mockOpenDetail }),
}))
vi.mock('@/shell/composables/useShellOpen', () => ({
  openShellApp: (...args: unknown[]) => mockOpenShellApp(...(args as [string])),
}))

import ShellNotificationShade from '@/shell/components/ShellNotificationShade.vue'

function app(id: string, state: ShellApp['state'], name = id): ShellApp {
  return {
    id,
    name,
    version: '1.0.0',
    state,
    permissions: [
      { key: 'fs:read', granted: state !== 'approval' },
      { key: 'network:http', granted: state !== 'approval' },
    ],
    contributions: {},
  }
}

const notifications = useShellNotifications()

beforeEach(() => {
  resetShellNotifications()
  apps.value = []
  shadeOpen.value = false
  mockCloseShade.mockClear()
  mockOpenDetail.mockClear()
  mockOpenShellApp.mockClear()
})

// ==================== 通知存储 ====================

describe('C-N1..C-N6 平台通知的存储契约', () => {
  it('should_pushNewestFirstWithUniqueIds_when_pushedTwice', () => {
    const first = notifications.pushNotification({
      kind: 'event',
      icon: 'M1 1',
      source: 'src',
      title: 'first',
    })
    const second = notifications.pushNotification({
      kind: 'event',
      icon: 'M1 1',
      source: 'src',
      title: 'second',
    })

    expect(notifications.notifications.value.map((n) => n.title)).toEqual(['second', 'first'])
    expect(first.id).not.toBe(second.id)
  })

  it('should_defaultToNowAndNonSticky_when_pushOmitsThem', () => {
    const before = Date.now()
    const entry = notifications.pushNotification({
      kind: 'info',
      icon: 'M1 1',
      source: 'src',
      title: 't',
    })

    expect(entry.sticky).toBe(false)
    expect(entry.at).toBeGreaterThanOrEqual(before)
    expect(entry.at).toBeLessThanOrEqual(Date.now())
  })

  it('should_removeOnlyTarget_when_dismissed', () => {
    const keep = notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'keep' })
    const drop = notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'drop' })

    notifications.dismissNotification(drop.id)

    expect(notifications.notifications.value.map((n) => n.id)).toEqual([keep.id])
  })

  it('should_keepListIntact_when_dismissingUnknownId', () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'a' })

    notifications.dismissNotification('does-not-exist')

    expect(notifications.notifications.value).toHaveLength(1)
    expect(notifications.notificationCount.value).toBe(1)
  })

  it('should_keepStickyEntries_when_clearingAll', () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'plain' })
    notifications.pushNotification({
      kind: 'event',
      icon: 'i',
      source: 's',
      title: 'needs-action',
      sticky: true,
    })

    notifications.clearNotifications()

    expect(notifications.notifications.value.map((n) => n.title)).toEqual(['needs-action'])
  })

  it('should_dropOldestNonSticky_when_capExceeded', () => {
    // 先放一条驻留项（它必须活过溢出裁剪），再推满 51 条普通项
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'sticky', sticky: true })
    for (let i = 0; i < 51; i++) {
      notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: `n${i}` })
    }

    const titles = notifications.notifications.value.map((n) => n.title)
    expect(notifications.notifications.value).toHaveLength(50)
    expect(titles, '驻留项不得被溢出裁剪丢掉').toContain('sticky')
    expect(titles, '最旧的非驻留项应被丢弃').not.toContain('n0')
    expect(titles, '最新的条目必须在列').toContain('n50')
  })
})

// ==================== 通知栏组件 ====================

function mountShade() {
  return mount(ShellNotificationShade, { global: { stubs: { teleport: true } } })
}

describe('C-N7/C-N8 通知栏内容与计数', () => {
  it('should_renderPendingAndNotifications_when_shadeOpen', () => {
    apps.value = [app('com.test.pending', 'approval', 'PendingApp')]
    notifications.pushNotification({
      kind: 'event',
      icon: 'i',
      source: 'host',
      title: 'Connected',
      body: 'ok',
    })
    shadeOpen.value = true

    const text = mountShade().text()

    expect(text).toContain('shell.notify.pending')
    expect(text, '待审批卡片应显示应用名').toContain('PendingApp')
    expect(text, '待审批正文应带权限项数').toContain('"count":2')
    expect(text).toContain('Connected')
    expect(text, '计数 = 1 条通知 + 1 条待审批').toContain('"count":2')
  })

  it('should_derivePendingFromAppState_when_appLeavesApproval', async () => {
    // 待审批不是独立生命周期：应用状态一变，卡片自然消失（不留尸体）
    apps.value = [app('com.test.pending', 'approval', 'PendingApp')]
    shadeOpen.value = true
    const wrapper = mountShade()
    expect(wrapper.text()).toContain('PendingApp')

    apps.value = [app('com.test.pending', 'running', 'PendingApp')]
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('PendingApp')
    expect(wrapper.text()).toContain('shell.notify.empty')
  })

  it('should_showEmptyState_when_nothingToShow', () => {
    shadeOpen.value = true
    const text = mountShade().text()

    expect(text).toContain('shell.notify.empty')
    expect(text).toContain('shell.notify.emptyHint')
    expect(text, '无内容时不渲染「清除全部」').not.toContain('shell.notify.clearAll')
  })

  it('should_renderNothing_when_shadeClosed', () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'hidden' })
    shadeOpen.value = false

    expect(mountShade().text()).not.toContain('hidden')
  })
})

describe('C-N9 通知栏动作', () => {
  it('should_dismissSingleEntry_when_closeButtonTapped', async () => {
    const entry = notifications.pushNotification({
      kind: 'event',
      icon: 'i',
      source: 's',
      title: 'closable',
    })
    shadeOpen.value = true
    const wrapper = mountShade()

    // aria-label 含 title（带引号），不能用属性选择器拼 CSS：按前缀过滤更稳
    const closeBtn = wrapper
      .findAll('button')
      .find((b) => (b.attributes('aria-label') ?? '').startsWith('shell.notify.dismissOne'))
    expect(closeBtn, '非驻留条目应有单条关闭').toBeTruthy()
    await closeBtn!.trigger('click')

    expect(notifications.notifications.value.map((n) => n.id)).not.toContain(entry.id)
  })

  it('should_notOfferCloseButton_when_entryIsSticky', () => {
    notifications.pushNotification({
      kind: 'event',
      icon: 'i',
      source: 's',
      title: 'sticky',
      sticky: true,
    })
    shadeOpen.value = true

    const wrapper = mountShade()
    expect(wrapper.find('button[aria-label^="shell.notify.dismissOne"]').exists()).toBe(false)
  })

  it('should_clearAllButSticky_when_clearAllTapped', async () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'plain' })
    notifications.pushNotification({ kind: 'event', icon: 'i', source: 's', title: 'sticky', sticky: true })
    shadeOpen.value = true
    const wrapper = mountShade()

    const clearBtn = wrapper
      .findAll('button')
      .find((b) => b.text().includes('shell.notify.clearAll'))!
    expect(clearBtn, '有非驻留条目时应提供「清除全部」').toBeTruthy()
    await clearBtn.trigger('click')

    // 2026-10-15：清除全部改为级联飞离动画（等差逐张右飞）——点击后非立即清空，
    // 末张飞完（360ms + 等差步长）才真正落库；动画期间非驻留卡仍在
    expect(
      notifications.notifications.value.map((n) => n.title),
      '级联动画未结束前不立即清空',
    ).toContain('plain')
    await vi.waitFor(
      () => {
        expect(notifications.notifications.value.map((n) => n.title)).toEqual(['sticky'])
      },
      { timeout: 2000, interval: 50 },
    )
    wrapper.unmount()
  })

  it('should_openDetailAndCloseShade_when_pendingCardTapped', async () => {
    apps.value = [app('com.test.pending', 'approval', 'PendingApp')]
    shadeOpen.value = true
    const wrapper = mountShade()

    const review = wrapper.findAll('button').find((b) => b.text().includes('shell.notify.pendingAction'))!
    await review.trigger('click')

    expect(mockCloseShade, '去审批要先收起通知栏').toHaveBeenCalled()
    expect(mockOpenDetail).toHaveBeenCalledWith('com.test.pending')
  })

  it('should_openAppAndCloseShade_when_notificationHasAppTarget', async () => {
    notifications.pushNotification({
      kind: 'event',
      icon: 'i',
      source: 's',
      title: 'from-app',
      appId: 'com.test.app',
    })
    shadeOpen.value = true
    const wrapper = mountShade()

    // 2026-10-15：去掉了独立「打开」按钮——整卡可点即打开（有关联应用时）
    const card = wrapper
      .findAll('article')
      .find((a) => a.text().includes('from-app'))!
    expect(card, '带关联应用的通知卡应在场').toBeTruthy()
    expect(card.attributes('role'), '可打开的卡片应暴露 button 语义').toBe('button')
    await card.trigger('click')

    expect(mockCloseShade).toHaveBeenCalled()
    expect(mockOpenShellApp).toHaveBeenCalledWith('com.test.app')
  })
})

// ==================== 滑动删除 ====================

describe('C-N10 左右滑动删除', () => {
  /** 找指定标题的通知卡 */
  function cardOf(wrapper: ReturnType<typeof mount>, title: string) {
    return wrapper.findAll('article').find((a) => a.text().includes(title))!
  }

  /**
   * 模拟一次拖动：pointerdown → 两次 move（首帧定轴，次帧到终点） → pointerup
   * 传入相对位移 dx/dy（屏幕坐标 delta）
   */
  async function swipe(
    wrapper: ReturnType<typeof mount>,
    title: string,
    dx: number,
    dy = 0,
  ): Promise<void> {
    const card = cardOf(wrapper, title)!
    await card.trigger('pointerdown', { clientX: 100, clientY: 100, pointerId: 1 })
    await card.trigger('pointermove', {
      clientX: 100 + dx / 2,
      clientY: 100 + dy / 2,
      pointerId: 1,
    })
    await card.trigger('pointermove', {
      clientX: 100 + dx,
      clientY: 100 + dy,
      pointerId: 1,
    })
    await card.trigger('pointerup', { clientX: 100 + dx, clientY: 100 + dy, pointerId: 1 })
  }

  it('should_dismissAfterSwipeOut_when_horizontalSwipePassesThreshold', async () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'swipe-me' })
    shadeOpen.value = true
    const wrapper = mountShade()

    await swipe(wrapper, 'swipe-me', 150, 0)

    // 松手瞬间：滑出动画未结束，卡片仍在、通知未删（swipeOut 位移定格屏外）
    expect(cardOf(wrapper, 'swipe-me').exists(), '滑出动画期间卡片暂留').toBe(true)
    expect(notifications.notifications.value.length, '动画期间存储未删').toBe(1)
    // 220ms 滑出动画结束后删除（真实等待，避免 fake timers 与 vtu flush 纠缠）
    await vi.waitFor(
      () => {
        expect(notifications.notifications.value.length, '超阈值滑动后通知应被删除').toBe(0)
      },
      { timeout: 800, interval: 50 },
    )
    await nextTick()
    const remains = wrapper.findAll('article').some((a) => a.text().includes('swipe-me'))
    expect(remains, 'DOM 同步移除').toBe(false)
    wrapper.unmount()
  })

  it('should_snapBack_when_swipeBelowThreshold', async () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'stay' })
    shadeOpen.value = true
    const wrapper = mountShade()

    await swipe(wrapper, 'stay', 30, 0)

    expect(cardOf(wrapper, 'stay').exists(), '短滑应回弹不删除').toBe(true)
    expect(notifications.notifications.value.length).toBe(1)
    wrapper.unmount()
  })

  it('should_notDrag_when_gestureIsVertical', async () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'v-swip' })
    shadeOpen.value = true
    const wrapper = mountShade()

    await swipe(wrapper, 'v-swip', 10, 120)

    expect(cardOf(wrapper, 'v-swip').exists(), '垂直滚动应让位列表滚动，不删除').toBe(true)
    expect(notifications.notifications.value.length).toBe(1)
    wrapper.unmount()
  })

  it('should_useTokenDerivedReveal_when_swipeLayerRendered', () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'layer' })
    shadeOpen.value = true
    const wrapper = mountShade()

    // ① 结构：滑动露出的是专用的指示层类（颜色不在组件里写死）
    const layer = wrapper.find('.notify-swipe-reveal')
    expect(layer.exists(), '滑开时露出的删除指示层在场').toBe(true)

    // ② 颜色：该类在 shell.css 里由 --mobile-error 低比例混进浮起底色得出，
    //    不是破坏色实心铺底——实心大红色是用户实测反馈的「太突兀」
    const css = readFileSync(join(process.cwd(), 'src/shell/styles/shell.css'), 'utf-8')
    const block = css.slice(css.indexOf('.notify-swipe-reveal'))
    const rule = block.slice(0, block.indexOf('}'))
    expect(rule, '指示层底色由 color-mix 派生').toContain('color-mix(in srgb, var(--mobile-error)')
    expect(rule, '不得退回破坏色实心底').not.toMatch(/background:\s*var\(--mobile-error\)/)

    wrapper.unmount()
  })

  it('should_ignoreSwipe_when_entryIsSticky', async () => {
    notifications.pushNotification({
      kind: 'info',
      icon: 'i',
      source: 's',
      title: 'pinned',
      sticky: true,
    })
    shadeOpen.value = true
    const wrapper = mountShade()

    await swipe(wrapper, 'pinned', 200, 0)

    expect(cardOf(wrapper, 'pinned').exists(), '驻留项不响应滑动（同 X 语义）').toBe(true)
    expect(notifications.notifications.value.length).toBe(1)
    wrapper.unmount()
  })
})
