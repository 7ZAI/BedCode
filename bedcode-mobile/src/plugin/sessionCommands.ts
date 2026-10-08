/**
 * 会话控制命令面（票 13）
 *
 * 会话控制（list / start / stop / remove / input-HTTP）从宿主 Rust 客户端
 * （`session::http::SessionHttpClient` + `commands::session`，票 13 已退役）迁入
 * 插件 `com.bedcode.terminal-session`：插件经 host-http（`jwtAuth` 宿主代注
 * Bearer，token 不落插件）直连桌面 `/api/sessions*`。
 *
 * 返回形状与退役前 `useHttpApi` 的会话函数逐字段一致（`ApiResult`：成功透传
 * 桌面信封；插件命令失败归一为 `{code: -1, message}`）——调用点
 * （useMobileConnection / SessionsView / usePresetTasks / plugin context）行为等价。
 *
 * 边界（票 13 点名，勿顺手扩面）：`resize` 不在此面（留 `useHttpApi`
 * 的 HTTP 代理路径，票 15 终端 UI 迁移时经 `mobileApi.httpRequest` 消费）。
 */

import { pluginInvoke } from '@/plugin/commands'
import { logger } from '@/utils/frontendLogger'
import type { ApiResult } from '@/composables/useHttpApi'

/** 移动端内置会话/终端插件（D6 选项 A：与桌面同名同构，职责为远程终端控制端） */
const TERMINAL_PLUGIN_ID = 'com.bedcode.terminal-session'

/**
 * 调插件会话控制命令并归一失败语义。
 *
 * 插件命令失败（插件未激活 / 未连接 / 网络故障 / 权限门拒绝）与退役前
 * `useHttpApi` 的 catch 一致 → `{code: -1, message}`（不抛，调用点按 code 分支）。
 */
async function invokeSessionCommand<T>(command: string, args: Record<string, unknown> = {}): Promise<ApiResult<T>> {
  try {
    return (await pluginInvoke(TERMINAL_PLUGIN_ID, command, args)) as ApiResult<T>
  } catch (e: unknown) {
    // Rust AppError 经 Tauri IPC 以纯字符串 reject（无 .message），需按类型提取
    const message = typeof e === 'string' ? e : ((e as Error)?.message ?? String(e))
    logger.error(`[SessionCommands] ${command} failed:`, message)
    return { code: -1, message }
  }
}

/** 会话列表（桌面 `GET /api/sessions`） */
export function listSessions(): Promise<ApiResult<{ sessions: any[] }>> {
  return invokeSessionCommand('terminal-session.list-sessions')
}

/** 启动会话（桌面 `POST /api/sessions/start`）；size = 本端终端组件按设备屏幕预算的默认网格 */
export function startSession(
  configId: string,
  size?: { cols: number; rows: number },
): Promise<ApiResult<{ sessionId: string; status: string }>> {
  return invokeSessionCommand('terminal-session.start-session', {
    configId,
    ...(size ? { cols: size.cols, rows: size.rows } : {}),
  })
}

/** 停止会话（桌面 `POST /api/sessions/{id}/stop`） */
export function stopSession(sessionId: string): Promise<ApiResult> {
  return invokeSessionCommand('terminal-session.stop-session', { sessionId })
}

/** 删除会话（桌面 `DELETE /api/sessions/{id}/remove`） */
export function removeSession(sessionId: string): Promise<ApiResult> {
  return invokeSessionCommand('terminal-session.remove-session', { sessionId })
}

/**
 * 直发终端输入（桌面 `POST /api/sessions/{id}/input`，绕过 WS 订阅的 HTTP 通道）
 *
 * 与插件 WS 帧 `terminal-session.send-input`（订阅链路输入）是两条通道：
 * 本函数供 TUI 滚轮序列 / 预设任务下发使用（不依赖 subscribe 连接）；
 * specialKey 由桌面端翻译。`context.terminal.sendInput` 外部插件 API 已随
 * 票 15 阶段 B 退役（TerminalAPI 整面删除）。
 */
export function sendHttpInput(sessionId: string, data: string, specialKey?: string): Promise<ApiResult> {
  return invokeSessionCommand('terminal-session.send-http-input', {
    sessionId,
    data,
    specialKey: specialKey ?? null,
  })
}
