/**
 * 「打开应用」动作
 * -----------------------------------------------------------------------------
 * 首页宫格、应用列表、多任务卡、详情页入口都要「先确保应用在跑，再进运行面」。
 * 这个两步动作在每个屏各写一遍就会出现口径分叉（有的先导航后启动、有的反过来），
 * 因此收口到一处：启动失败不进运行面，让错误留在应用列表上可见。
 */

import { useShellApps } from './useShellApps'
import { useShellNavigation } from './useShellNavigation'

/** 启动并进入运行面；启动失败返回 false（由调用方决定是否提示） */
export async function openShellApp(appId: string): Promise<boolean> {
  const { launch } = useShellApps()
  const nav = useShellNavigation()

  const ok = await launch(appId)
  if (!ok) return false
  nav.openApp(appId)
  return true
}

/** 组合式入口（组件内使用） */
export function useShellOpen(): { open: typeof openShellApp } {
  return { open: openShellApp }
}
