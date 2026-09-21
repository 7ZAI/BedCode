/**
 * Session Store — 宿主终端窗口的会话事实缓存 + 最小会话动作
 *
 * 两条通道（2026-09-21 命令面收敛后，见
 * `.scratch/2026-09-21-host-rust-residue/issues/05`）：
 * - **引擎事实/渲染管道**：`list_sessions` / `resize_session` /
 *   `write_to_session` / `send_special_key`（宿主命令面直连，终端红线保留）
 * - **业务动作**：一律经 `com.bedcode.terminal-session` 插件命令面（`plugin_invoke` 转发）——
 *   停止会话 `session.close`、读配置 `session.config.list`（插件私有库为配置真源）
 *
 * 已删除（宿主命令面注销 + 无生产调用方）：创建 / 两阶段启动 / 移除 / 重启 /
 * 配置 CRUD 五个 action——产品面归会话中心插件，宿主 shell 不再承载会话编排。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { pluginInvoke } from '@/plugin/commands'
import {
  listSessions,
  writeToSession,
  sendSpecialKey,
  resizeSession,
} from '@/composables/useDesktopCommands'
import type { SessionConfig, SessionInfo } from '@/composables/model'

export { type SessionConfig, type SessionInfo }

/** 会话中心插件 ID（会话业务域唯一权威；宿主 Rust 侧同值常量见 `utils/auth/auth_center.rs`） */
const SESSION_PLUGIN_ID = 'com.bedcode.terminal-session'

export const useSessionStore = defineStore('session', () => {
  const sessions = ref<SessionInfo[]>([])

  /** 拉取会话列表（引擎事实面，插件写入后宿主据此刷新） */
  async function loadSessions() {
    sessions.value = await listSessions()
    logger.log(
      'loadSessions completed, sessions:',
      sessions.value.map((s) => ({ id: s.id, status: s.status })),
    )
  }

  /**
   * 读单个会话配置（工具条展示 cwd / 命令用）
   *
   * 真源在插件私有库，经插件命令面 `session.config.list` 取全量后按 id 命中；
   * 未命中返回 null（配置已删除 / 会话由移动端创建）
   */
  async function loadSessionConfig(configId: string): Promise<SessionConfig | null> {
    const configs = (await pluginInvoke(SESSION_PLUGIN_ID, 'session.config.list')) as SessionConfig[]
    return configs.find((c) => c.id === configId) ?? null
  }

  /**
   * 停止会话（保留会话记录，置 Stopped）
   *
   * 编排归插件（存在性预检 + `host-session.close`）；插件未激活时命令面显性
   * 报错，调用方据此提示用户（不做宿主降级）。
   */
  async function stopSession(sessionId: string) {
    logger.log('stopSession called with sessionId:', sessionId)
    await pluginInvoke(SESSION_PLUGIN_ID, 'session.close', { sessionId })
    sessions.value = await listSessions()
  }

  async function writeToSessionAction(sessionId: string, data: string) {
    await writeToSession(sessionId, data)
  }

  async function sendSpecialKeyAction(sessionId: string, key: string) {
    await sendSpecialKey(sessionId, key)
  }

  async function resizeSessionAction(sessionId: string, cols: number, rows: number, force = false) {
    return await resizeSession(sessionId, cols, rows, force)
  }

  return {
    sessions,
    loadSessions,
    loadSessionConfig,
    stopSession,
    writeToSession: writeToSessionAction,
    sendSpecialKey: sendSpecialKeyAction,
    resizeSession: resizeSessionAction,
  }
})
