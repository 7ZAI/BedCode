/**
 * 内置应用「模拟终端」
 * -----------------------------------------------------------------------------
 * 身份：一个**内置应用**，与被调试插件同构——同样有 id / manifest / activate /
 * deactivate，同样经 `context.ui.registerSurface` 把运行面交给壳。
 *
 * 为什么这么绕（不直接当壳的一页）：
 *   宿主壳的核心形态是「应用自持运行面，壳只挂载」。预览环境里若没有真实应用在场，
 *   插件开发者就无法在 dev-shell 里验证「壳怎么渲染别人的界面」——这条路径只存在于
 *   与真机相同的加载链路里才有效。把模拟终端做成内置应用，等于让预览环境自带一个
 *   正确形态的样本，同时顺带跑通 registerSurface / registerSlot 两条贡献路径。
 *
 * activate 契约与插件入口一致（收 context、注册贡献、返回 Disposable 由外层收集），
 * 因此「内置应用」与「被调试插件」在壳眼里没有区别。
 */

import type { Disposable, PluginContext } from '../../../../src/types'
import MockTerminalSlotCard from './MockTerminalSlotCard.vue'
import MockTerminalView from './MockTerminalView.vue'

/** 内置应用标识（与插件 id 同命名空间，避免与被调试插件撞名） */
export const MOCK_TERMINAL_ID = 'dev.mock-terminal'

/** 内置应用元信息（结构对齐 plugin.json 的可读字段，供数据源投影） */
export const MOCK_TERMINAL_MANIFEST = {
  id: MOCK_TERMINAL_ID,
  name: 'Mock Terminal',
  version: '1.0.0',
  description: 'dev-shell 内置应用：驱动模拟会话 / 生命周期事件，供插件调试前端逻辑',
  author: 'BedCode Dev Shell',
  permissions: ['storage', 'bus', 'session:read'],
}

export function activate(context: PluginContext): Disposable[] {
  const disposables: Disposable[] = []

  // 运行面：壳的应用运行屏渲染它，壳不加任何 chrome（应用自持全部界面）
  disposables.push(context.ui.registerSurface({ component: MockTerminalView }))

  // 首页快捷卡片：让 registerSlot 这条路径在预览环境里也有真实消费方
  disposables.push(
    context.ui.registerSlot({ id: 'sessions', component: MockTerminalSlotCard, order: 10 }),
  )

  return disposables
}

export function deactivate(): void {
  // 贡献项由 mock context 的 _disposables 统一回收（与被调试插件同一条路径）
}