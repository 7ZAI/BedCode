/**
 * 应用内返回栈（系统返回键：应用二级页/浮层 → 应用主页面 → 平台兜底）
 * -----------------------------------------------------------------------------
 * 为什么由应用自己接：宿主壳只认识「运行面 + 平台屏栈」这一层，应用内部的二级页
 * 与浮层（本应用 = 供应商配置页 / 对话列表抽屉 / 插件设置弹层）都是应用自持的
 * 视图状态，壳无从得知。系统返回先经 `context.ui.onBackPressed`（应用层回调），
 * 应用不消费时才落到壳的平台兜底（宿主 `src/plugin/back-button.ts` 的
 * 「应用层 → 平台层」分发）。
 *
 * 行为契约：
 *   - 任一二级层「有可退的界面」→ 后退一步并返回 `true`（本次返回已消费）；
 *   - 已经在应用主页面（聊天页，无浮层无二级页）→ 返回 `false`（不消费），
 *     交给壳兜底：退平台屏栈 / 在主屏则由系统退出应用。
 *
 * 为什么单独成模块：判定顺序（层内优先级、逐级后退、到头不消费）与视图状态解耦，
 * 便于单测钉住；ChatView 只负责按优先级声明层（打开状态 + 后退动作）。
 */

import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

/** 一层应用内二级页面 / 浮层 */
export interface AppBackLayer {
  /** 该层当前是否有可后退的界面（打开中 / 内部还有上一级） */
  canGoBack(): boolean
  /** 后退一步（收掉该浮层，或从该层的内部视图退回上一级） */
  goBack(): void
}

/**
 * 生成应用层返回处理器
 *
 * 按声明顺序找第一个「有可退界面」的层后退一步；所有层都到头即返回 `false`，
 * 让返回落到宿主壳的平台兜底（不吞掉返回，否则用户在主页面按返回无反应）。
 */
export function createAppBackHandler(layers: AppBackLayer[]): () => boolean {
  return () => {
    for (const layer of layers) {
      if (!layer.canGoBack()) continue
      layer.goBack()
      return true
    }
    return false
  }
}

/**
 * 注册应用层返回处理
 *
 * 需要清单声明 `ui:back` 权限（`ui.onBackPressed` 的权限位，见宿主
 * `src/plugin/permission.ts`）；Disposable.dispose = 取消接管（离开运行面时调用，
 * 否则退出运行面后回调仍在，返回会被已关闭的应用吞掉）。
 */
export function registerAppBack(context: PluginContext, layers: AppBackLayer[]): Disposable {
  return context.ui.onBackPressed(createAppBackHandler(layers))
}
