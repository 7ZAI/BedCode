/**
 * 重新激活循环测试用 mock 插件前端模块（忠实复刻版）
 *
 * 经 loader 动态 import 加载。activate 忠实复刻 file-transfer 的注册顺序：
 * 经 context.ui 走 registerSurface + registerRoute + registerSettingsEntry
 * （而非直接调 registry），覆盖 disposable 入 _disposables + registerRoute
 * 经 getSharedModule('router') 的真实路径。若任一步在再激活时抛，
 * loadFrontend catch 的 clearPlugin 会摘除刚注册的入口 → 测试能捕获。
 *
 * 票 2026-10-10 批次 C2：原复刻的 registerToolboxPage 已退役，改用运行面
 * registerSurface——这正是当前应用在壳内的唯一运行面形态。
 */
import type { PluginContext } from '@/plugin/types'

export let activateCallCount = 0
export let deactivateCallCount = 0

export function _resetCounts(): void {
  activateCallCount = 0
  deactivateCallCount = 0
}

export async function activate(context: PluginContext): Promise<void> {
  activateCallCount += 1
  // 1. 壳运行面（经 context.ui；再激活时若未在 deactivate 摘除会留下旧组件引用）
  context.ui.registerSurface({ component: {} })
  // 2. 动态路由（经 getSharedModule('router') → router.addRoute；再激活时若路由
  //    未在 deactivate 摘除会触发重复名问题，本步最可疑）
  context.ui.registerRoute({
    id: 'settings',
    title: 'Settings',
    component: {},
    header: false,
  })
  // 3. 壳内设置入口（旧宿主「设置区」已随阶段 B 退役）
  context.ui.registerSettingsEntry({
    id: `${context.id}.settings`,
    label: 'Settings',
  })
}

export async function deactivate(): Promise<void> {
  deactivateCallCount += 1
}
