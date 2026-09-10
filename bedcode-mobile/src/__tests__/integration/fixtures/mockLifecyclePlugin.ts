/**
 * 重新激活循环测试用 mock 插件前端模块（忠实复刻版）
 *
 * 经 loader 动态 import 加载。activate 忠实复刻 file-transfer 的注册顺序：
 * 经 context.ui 走 registerToolboxPage + registerRoute + registerSettingsSection
 * （而非直接调 registry），覆盖权限层 + disposable 入 _disposables + registerRoute
 * 经 getSharedModule('router') 的真实路径。若任一步在再激活时抛，
 * loadFrontend catch 的 clearPlugin 会摘除刚注册的入口 → 测试能捕获。
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
  // 1. 工具箱视图（经 context.ui 权限层 + disposable 入 _disposables）
  context.ui.registerToolboxPage({
    id: `${context.id}.toolbox`,
    title: 'Mock Plugin',
    icon: 'M8 7h12',
    component: {},
    entry: undefined,
  })
  // 2. 动态路由（经 getSharedModule('router') → router.addRoute；再激活时若路由
  //    未在 deactivate 摘除会触发重复名问题，本步最可疑）
  context.ui.registerRoute({
    id: 'settings',
    title: 'Settings',
    component: {},
    header: false,
  })
  // 3. 设置区
  context.ui.registerSettingsSection({
    id: `${context.id}.settings`,
    pluginId: context.id,
    section: 'mock',
    component: {},
  })
}

export async function deactivate(): Promise<void> {
  deactivateCallCount += 1
}
