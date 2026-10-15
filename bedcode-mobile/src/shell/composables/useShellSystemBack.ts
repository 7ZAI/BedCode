/**
 * 壳的系统返回键兜底
 * -----------------------------------------------------------------------------
 * 壳要处理的情况（按处理顺序）：
 *   ① 覆盖层开着（通知栏 / 胶囊菜单 / 授权弹窗）→ 先收掉它
 *   ② 屏栈可回退 → 退一屏
 *   ③ 停在非主屏的 Tab 根（应用管理 / 权限 / 设置）→ 回主屏
 * 都轮不到（就在主屏且无覆盖层）时不消费：让系统默认行为生效（退出应用）。
 *
 * 为什么**按需订阅**而不是常驻：只要 JS 侧存在 back-button listener，系统返回就一律
 * 转发到 JS、不再执行默认退出（见 `@/plugin/back-button` 头注）。常驻订阅会让「主屏
 * 按返回退出应用」永久失效，所以只在确实有东西可返回时才持有监听。
 *
 * 与应用内处理器的关系：应用（如文件传输逐级退目录）先拿到返回，未消费（回调未返回
 * true）才落到这里——同一个返回不会被应用和平台各处理一次。
 *
 * 生命周期由调用方（ShellHost）在卸载时 `dispose()`：壳不在时没有任何可退的屏，
 * 返回应交回系统默认 / 当前整页路由自己的历史。
 */

import { computed, watch, type ComputedRef } from 'vue'
import { subscribeHostBackPressed } from '@/plugin/back-button'
import { useShellNavigation } from './useShellNavigation'
import { useShellOverlays } from './useShellOverlays'

/** 壳系统返回兜底句柄 */
export interface ShellSystemBack {
  /** 有可返回目标（驱动是否接管系统返回） */
  hasBackTarget: ComputedRef<boolean>
  /** 平台兜底返回处理（导出以便直接断言；正常由原生监听触发） */
  handleSystemBack: () => boolean
  /** 停止接管并撤销订阅（ShellHost 卸载时调用） */
  dispose: () => void
}

export function useShellSystemBack(): ShellSystemBack {
  const nav = useShellNavigation()
  const overlays = useShellOverlays()

  const hasBackTarget = computed(
    () =>
      overlays.shadeOpen.value ||
      overlays.capsuleAppId.value !== null ||
      overlays.permissionRequest.value !== null ||
      nav.canBack.value ||
      nav.current.value.id !== 'home',
  )

  /** 平台兜底返回处理；返回 true = 本次已消费 */
  function handleSystemBack(): boolean {
    if (overlays.shadeOpen.value) {
      overlays.closeShade()
      return true
    }
    if (overlays.capsuleAppId.value !== null) {
      overlays.closeCapsule()
      return true
    }
    if (overlays.permissionRequest.value !== null) {
      overlays.closePermissionRequest()
      return true
    }
    if (nav.canBack.value) {
      nav.back()
      return true
    }
    if (nav.current.value.id !== 'home') {
      nav.goHome()
      return true
    }
    // 主屏且无覆盖层：不消费——系统默认行为把应用切到后台
    return false
  }

  let disposeBackPressed: (() => void) | null = null

  // immediate：壳挂载时就可能已有可返回目标（例如热重载保留了屏栈）
  const stopWatch = watch(
    hasBackTarget,
    (active) => {
      if (active) {
        if (!disposeBackPressed) disposeBackPressed = subscribeHostBackPressed(handleSystemBack)
        return
      }
      disposeBackPressed?.()
      disposeBackPressed = null
    },
    { immediate: true },
  )

  function dispose(): void {
    stopWatch()
    disposeBackPressed?.()
    disposeBackPressed = null
  }

  return { hasBackTarget, handleSystemBack, dispose }
}
