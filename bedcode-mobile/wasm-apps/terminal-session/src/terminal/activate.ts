/**
 * 终端域激活（票 15：终端 UI 域下沉）
 *
 * - 注入域宿主服务（PluginContext → host.ts）
 * - 注册终端域文案（zh-CN / en；键自动加插件 id 前缀，取文案走 host.t）
 * - 终端主视图不再独立注册（票 2026-10-10 批次 C2 退役 `registerTerminalView`），
 *   改由 app 域运行面在终端子页内直接渲染本域的 `TerminalView.vue`
 * - 全域样式注入（xterm.css / terminal.css / markdown-body.css，?inline 运行时挂载）
 * - 会话/连接生命周期 → 终端订阅状态联动（宿主 onSessionEvent 白名单事件；
 *   与迁移前 `useMobileConnection` 内的联动逐条一致）
 */

import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'
import xtermCss from '@xterm/xterm/css/xterm.css?inline'
import { createTerminalHostServices, initTerminalHost, logger } from './host'
import { messagesEn, messagesZhCN } from './i18n'
import { useTerminalBuffer } from './composables/useTerminalBuffer'
import markdownCss from './styles/markdown-body.css?inline'
import terminalCss from './styles/terminal.css?inline'

/** 幂等样式注入（Disposable 移除；与 activate/deactivate 对称回收） */
function injectStyle(id: string, css: string): Disposable {
  const el = document.createElement('style')
  el.dataset.bedcodeTerminalStyle = id
  el.textContent = css
  document.head.appendChild(el)
  return {
    dispose() {
      el.remove()
    },
  }
}

/** 激活终端域；返回 Disposable（deactivate 时回收注册、样式与事件桥） */
export function activateTerminalDomain(context: PluginContext): Disposable {
  initTerminalHost(createTerminalHostServices(context))
  context.i18n.registerMessages('zh-CN', messagesZhCN)
  context.i18n.registerMessages('en', messagesEn)

  const buffer = useTerminalBuffer()

  const disposables: Disposable[] = [
    injectStyle('xterm', xtermCss),
    injectStyle('terminal', terminalCss),
    injectStyle('markdown-body', markdownCss),
    // 票 2026-10-10 批次 C2：`registerTerminalView` 已整面退役——终端主视图不再
    // 作为「宿主薄壳渲染的独立扩展点」存在，改由 app 域的运行面（AppRoot）在
    // 终端子页内直接渲染本组件（见 ../app/AppRoot.vue）。
    // 会话/连接生命周期 → 订阅状态联动（白名单事件；断线清信念，
    // running 仅在未跟踪/已停止时复位，停止/删除走 store 对应清理）
    getMobileApi().onSessionEvent((event) => {
      switch (event.type) {
        case 'disconnected':
          buffer.store.markAllUnsubscribed()
          break
        case 'session_status':
          if (event.newStatus === 'running' && event.sessionId) {
            const b = buffer.store.getBuffer(event.sessionId)
            if (!b || b.sessionStopped) buffer.store.markSessionRunning(event.sessionId)
          }
          break
        case 'session_stopped':
          if (event.sessionId) void buffer.handleSessionStopped(event.sessionId)
          break
        case 'session_removed':
          if (event.sessionId) void buffer.handleSessionRemoved(event.sessionId)
          break
      }
    }),
  ]

  logger.debug('[terminal-session] terminal domain activated')
  return {
    dispose() {
      for (const d of disposables) d.dispose()
      initTerminalHost(null)
      logger.debug('[terminal-session] terminal domain deactivated')
    },
  }
}
