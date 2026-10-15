/**
 * 壳系统返回键兜底 行为契约测试
 *
 * 被测：`src/shell/composables/useShellSystemBack.ts`（平台兜底：覆盖层 → 屏栈回退 → 回主屏）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | `hasBackTarget` 计算 | 主屏且无覆盖层 | false（不接管系统返回 = 保留「主屏返回退出应用」） |
 * | C-S2 | 同上 | 覆盖层开 / 屏栈可回退 / 非主屏 | true |
 * | C-S3 | `handleSystemBack` 分支顺序 | 通知栏开着 | closeShade，返回 true，不动屏栈与其它覆盖层 |
 * | C-S4 | 同上 | 胶囊菜单开着 | closeCapsule，返回 true |
 * | C-S5 | 同上 | 授权弹窗开着 | closePermissionRequest，返回 true |
 * | C-S6 | 同上 | 无覆盖层且屏栈可回退 | nav.back()，返回 true（不走 goHome） |
 * | C-S7 | 同上 | 非主屏的 Tab 根（不可回退） | nav.goHome()，返回 true |
 * | C-S8 | 同上 | 主屏且无覆盖层 | 返回 false（不消费 → 系统默认行为生效） |
 * | C-S9 | `watch(hasBackTarget)` | 目标出现/消失 | 订阅 / 退订平台返回通道 |
 * | C-S10 | `dispose` | 壳卸载后 | 已退订且不再随状态变化重新订阅 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { computed, nextTick, ref } from 'vue'

// 平台返回通道替身：记录处理器 + 退订计数（跨进程边界按契约替身）
const backChannel = vi.hoisted(() => ({
  handlers: [] as Array<() => boolean>,
  dispose: vi.fn(),
}))

const current = ref<{ id: string }>({ id: 'home' })
const canBack = ref(false)
const shadeOpen = ref(false)
const capsuleAppId = ref<string | null>(null)
const permissionRequest = ref<{ appId: string } | null>(null)

const navBack = vi.fn(() => {
  canBack.value = false
})
const goHome = vi.fn()
const closeShade = vi.fn(() => {
  shadeOpen.value = false
})
const closeCapsule = vi.fn(() => {
  capsuleAppId.value = null
})
const closePermissionRequest = vi.fn(() => {
  permissionRequest.value = null
})

vi.mock('@/plugin/back-button', () => ({
  subscribeHostBackPressed: (handler: () => boolean) => {
    backChannel.handlers.push(handler)
    return () => {
      const index = backChannel.handlers.indexOf(handler)
      if (index >= 0) backChannel.handlers.splice(index, 1)
      backChannel.dispose()
    }
  },
}))

vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({
    current: computed(() => current.value),
    canBack: computed(() => canBack.value),
    back: navBack,
    goHome,
  }),
}))

vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({
    shadeOpen: computed(() => shadeOpen.value),
    capsuleAppId: computed(() => capsuleAppId.value),
    permissionRequest: computed(() => permissionRequest.value),
    closeShade,
    closeCapsule,
    closePermissionRequest,
  }),
}))

import { useShellSystemBack } from '@/shell/composables/useShellSystemBack'

/** 本次用例的兜底实例（afterEach 统一 dispose，避免跨用例串味） */
let systemBack: ReturnType<typeof useShellSystemBack> | null = null

function setup(): ReturnType<typeof useShellSystemBack> {
  systemBack = useShellSystemBack()
  return systemBack
}

beforeEach(() => {
  current.value = { id: 'home' }
  canBack.value = false
  shadeOpen.value = false
  capsuleAppId.value = null
  permissionRequest.value = null
  backChannel.handlers.length = 0
  backChannel.dispose.mockClear()
  navBack.mockClear()
  goHome.mockClear()
  closeShade.mockClear()
  closeCapsule.mockClear()
  closePermissionRequest.mockClear()
})

afterEach(() => {
  systemBack?.dispose()
  systemBack = null
})

