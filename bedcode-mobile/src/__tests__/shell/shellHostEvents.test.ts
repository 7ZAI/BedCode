/**
 * 宿主事件桥 + 状态条 行为契约测试（2026-10-14 Android 平台化）
 *
 * 被测：
 *   · `src/shell/composables/useShellHostEvents.ts`（引擎事件 → 通知栏）
 *   · `src/shell/components/ShellStatusbar.vue`（状态条：平台事实 + 通知入口）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-E1 | ws_connected / ws_reconnected | 连接成功类事件落普通通知 | 各推一条，标题对应 |
 * | C-E2 | ws_disconnected | 断开事件带后端 reason | body = reason（无 reason 时用兜底文案） |
 * | C-E3 | ws_reauth_rejected | 需重新配对 | 驻留项（清除全部不得抹掉） |
 * | C-E4 | on() 失败分支 | 无事件总线（浏览器 / 单测） | 不抛，其余事件照常注册 |
 * | C-E5 | stop() | 壳卸载 | 逐个解绑监听 |
 * | C-B6 | 状态条渲染 | 仅移动端且系统状态栏不在屏上（safeArea.top = 0） | top>0 时不自画（防双状态条） |
 * | C-B7 | 状态条角标 | 计数 = 通知 + 待审批 | 计数与点击开通知栏 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'
import { resetShellNotifications, useShellNotifications } from '@/shell/composables/useShellNotifications'
import type { ShellApp } from '@/shell/types'

// ==================== 替身 ====================

/** listen 注册表：event → handler（测试直接触发） */
const handlers = new Map<string, (event: { payload: unknown }) => void>()
const unlisten = vi.fn()
let listenShouldFail = false

const apps = ref<ShellApp[]>([])
const mockToggleShade = vi.fn()
const platformInfo = ref({ isMobile: true })
const safeArea = ref<{ top: number; bottom: number; navigationBar?: number }>({ top: 0, bottom: 0 })

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    if (listenShouldFail) throw new Error('no event bus')
    handlers.set(event, handler)
    return unlisten
  }),
}))
vi.mock('@/locales', () => ({
  default: {
    global: {
      t: (key: string, params?: Record<string, unknown>) =>
        params ? `${key}:${JSON.stringify(params)}` : key,
    },
  },
}))
vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}))
vi.mock('@/shell/composables/usePlatform', () => ({
  usePlatform: () => ({ platformInfo: computed(() => platformInfo.value) }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({ apps: computed(() => apps.value) }),
}))
vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({ toggleShade: mockToggleShade }),
}))

import { startShellHostEvents } from '@/shell/composables/useShellHostEvents'
import ShellStatusbar from '@/shell/components/ShellStatusbar.vue'

const notifications = useShellNotifications()

function app(id: string, state: ShellApp['state']): ShellApp {
  return { id, name: id, version: '1.0.0', state, permissions: [], contributions: {} }
}

beforeEach(() => {
  handlers.clear()
  unlisten.mockClear()
  listenShouldFail = false
  resetShellNotifications()
  apps.value = []
  mockToggleShade.mockClear()
  platformInfo.value = { isMobile: true }
  safeArea.value = { top: 0, bottom: 0 }
})

describe('C-E1..C-E3 引擎事件 → 通知', () => {
  it('should_pushNotificationPerConnectionEvent_when_eventsArrive', async () => {
    await startShellHostEvents()

    handlers.get('ws_connected')!({ payload: undefined })
    handlers.get('ws_reconnected')!({ payload: undefined })

    const titles = notifications.notifications.value.map((n) => n.title)
    expect(titles.join('|')).toContain('shell.notify.connected.title')
    expect(titles.join('|')).toContain('shell.notify.reconnected.title')
    expect(notifications.notificationCount.value).toBe(2)
  })

  it('should_useBackendReasonAsBody_when_disconnectCarriesReason', async () => {
    await startShellHostEvents()

    handlers.get('ws_disconnected')!({ payload: { reason: 'auth failed' } })

    const entry = notifications.notifications.value[0]
    expect(entry.title).toBe('shell.notify.disconnected.title')
    expect(entry.body, '原因来自后端，原样呈现不加工').toBe('auth failed')
  })

  it('should_fallBackToGenericBody_when_disconnectHasNoReason', async () => {
    await startShellHostEvents()

    handlers.get('ws_disconnected')!({ payload: {} })

    expect(notifications.notifications.value[0].body).toBe('shell.notify.disconnected.body')
  })

  it('should_markReauthRejectedAsSticky_when_eventArrives', async () => {
    await startShellHostEvents()

    handlers.get('ws_reauth_rejected')!({ payload: { reason: 'invalid credential' } })

    const entry = notifications.notifications.value[0]
    expect(entry.sticky, '需重新配对是待处理事项：清除全部不得抹掉').toBe(true)
    expect(entry.body).toBe('invalid credential')

    notifications.clearNotifications()
    expect(notifications.notificationCount.value, '驻留项活过清除全部').toBe(1)
  })

  it('should_registerRemainingEvents_when_singleListenFails', async () => {
    // 只有第一个 listen 失败：后续事件仍要注册（部分失败不能拖垮整条桥）
    const { listen } = await import('@tauri-apps/api/event')
    vi.mocked(listen).mockRejectedValueOnce(new Error('bus down'))

    await expect(startShellHostEvents()).resolves.toBeTypeOf('function')
    expect(handlers.has('ws_reconnected'), '失败之后的事件照常注册').toBe(true)
  })

  it('should_unlistenEveryEvent_when_stopCalled', async () => {
    const stop = await startShellHostEvents()
    expect(handlers.size).toBe(4)

    stop()

    expect(unlisten).toHaveBeenCalledTimes(4)
  })
})

describe('C-B6/C-B7 状态条', () => {
  function mountBar() {
    return mount(ShellStatusbar, { global: { provide: { safeArea } } })
  }

  it('should_renderOwnStatusbar_when_noSystemStatusbarOnScreen', () => {
    const wrapper = mountBar()

    expect(wrapper.find('button').exists(), 'safeArea.top = 0 时壳自画状态条').toBe(true)
  })

  it('should_notRender_when_systemStatusbarOccupiesTop', () => {
    safeArea.value = { top: 24, bottom: 0 }

    const wrapper = mountBar()

    // 实机双状态条（系统一条 + 壳一条）是已修缺陷，必须保持不复发
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('should_countNotificationsPlusPendingAndToggleShade_when_tapped', async () => {
    notifications.pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'n1' })
    apps.value = [app('p', 'approval')]
    const wrapper = mountBar()

    const bar = wrapper.find('button')
    expect(bar.attributes('aria-label'), '计数 = 1 条通知 + 1 个待审批').toContain('"count":2')
    expect(bar.text()).toContain('2')

    await bar.trigger('click')
    expect(mockToggleShade).toHaveBeenCalledTimes(1)
  })

  it('should_hideBadge_when_nothingPending', () => {
    const wrapper = mountBar()

    // 0 条不占位（不显示「0」）：角标只在有内容时出现。
    // 不能拿整条文本断言「不含 0」——状态条上还有 HH:mm 时间（16:06 里就有 0），
    // 断言会随时钟跳红。改为直接找角标元素（唯一带 rounded-full 的 span）。
    expect(wrapper.find('button').attributes('aria-label')).toContain('"count":0')
    const badge = wrapper
      .findAll('span')
      .find((span) => span.classes().includes('rounded-full'))
    expect(badge, '无待处理项时不得渲染角标').toBeUndefined()
  })
})
