/**
 * 底部系统条（Android 导航条语义）行为契约测试（2026-10-14 Android 平台化）
 *
 * 被测：`src/shell/components/ShellSystemBar.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-B1 | back 按钮 | 栈底禁用（位置固定，不隐藏） | canBack=false → disabled；true → 调 nav.back |
 * | C-B2 | 主屏键 | 点击回主屏 | nav.goHome |
 * | C-B3 | 多任务键 | 显式入口（与上滑手势等效，WCAG 拖拽替代路径） | nav.openSwitcher |
 * | C-B4 | onPointerUp 手势 | 上滑超过阈值呼出多任务；未达阈值不算手势 | 阈值判定边界 |
 * | C-B5 | 安全区注入 | 底部导航栏占位优先于 bottom | padding-bottom = navigationBar |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'

const mockBack = vi.fn()
const mockGoHome = vi.fn()
const mockOpenSwitcher = vi.fn()
const canBack = ref(false)
const safeArea = ref({ top: 0, bottom: 12, navigationBar: 24 })

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({
    canBack: computed(() => canBack.value),
    back: mockBack,
    goHome: mockGoHome,
    openSwitcher: mockOpenSwitcher,
  }),
}))

import ShellSystemBar from '@/shell/components/ShellSystemBar.vue'

function mountBar() {
  return mount(ShellSystemBar, {
    global: { provide: { safeArea } },
  })
}

beforeEach(() => {
  mockBack.mockClear()
  mockGoHome.mockClear()
  mockOpenSwitcher.mockClear()
  canBack.value = false
})

describe('C-B1 返回键', () => {
  it('should_disableBack_when_stackAtRoot', () => {
    const wrapper = mountBar()
    const back = wrapper.find('button[aria-label="shell.systemBar.back"]')

    expect(back.attributes('disabled')).toBeDefined()
  })

  it('should_callBack_when_stackHasHistory', async () => {
    canBack.value = true
    const wrapper = mountBar()
    const back = wrapper.find('button[aria-label="shell.systemBar.back"]')

    expect(back.attributes('disabled')).toBeUndefined()
    await back.trigger('click')
    expect(mockBack).toHaveBeenCalledTimes(1)
  })
})

describe('C-B2/C-B3 主屏与多任务键', () => {
  it('should_goHome_when_homeKeyTapped', async () => {
    const wrapper = mountBar()

    await wrapper.find('button[aria-label="shell.systemBar.home"]').trigger('click')

    expect(mockGoHome).toHaveBeenCalledTimes(1)
  })

  it('should_openSwitcher_when_recentsKeyTapped', async () => {
    const wrapper = mountBar()

    await wrapper.find('button[aria-label="shell.systemBar.recents"]').trigger('click')

    expect(mockOpenSwitcher).toHaveBeenCalledTimes(1)
  })
})

describe('C-B4 主屏键上滑手势', () => {
  const home = (wrapper: ReturnType<typeof mount>) =>
    wrapper.find('button[aria-label="shell.systemBar.home"]')

  it('should_openSwitcher_when_swipedUpBeyondThreshold', async () => {
    const wrapper = mountBar()

    await home(wrapper).trigger('pointerdown', { clientY: 300 })
    await home(wrapper).trigger('pointerup', { clientY: 260 })

    expect(mockOpenSwitcher, '上滑 40px ≥ 阈值 28px 应呼出多任务').toHaveBeenCalledTimes(1)
  })

  it('should_keepHomeSemantics_when_movementBelowThreshold', async () => {
    const wrapper = mountBar()

    // 手抖 10px：不算手势，随后的 click 仍是「回主屏」
    await home(wrapper).trigger('pointerdown', { clientY: 300 })
    await home(wrapper).trigger('pointerup', { clientY: 290 })
    await home(wrapper).trigger('click')

    expect(mockOpenSwitcher).not.toHaveBeenCalled()
    expect(mockGoHome).toHaveBeenCalledTimes(1)
  })

  it('should_notOpenSwitcher_when_pointerCancelled', async () => {
    const wrapper = mountBar()

    await home(wrapper).trigger('pointerdown', { clientY: 300 })
    await home(wrapper).trigger('pointercancel')
    await home(wrapper).trigger('pointerup', { clientY: 200 })

    expect(mockOpenSwitcher, '取消后不得用旧的起点继续判定').not.toHaveBeenCalled()
  })
})

describe('C-B5 安全区', () => {
  it('should_useNavigationBarInset_when_present', () => {
    const wrapper = mountBar()

    // 底部导航栏占位优先于通用 bottom（Android 三键导航栏比手势条高）
    expect(wrapper.find('nav').attributes('style')).toContain('padding-bottom: 24px')
  })

  it('should_fallBackToBottomInset_when_navigationBarMissing', () => {
    safeArea.value = { top: 0, bottom: 12 }
    const wrapper = mountBar()

    expect(wrapper.find('nav').attributes('style')).toContain('padding-bottom: 12px')
    safeArea.value = { top: 0, bottom: 12, navigationBar: 24 }
  })
})