describe('C-S1/C-S2 是否接管系统返回', () => {
  it('should_notTakeOver_when_homeAndNoOverlay', () => {
    const back = setup()
    expect(back.hasBackTarget.value).toBe(false)
    expect(backChannel.handlers).toHaveLength(0)
  })

  it('should_takeOver_when_screenStackCanGoBack', () => {
    canBack.value = true
    expect(setup().hasBackTarget.value).toBe(true)
  })

  it('should_takeOver_when_currentScreenIsNotHome', () => {
    current.value = { id: 'settings' }
    expect(setup().hasBackTarget.value).toBe(true)
  })

  it('should_takeOver_when_overlayOpen', () => {
    shadeOpen.value = true
    expect(setup().hasBackTarget.value).toBe(true)
  })
})

describe('C-S3/C-S4/C-S5 覆盖层优先关闭', () => {
  it('should_closeShadeFirst_when_shadeAndCapsuleBothOpen', () => {
    shadeOpen.value = true
    capsuleAppId.value = 'com.test.app'
    canBack.value = true

    const consumed = setup().handleSystemBack()

    expect(consumed).toBe(true)
    expect(closeShade).toHaveBeenCalledTimes(1)
    expect(closeCapsule).not.toHaveBeenCalled()
    expect(navBack).not.toHaveBeenCalled()
  })

  it('should_closeCapsule_when_onlyCapsuleOpen', () => {
    capsuleAppId.value = 'com.test.app'

    const consumed = setup().handleSystemBack()

    expect(consumed).toBe(true)
    expect(closeCapsule).toHaveBeenCalledTimes(1)
    expect(closeShade).not.toHaveBeenCalled()
    expect(closePermissionRequest).not.toHaveBeenCalled()
  })

  it('should_closePermissionPrompt_when_onlyPromptOpen', () => {
    permissionRequest.value = { appId: 'com.test.app' }

    const consumed = setup().handleSystemBack()

    expect(consumed).toBe(true)
    expect(closePermissionRequest).toHaveBeenCalledTimes(1)
    expect(goHome).not.toHaveBeenCalled()
  })
})

describe('C-S6/C-S7/C-S8 屏栈与主屏', () => {
  it('should_popScreenStack_when_canGoBack', () => {
    canBack.value = true

    const consumed = setup().handleSystemBack()

    expect(consumed).toBe(true)
    expect(navBack).toHaveBeenCalledTimes(1)
    expect(goHome).not.toHaveBeenCalled()
  })

  it('should_goHome_when_tabRootWithoutStackHistory', () => {
    current.value = { id: 'settings' }

    const consumed = setup().handleSystemBack()

    expect(consumed).toBe(true)
    expect(goHome).toHaveBeenCalledTimes(1)
    expect(navBack).not.toHaveBeenCalled()
  })

  it('should_notConsume_when_homeAndNoOverlay', () => {
    const consumed = setup().handleSystemBack()

    // 不消费 = 系统默认行为（退出应用）继续生效
    expect(consumed).toBe(false)
    expect(navBack).not.toHaveBeenCalled()
    expect(goHome).not.toHaveBeenCalled()
    expect(closeShade).not.toHaveBeenCalled()
  })
})

describe('C-S9/C-S10 订阅生命周期', () => {
  // watch 默认 pre 冲刷：订阅/退订在下一个调度刻度生效（原生监听本身也是异步注册）
  it('should_subscribeAndUnsubscribe_when_backTargetAppearsAndDisappears', async () => {
    const back = setup()
    await nextTick()
    expect(backChannel.handlers).toHaveLength(0)

    canBack.value = true
    await nextTick()
    expect(backChannel.handlers).toHaveLength(1)
    expect(backChannel.dispose).not.toHaveBeenCalled()

    canBack.value = false
    await nextTick()
    expect(backChannel.handlers).toHaveLength(0)
    expect(backChannel.dispose).toHaveBeenCalledTimes(1)

    // 回到主屏：原生监听不得常驻（否则「主屏按返回退出应用」永久失效）
    expect(back.hasBackTarget.value).toBe(false)
  })

  it('should_stopTakingOver_when_disposed', async () => {
    const back = setup()
    canBack.value = true
    await nextTick()
    expect(backChannel.handlers).toHaveLength(1)

    back.dispose()
    expect(backChannel.handlers).toHaveLength(0)

    // 卸载后状态再变化也不重新订阅（壳不在，返回应交回系统默认行为）
    canBack.value = false
    await nextTick()
    canBack.value = true
    await nextTick()
    expect(backChannel.handlers).toHaveLength(0)
  })
})
