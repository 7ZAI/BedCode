/**
 * 宿主共享 router 的安全读取
 *
 * 插件前端取路由必须经 SDK `getRouter()`（C4 红线：禁绕过 SDK 直读宿主共享运行时
 * 全局）。该访问器在共享运行时未初始化的环境（dev-shell、vitest 未注入桩）会抛，
 * 而任务域两处消费者都只是「取不到就不渲染/不注册」的渐进增强，故在此收口成
 * null 语义，避免每个调用点各 try/catch 一次。
 */
import { getRouter } from '@binblink/bedcode-plugin-sdk-desktop'

/** 宿主共享 router；非宿主环境取不到返回 null */
export function sharedRouter(): any {
  try {
    return getRouter() ?? null
  } catch {
    return null
  }
}

/** 终端窗口路由（`/terminal-window/:id`）的会话 id；不在终端窗口返回空串 */
export function currentSessionId(): string {
  const id = sharedRouter()?.currentRoute?.value?.params?.id
  return typeof id === 'string' ? id : ''
}
