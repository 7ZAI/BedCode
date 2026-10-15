/**
 * 应用运行页（平台顶栏 + 胶囊）行为契约测试（2026-10-14 二改）
 *
 * 被测：`src/shell/components/screens/ShellAppRunScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-R1 | 应用身份 | 顶栏左侧显示运行中应用的图标与名称 | 名称在场 + 图标 aria-label = 应用名 |
 * | C-R2 | 胶囊菜单 | 菜单键开覆盖层（平台项与应用项在同一张卡里） | overlays.openCapsule(appId) |
 * | C-R3 | 胶囊多任务 | 多任务键进叠放屏（底部系统条退役后的入口） | nav.openSwitcher |
 * | C-R4 | 胶囊回主屏 | home 图标（不是「退出应用」语义）回主屏 | nav.goHome |
 * | C-R5 | 运行面 | 应用运行面在胶囊之下挂载 | ShellAppSurface 在场 |
 * | C-R6 | 无底部导航条 | 平台导航条已退役，本屏不渲染系统条 | 页面无 shell-system-bar |
 * | C-R7 | 清单未落地不出空态 | 壳重挂载后清单要等 refresh，这段窗口 apps 为空 | 标题留空，**不**显示 notFound |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'
import type { ShellApp } from '@/shell/types'

const mockOpenCapsule = vi.fn()
const mockOpenSwitcher = vi.fn()
const mockGoHome = vi.fn()
const params = ref<Record<string, string>>({ appId: 'a' })

const apps = ref<ShellApp[]>([])
/** 清单是否已拉完（useShellApps.listReady 替身） */
const listReady = ref(true)

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({
    getApp: (id: string) => apps.value.find((a) => a.id === id),
    listReady,
  }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({
    params: computed(() => params.value),
    openSwitcher: mockOpenSwitcher,
    goHome: mockGoHome,
  }),
}))
vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({ openCapsule: mockOpenCapsule }),
}))
vi.mock('@/shell/components/ShellAppSurface.vue', () => ({
  default: { name: 'ShellAppSurface', props: ['appId'], template: '<div class="surface" />' },
}))

import ShellAppRunScreen from '@/shell/components/screens/ShellAppRunScreen.vue'

function app(id: string, name: string): ShellApp {
  return {
    id,
    name,
    version: '1.0.0',
    state: 'running',
    permissions: [],
    contributions: {},
  }
}

beforeEach(() => {
  params.value = { appId: 'a' }
  apps.value = [app('a', 'AppA')]
  listReady.value = true
  mockOpenCapsule.mockClear()
  mockOpenSwitcher.mockClear()
  mockGoHome.mockClear()
})

describe('C-R1 应用身份', () => {
  it('should_renderAppIconAndName_when_runningAppShown', () => {
    const wrapper = mount(ShellAppRunScreen)

    expect(wrapper.text()).toContain('AppA')
    expect(wrapper.find('[role="img"][aria-label="AppA"]').exists(), '应用图标应在场').toBe(true)
  })

  it('should_fallBackToNotFound_when_appMissingAfterListReady', () => {
    apps.value = []
    listReady.value = true
    const wrapper = mount(ShellAppRunScreen)

    expect(wrapper.text()).toContain('shell.run.notFound')
  })

  it('should_notShowNotFound_when_appMissingButListStillLoading', () => {
    // 回归（2026-10-16 用户反馈：从插件设置页返回闪「应用不存在」）：
    // 清单未落地（壳先渲染、清单后到的窗口）时不得判空——「还没拉」不是「不存在」
    apps.value = []
    listReady.value = false
    const wrapper = mount(ShellAppRunScreen)

    expect(wrapper.text()).not.toContain('shell.run.notFound')
  })
})

describe('C-R2..C-R4 胶囊（菜单 / 多任务 / 回主屏）', () => {
  it('should_openCapsuleMenu_when_menuTapped', async () => {
    const wrapper = mount(ShellAppRunScreen)

    await wrapper.find('button[aria-label="shell.run.menu"]').trigger('click')

    expect(mockOpenCapsule).toHaveBeenCalledWith('a')
  })

  it('should_openSwitcher_when_recentsTapped', async () => {
    const wrapper = mount(ShellAppRunScreen)

    await wrapper.find('button[aria-label="shell.run.recents"]').trigger('click')

    expect(mockOpenSwitcher).toHaveBeenCalledTimes(1)
  })

  it('should_goHome_when_homeTapped', async () => {
    const wrapper = mount(ShellAppRunScreen)

    await wrapper.find('button[aria-label="shell.run.home"]').trigger('click')

    expect(mockGoHome).toHaveBeenCalledTimes(1)
  })

  it('should_notExposeExitAppSemantics_when_capsuleRendered', () => {
    // 「退出应用」措辞已退役：本屏只回主屏，不停应用（停止在胶囊菜单里显式给出）
    expect(mount(ShellAppRunScreen).text()).not.toContain('shell.run.exitApp')
  })
})

describe('C-R5/C-R6 运行面与导航条', () => {
  it('should_mountSurfaceUnderCapsule_when_rendered', () => {
    const wrapper = mount(ShellAppRunScreen)

    expect(wrapper.find('.surface').exists(), '应用运行面应挂载').toBe(true)
  })

  it('should_notRenderBottomSystemBar_when_rendered', () => {
    // 底部系统条整条退役：本屏不再有常驻导航条，导航只在胶囊里
    const wrapper = mount(ShellAppRunScreen)

    expect(wrapper.find('.shell-system-bar').exists()).toBe(false)
  })
})
