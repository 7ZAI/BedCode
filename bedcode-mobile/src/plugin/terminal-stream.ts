/**
 * 终端输出页面通道（票 15：宿主为插件提供的传输机制封装）
 *
 * 插件前端禁止直连 @tauri-apps/api；此处由宿主创建页面 Channel 并登记
 * （terminal_page_subscribe，裸字节 Raw 投递），把「按序字节回调」交给插件。
 * 链路订阅（terminal-session.subscribe）仍由插件命令面负责。
 */

import { Channel } from '@tauri-apps/api/core'
import { terminalPageSubscribe, terminalPageUnsubscribe } from '@/composables/useMobileCommands'
import { logger } from '@/utils/frontendLogger'

/** 终端流句柄（插件侧只持 dispose） */
export interface HostTerminalStreamHandle {
  dispose(): void
}

/** 打开终端输出字节流（登记语义：同一会话重复调用以最后一次通道为准） */
export async function openTerminalStream(
  sessionId: string,
  onBytes: (bytes: Uint8Array) => void,
): Promise<HostTerminalStreamHandle> {
  const channel = new Channel<ArrayBuffer>()
  channel.onmessage = (payload) => {
    const bytes =
      payload instanceof ArrayBuffer ? new Uint8Array(payload) : new Uint8Array(payload as ArrayBufferLike)
    onBytes(bytes)
  }
  await terminalPageSubscribe(sessionId, channel)

  let disposed = false
  return {
    dispose() {
      if (disposed) return
      disposed = true
      // 先摘回调再注销通道：注销往返期间到达的帧直接丢弃（页面已离开）
      channel.onmessage = () => {}
      void terminalPageUnsubscribe(sessionId).catch((e) => {
        logger.warn(`[TerminalStream] page unsubscribe failed: sessionId=${sessionId}`, e)
      })
    },
  }
}
