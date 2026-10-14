/**
 * 应用壳域激活（票 2026-10-10：全量 UI 下沉 —— 底部导航 + 页签容器）
 *
 * - 注册域 i18n（app.*，双语）
 * - 注册宿主壳运行面（AppRoot = 底部导航 + 页签容器 + 终端沉浸，壳内打开本 app 即进入）
 *
 * 激活顺序约束：运行面组件在壳渲染时才 setup，但文案必须在本域注册完成后才可能
 * 被取到——因此本域在 host / terminal / task 三域之后激活（见 ../index.ts）。
 *
 * 返回 Disposable；deactivate 对称回收。
 */
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import AppRoot from './AppRoot.vue'
import { messagesEn as appEn, messagesZhCN as appZhCN } from './i18n'
import { messagesEn as settingsEn, messagesZhCN as settingsZhCN } from '../settings/i18n'

export function activateAppDomain(context: PluginContext): Disposable {
  context.i18n.registerMessages('zh-CN', appZhCN)
  context.i18n.registerMessages('en', appEn)
  // 设置域文案随运行面一起注册：运行面内含设置页签，setup 时就要能取到 settings.* 键
  context.i18n.registerMessages('zh-CN', settingsZhCN)
  context.i18n.registerMessages('en', settingsEn)

  const surface = context.ui.registerSurface({ component: AppRoot })

  context.logger.info('[terminal-session] app shell domain activated')

  return {
    dispose() {
      surface.dispose()
      context.logger.info('[terminal-session] app shell domain deactivated')
    },
  }
}