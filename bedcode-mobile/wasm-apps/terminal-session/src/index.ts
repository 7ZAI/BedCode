/**
 * Terminal Session Plugin（移动版）— TS 前端入口（票 12 建壳，票 16 并入任务域，票 15 并入终端 UI 域）
 *
 * 本插件是「远程终端控制端」（D6 选项 A：与桌面端同名但职责不同，契约独立，
 * 两端不因同名互相约束）。前端按域组织：
 * - `src/app/**`：应用壳域（票 2026-10-10：底部导航 + 页签容器 + 终端沉浸运行面，
 *   承接旧宿主 `MobileLayout + MobileSwipeContainer + MobileNav` 三件套职责）
 * - `src/host/**`：设备 / 会话 / 连接态内容面（票 2026-10-09 自宿主 src/ 迁入）
 * - `src/terminal/**`：终端消费 UI 域（TerminalView / terminalBuffer store / 终端
 *   输入助手 / 样式与文案，票 15 自宿主 src/ 整体迁入）
 * - `src/task/**`：任务域（原 auto-task 插件整体并入，票 16）——任务队列面板 /
 *   工具箱「任务记录 + 定时任务」/ 桌面任务域只读投影（ADR 0012「手机看、桌面管」）
 *
 * 插件业务面（终端订阅协议客户端 + 认证/配对编排）在 WASM 后端（rust/src/）。
 *
 * ⚠️ 文案口径（spec D6 强制⑥）：本 app 是「远程终端控制端」，不是会话/终端/任务
 * 权威；任务域数据面是桌面任务域的只读投影，UI 文案不得自称权威。
 */
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { activateAppDomain } from './app/activate'
import { activateHostPageDomain } from './host/activate'
import { activateTaskDomain, deactivateTaskDomain } from './task/activate'
import { activateTerminalDomain } from './terminal/activate'

// dev-shell 领域数据（任务队列种子）：从插件入口模块导出，dev-shell 按此协议装载
export { devMock } from './task/devMock'

let terminalDomain: Disposable | null = null
let hostDomain: Disposable | null = null
let appDomain: Disposable | null = null

export async function activate(context: PluginContext): Promise<void> {
  context.logger.info('Terminal Session plugin activating (remote-terminal-consumer, mobile)')
  // 内容域先于壳域激活：运行面组件 setup 时要能取到 host / terminal / task 三域文案
  hostDomain = activateHostPageDomain(context)
  terminalDomain = activateTerminalDomain(context)
  await activateTaskDomain(context)
  appDomain = activateAppDomain(context)
  context.logger.info('Terminal Session plugin activated')
}

export function deactivate(): void {
  appDomain?.dispose()
  appDomain = null
  hostDomain?.dispose()
  hostDomain = null
  terminalDomain?.dispose()
  terminalDomain = null
  deactivateTaskDomain()
}

export default { activate, deactivate }
