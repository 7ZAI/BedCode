/**
 * 宿主事件 → 壳通知中心桥
 * -----------------------------------------------------------------------------
 * 壳的通知栏要显示「平台自己发生的事」。平台事实里有一条来自宿主引擎：
 * 与桌面端的连接生命周期（建立 / 重连 / 断开 / 重认证被拒）——这些事件由 Rust 侧
 * 直接 emit，前端其它消费者（旧宿主 `useMobileConnection`）在壳内不可直连（L7），
 * 因此壳自持一个轻量监听器，把事件翻译成通知卡片。
 *
 * 边界（AGENTS §5.1 B6）：壳**不解释业务事件**，只投影引擎连接事实；
 * 文案是平台自己的（i18n），不携带任何插件业务语义。
 *
 * 失败口径：非 Tauri 运行时（浏览器预览 / 单测）拿不到事件总线，按 warn 记录后
 * 静默不注册——通知栏退化为只显示壳自身动作与待审批，不影响其它功能。
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import i18n from '@/locales'
import { logger } from '@/utils/frontendLogger'
import { useShellNotifications } from './useShellNotifications'

/** 图标（24×24 stroke 视框，与壳内其它 SVG 同规范） */
const ICONS = {
  link: 'M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71 M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71',
  linkOff:
    'M1 1l22 22 M16.72 11.06A10.94 10.94 0 0119 12.55 M5 12.55a10.94 10.94 0 015.17-2.39 M10.71 5.05A16 16 0 0122.58 9 M1.42 9a15.91 15.91 0 014.7-2.88 M8.53 16.11a6 6 0 016.95 0 M12 20h.01',
  shieldX: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9.5 9.5l5 5 M14.5 9.5l-5 5',
} as const

/**
 * 注册宿主事件监听
 *
 * @returns 停止函数；单个事件注册失败不影响其它事件（各自 try 包裹）。
 */
export async function startShellHostEvents(): Promise<() => void> {
  const { pushNotification } = useShellNotifications()
  const t = i18n.global.t
  const source = t('shell.notify.source.host')
  const stops: UnlistenFn[] = []

  /** 注册一个事件；失败只记录，不抛出（缺事件总线时通知栏降级而非崩壳） */
  async function on(event: string, handler: (payload: unknown) => void): Promise<void> {
    try {
      stops.push(await listen(event, (e) => handler(e.payload)))
    } catch (err) {
      logger.warn(`[Shell] host event listen failed: event=${event}`, err)
    }
  }

  await on('ws_connected', () => {
    pushNotification({
      kind: 'event',
      icon: ICONS.link,
      source,
      title: t('shell.notify.connected.title'),
      body: t('shell.notify.connected.body'),
    })
  })

  await on('ws_reconnected', () => {
    pushNotification({
      kind: 'event',
      icon: ICONS.link,
      source,
      title: t('shell.notify.reconnected.title'),
      body: t('shell.notify.reconnected.body'),
    })
  })

  await on('ws_disconnected', (payload) => {
    // 断开原因由宿主给出（如认证失败 / 网络中断）；缺字段时不编造，只显示标题
    const reason =
      payload && typeof payload === 'object' && 'reason' in payload
        ? String((payload as { reason?: unknown }).reason ?? '')
        : ''
    pushNotification({
      kind: 'event',
      icon: ICONS.linkOff,
      source,
      title: t('shell.notify.disconnected.title'),
      body: reason !== '' ? reason : t('shell.notify.disconnected.body'),
    })
  })

  await on('ws_reauth_rejected', (payload) => {
    const reason =
      payload && typeof payload === 'object' && 'reason' in payload
        ? String((payload as { reason?: unknown }).reason ?? '')
        : ''
    pushNotification({
      kind: 'event',
      icon: ICONS.shieldX,
      source,
      title: t('shell.notify.reauthRejected.title'),
      body: reason !== '' ? reason : t('shell.notify.reauthRejected.body'),
      // 需要重新配对才能恢复：驻留到用户处理，避免被「清除全部」顺手抹掉
      sticky: true,
    })
  })

  return () => {
    for (const stop of stops) stop()
  }
}
