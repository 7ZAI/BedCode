/**
 * 宿主页域激活（票 2026-10-09：旧宿主主流程下沉 —— 设备/会话/连接态整体页）
 *
 * - 注册域 i18n（hub.*，双语）
 * - 注册宿主壳运行面（HostPage = 整体页面，壳内打开本 app 即进入）
 * - 注册宿主壳首页快捷卡片（活跃会话数，ShellHome 槽位）
 * 返回 Disposable；deactivate 对称回收。
 */
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import HostPage from './HostPage.vue'
import SlotCard from './SlotCard.vue'
import { messagesEn, messagesZhCN } from './i18n'

export function activateHostPageDomain(context: PluginContext): Disposable {
  context.i18n.registerMessages('zh-CN', messagesZhCN)
  context.i18n.registerMessages('en', messagesEn)

  const surface = context.ui.registerSurface({ component: HostPage })
  const slot = context.ui.registerSlot({
    id: 'host-sessions',
    component: SlotCard,
    order: 10,
  })

  context.logger.info('[terminal-session] host page domain activated')

  return {
    dispose() {
      surface.dispose()
      slot.dispose()
      context.logger.info('[terminal-session] host page domain deactivated')
    },
  }
}