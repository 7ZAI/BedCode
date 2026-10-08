/**
 * 宿主壳屏幕栈导航
 * -----------------------------------------------------------------------------
 * 为什么壳内不用 vue-router 子路由：
 *   壳是一层「应用运行平台」，屏幕栈要支持三件路由表不擅长的事——
 *   ① 应用运行面覆盖在栈上、退出即回落到上一个屏幕（不是 URL 前进后退语义）；
 *   ② 多任务页是临时浮层，进出都不该留下历史；
 *   ③ 屏幕栈要能整体重置（底部 Tab 切换 = 回到该 Tab 的根，而不是 push）。
 *
 * 壳本身仍是一个 vue-router 路由（/mobile/shell），路由只负责「进壳」，
 * 壳内部的流转由这里的栈管理，两者职责不重叠。
 *
 * 状态是模块级单例：壳同时只有一份导航栈（与移动端连接态等既有 composable 一致）。
 */

import { computed, ref, type ComputedRef, type Ref } from 'vue'

/** 壳内屏幕标识 */
export type ShellScreenId =
  /** 平台首页（快捷卡片 + 应用宫格 + 最近使用） */
  | 'home'
  /** WASM 应用管理 */
  | 'apps'
  /** 应用详情与权限 */
  | 'app-detail'
  /** 应用运行面 */
  | 'app-run'
  /** 多任务（运行中应用） */
  | 'switcher'
  /** 权限总览（按权限看应用） */
  | 'permissions'
  /** 我的 / 平台设置 */
  | 'settings'

/** 栈中的一屏 */
export interface ShellScreenEntry {
  id: ShellScreenId
  /** 屏幕参数（目前仅 appId；后续扩展按契约新增字段即可） */
  params: Record<string, string>
}

/** 屏幕切换方向（驱动横向滑动过渡） */
export type ShellNavDirection = 'forward' | 'backward'

const stack: Ref<ShellScreenEntry[]> = ref<ShellScreenEntry[]>([{ id: 'home', params: {} }])
const direction: Ref<ShellNavDirection> = ref('forward')

/** 当前屏 */
const current: ComputedRef<ShellScreenEntry> = computed(
  () => stack.value[stack.value.length - 1] ?? { id: 'home', params: {} },
)

/** 当前屏参数 */
const params: ComputedRef<Record<string, string>> = computed(() => current.value.params)

/** 是否可回退（栈深度 > 1） */
const canBack: ComputedRef<boolean> = computed(() => stack.value.length > 1)

/** 参数是否一致（避免同一应用重复入栈） */
function sameParams(a: Record<string, string>, b: Record<string, string>): boolean {
  const ka = Object.keys(a)
  const kb = Object.keys(b)
  if (ka.length !== kb.length) return false
  return ka.every((k) => a[k] === b[k])
}

/**
 * 压入一屏
 *
 * 若该屏已在栈中且参数一致，则回退到该层（多次点同一入口不会堆出重复栈）。
 */
function navigate(id: ShellScreenId, nextParams: Record<string, string> = {}): void {
  const top = stack.value[stack.value.length - 1]
  if (top && top.id === id && sameParams(top.params, nextParams)) return

  const index = stack.value.findIndex((s) => s.id === id && sameParams(s.params, nextParams))
  if (index >= 0) {
    direction.value = 'backward'
    stack.value = stack.value.slice(0, index + 1)
    return
  }

  direction.value = 'forward'
  stack.value = [...stack.value, { id, params: nextParams }]
}

/** 回退一屏；已在根时无操作 */
function back(): void {
  if (stack.value.length <= 1) return
  direction.value = 'backward'
  stack.value = stack.value.slice(0, -1)
}

/**
 * 回到某个 Tab 的根屏（底部导航语义：不叠加历史）
 *
 * 首页是平台根，始终重置为单屏栈；其余 Tab 同理，避免 Tab 之间来回切出深栈。
 */
function switchTab(id: ShellScreenId): void {
  direction.value = 'forward'
  stack.value = [{ id, params: {} }]
}

/** 回到平台首页（等价于退出所有应用与浮层） */
function goHome(): void {
  switchTab('home')
}

/** 打开应用运行面 */
function openApp(appId: string): void {
  navigate('app-run', { appId })
}

/** 打开应用详情 */
function openDetail(appId: string): void {
  navigate('app-detail', { appId })
}

/** 打开多任务浮层（临时屏，回退即消失，不留在历史里） */
function openSwitcher(): void {
  navigate('switcher')
}

/** 打开权限总览 */
function openPermissions(): void {
  navigate('permissions')
}

/** 供测试与 Hot Reload 复位 */
function resetShellNavigation(): void {
  direction.value = 'forward'
  stack.value = [{ id: 'home', params: {} }]
}

/**
 * 宿主壳导航句柄
 *
 * 所有返回值都是只读视图：屏幕栈只能经本文件的方法变更，避免组件直接改栈导致
 * 方向与栈深不一致（表现为滑动动画反向、回退失效）。
 */
export interface ShellNavigation {
  stack: ComputedRef<ShellScreenEntry[]>
  current: ComputedRef<ShellScreenEntry>
  params: ComputedRef<Record<string, string>>
  direction: ComputedRef<ShellNavDirection>
  canBack: ComputedRef<boolean>
  navigate: typeof navigate
  back: typeof back
  switchTab: typeof switchTab
  goHome: typeof goHome
  openApp: typeof openApp
  openDetail: typeof openDetail
  openSwitcher: typeof openSwitcher
  openPermissions: typeof openPermissions
  reset: typeof resetShellNavigation
}

/** 获取宿主壳导航句柄（单例） */
export function useShellNavigation(): ShellNavigation {
  return {
    stack: computed(() => stack.value),
    current,
    params,
    direction: computed(() => direction.value),
    canBack,
    navigate,
    back,
    switchTab,
    goHome,
    openApp,
    openDetail,
    openSwitcher,
    openPermissions,
    reset: resetShellNavigation,
  }
}

export { resetShellNavigation }
