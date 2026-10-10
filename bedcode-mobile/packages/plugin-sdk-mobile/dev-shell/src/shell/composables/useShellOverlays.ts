/**
 * Dev Shell 宿主壳覆盖层状态（与宿主 shell/composables/useShellOverlays.ts 同构）
 * -----------------------------------------------------------------------------
 * 两类平台级覆盖层都由壳统一渲染，不由各应用自己弹：
 *   ① 胶囊菜单：平台叠加在应用之上的控制项（与小程序胶囊同构）
 *   ② 运行时授权弹窗：应用临时申请权限时的统一裁决界面
 *
 * 统一渲染的原因是这两类界面必须与平台视觉和安全口径一致（拒绝即不可用、
 * fail-closed）。状态走模块级单例，任何屏幕都能打开，渲染位置固定在壳根。
 */

import { computed, ref, type ComputedRef, type Ref } from 'vue'

/** 一次运行时权限申请 */
export interface ShellPermissionRequest {
  /** 申请方应用 id */
  appId: string
  /** 申请的权限词 */
  keys: string[]
  /** 应用自述用途（键为权限词） */
  reasons?: Record<string, string>
  /** 申请目标（如域名 / 目录），键为权限词；缺省不展示 */
  targets?: Record<string, string>
}

const capsuleAppId: Ref<string | null> = ref(null)
const permissionRequest: Ref<ShellPermissionRequest | null> = ref(null)

/** 打开胶囊菜单 */
function openCapsule(appId: string): void {
  capsuleAppId.value = appId
}

/** 关闭胶囊菜单 */
function closeCapsule(): void {
  capsuleAppId.value = null
}

/**
 * 发起一次运行时授权请求
 *
 * 由「应用详情 → 演示授权弹窗」驱动；后续接入真授权事件后，事件侧同样调这里，
 * UI 与裁决口径复用同一份实现。
 */
function requestPermissions(request: ShellPermissionRequest): void {
  permissionRequest.value = request
}

/** 关闭授权弹窗（同时清空请求，避免残留状态影响下一次） */
function closePermissionRequest(): void {
  permissionRequest.value = null
}

/** 宿主壳覆盖层句柄 */
export interface ShellOverlays {
  capsuleAppId: ComputedRef<string | null>
  permissionRequest: ComputedRef<ShellPermissionRequest | null>
  openCapsule: typeof openCapsule
  closeCapsule: typeof closeCapsule
  requestPermissions: typeof requestPermissions
  closePermissionRequest: typeof closePermissionRequest
}

/** 获取宿主壳覆盖层句柄（单例） */
export function useShellOverlays(): ShellOverlays {
  return {
    capsuleAppId: computed(() => capsuleAppId.value),
    permissionRequest: computed(() => permissionRequest.value),
    openCapsule,
    closeCapsule,
    requestPermissions,
    closePermissionRequest,
  }
}

export { openCapsule, closeCapsule, requestPermissions, closePermissionRequest }