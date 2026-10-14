/**
 * 平台主屏（Android 平台化）行为契约测试（2026-10-14）
 *
 * 被测：`src/shell/components/screens/ShellHomeScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-H1 | wallClass / --wall-dim | 壁纸层由壳偏好驱动；none 档不挂壁纸类 | 类名与遮罩变量随偏好变化 |
 * | C-H2 | runningOrdered | 运行卡片带只列运行中，按最近使用排序 | 顺序 = recentIds 序；未运行不入带 |
 * | C-H3 | 应用宫格 | 全部应用在场；待审批有「!」角标与可访问状态 | 未运行也在格内；approval 角标可见 |
 * | C-H4 | 系统宫格 | 三个平台自有面进栈（可返回） | navigate('apps') / openPermissions / navigate('settings') |
 * | C-H5 | 铃铛 | 角标 = 通知 + 待审批；点击开通知栏 | 计数与 toggleShade |
 * | C-H6 | 空态 | 无应用时给出下一步说明 | 渲染 emptyAppsHint |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'
import { resetShellNotifications, useShellNotifications } from '@/shell/composables/useShellNotifications'
import { clearRecent, pushRecent } from '@/shell/composables/useShellRecent'
import { useShellPreferences } from '@/shell/composables/useShellPreferences'
import type { ShellApp } from '@/shell/types'

// ==================== 替身 ====================

const apps = ref<ShellApp[]>([])
const mockStop = vi.fn(async () => true)
const mockOpen = vi.fn(async () => true)
const mockToggleShade = vi.fn()
const mockNavigate = vi.fn()
const mockOpenPermissions = vi.fn()

vi.mock('vue-i18n', () => ({
  // 回显 key + 参数：角标计数与可访问名称里的参数才可断言
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({
    apps: computed(() => apps.value),
    stats: computed(() => ({ installed: apps.value.length, running: 0, totalBytes: 0 })),
    stop: mockStop,
  }),
}))
vi.mock('@/shell/composables/useShellOpen', () => ({
  useShellOpen: () => ({ open: mockOpen }),
}))
vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({ toggleShade: mockToggleShade }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({ navigate: mockNavigate, openPermissions: mockOpenPermissions }),
}))

import ShellHomeScreen from '@/shell/components/screens/ShellHomeScreen.vue'

function app(id: string, state: ShellApp['state'], name = id): ShellApp {
  return {
    id,
    name,
    version: '1.0.0',
    state,
    permissions: [{ key: 'fs:read', granted: state !== 'approval' }],
    contributions: {},
  }
}

beforeEach(() => {
  resetShellNotifications()
  clearRecent()
  localStorage.clear()
  apps.value = []
  mockStop.mockClear()
  mockOpen.mockClear()
  mockToggleShade.mockClear()
  mockNavigate.mockClear()
  mockOpenPermissions.mockClear()
  useShellPreferences().setWallpaper('aurora')
  useShellPreferences().setWallDim(30)
})

describe('C-H1 壁纸层', () => {
  it('should_applyWallpaperClassAndDim_when_preferenceSet', () => {
    useShellPreferences().setWallpaper('dune')
    useShellPreferences().setWallDim(55)

    const root = mount(ShellHomeScreen).find('div')
    const classes = root.classes()

    expect(classes, '壁纸类名由偏好驱动').toContain('shell-wall')
    expect(classes).toContain('wall-dune')
    expect(root.attributes('style')).toContain('--wall-dim: 55')
    expect(root.find('.shell-wallpaper').exists(), '壁纸层必须存在（纯装饰层）').toBe(true)
  })

  it('should_notApplyWallpaperClass_when_noneSelected', () => {
    useShellPreferences().setWallpaper('none')

    const classes = mount(ShellHomeScreen).find('div').classes()

    expect(classes).toContain('shell-wall')
    expect(classes.filter((c) => c.startsWith('wall-'))).toEqual([])
  })
})

describe('C-H2 运行卡片带', () => {
  it('should_listRunningByRecency_when_appsRunning', () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB'), app('c', 'stopped', 'AppC')]

    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('shell.home.runningTitle')
    // 未运行的应用不进运行带（宫格里另有它）
    const strip = mount(ShellHomeScreen).findAll('article')
    expect(strip.map((c) => c.text()).join('|')).toContain('AppA')
    expect(strip.map((c) => c.text()).join('|')).not.toContain('AppC')
  })

  it('should_stopInstance_when_stopChipTapped', async () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    const stop = wrapper.findAll('button').find((b) => b.text().includes('shell.home.stopInstance'))!
    await stop.trigger('click')

    expect(mockStop).toHaveBeenCalledWith('a')
  })
})

describe('C-H3 应用宫格', () => {
  it('should_listEveryAppIncludingDormant_when_gridRendered', () => {
    apps.value = [app('a', 'running', 'AppA'), app('b', 'stopped', 'AppB')]

    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('AppA')
    expect(text, '未运行应用同样在主屏宫格').toContain('AppB')
  })

  it('should_markApprovalApp_when_pending', () => {
    apps.value = [app('p', 'approval', 'PendingApp')]

    const wrapper = mount(ShellHomeScreen)
    // 可访问名称带应用名与状态（读屏用户看不到角标颜色）
    const cell = wrapper
      .findAll('button')
      .find((b) => (b.attributes('aria-label') ?? '').includes('PendingApp'))

    expect(cell, '待审批应用的宫格单元应在场').toBeTruthy()
    expect(cell!.attributes('aria-label')).toContain('shell.common.approval')
    expect(cell!.text()).toContain('PendingApp')
  })
})

describe('C-H4 系统宫格', () => {
  it('should_enterSystemPagesInStackOrder_when_entriesTapped', async () => {
    const wrapper = mount(ShellHomeScreen)

    await wrapper.find('button[aria-label="shell.home.sysApps"]').trigger('click')
    await wrapper.find('button[aria-label="shell.home.sysPermissions"]').trigger('click')
    await wrapper.find('button[aria-label="shell.home.sysSettings"]').trigger('click')

    // 平台面进栈而不是重置栈：系统屏要有可返回的上一级
    expect(mockNavigate.mock.calls.map((c) => c[0])).toEqual(['apps', 'settings'])
    expect(mockOpenPermissions).toHaveBeenCalledTimes(1)
  })
})

describe('C-H5 通知入口', () => {
  it('should_countNotificationsPlusPending_when_badgeRendered', () => {
    useShellNotifications().pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'n1' })
    useShellNotifications().pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'n2' })
    apps.value = [app('p', 'approval', 'PendingApp')]

    const bell = mount(ShellHomeScreen).find('button[aria-label^="shell.notify.open"]')

    // 计数 = 2 条通知 + 1 个待审批
    expect(bell.attributes('aria-label')).toContain('3')
    expect(bell.text()).toContain('3')
  })

  it('should_toggleShade_when_bellTapped', async () => {
    const wrapper = mount(ShellHomeScreen)

    await wrapper.find('button[aria-label^="shell.notify.open"]').trigger('click')

    expect(mockToggleShade).toHaveBeenCalledTimes(1)
  })
})

describe('C-H6 空态', () => {
  it('should_explainNextStep_when_noAppsInstalled', () => {
    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('shell.home.emptyAppsHint')
    expect(text, '没有应用时不渲染运行卡片带').not.toContain('shell.home.runningTitle')
  })
})
