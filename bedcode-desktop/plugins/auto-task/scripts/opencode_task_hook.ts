/**
 * BedCode OpenCode Task Hook Plugin
 *
 * opencode 插件：同步对话任务状态到 BedCode 桌面端（auto_task_hook.py 的 opencode 等效实现）。
 * 部署到项目 `.opencode/plugins/` 后由 opencode 自动加载（无需注册配置）。
 *
 * 事件映射（对应 Claude Code hooks 的状态机，payload 与 opencode 源码
 * packages/schema/src/session-event.ts / session-status-event.ts / v1/permission.ts 对齐）：
 *   session.created            → idle（opencode 启动会话后推送，宿主据此放行
 *                                waiting 态队列任务，等价 SessionStart）
 *   session.next.prompted      → in_progress（用户提交 prompt，等价 UserPromptSubmit）
 *   session.next.tool.called   → in_progress（工具被调用，等价 PostToolUse 前置信号）
 *   session.status(busy/retry) → in_progress（响应运行中/自动重试，任务未结束）
 *   session.status(idle)       → completed（run 完全收敛：子 agent / 后台工具均结束后
 *                                状态回 idle，天然等价 Stop + background_tasks 判定，
 *                                无 claude 的提前终态风险）
 *   session.idle               → completed（session.status 的 deprecated 前身，兼容回退）
 *   permission.asked           → asking（仅当规则评估需要用户决策时发布，
 *                                等价 Notification(permission_prompt)）
 *   permission.replied         → in_progress（用户已回复授权请求）
 *   session.error              → interrupted（仅 MessageAbortedError，即用户中断；
 *                                其余错误 agent 会重试恢复，按 PostToolUseFailure 语义
 *                                保持 in_progress）
 *   session.next.step.failed   → in_progress（步骤失败会触发自动重试，任务仍在进行）
 *
 * 生效条件：仅当 BEDCODE_SESSION_ID 环境变量存在时推送状态。
 * BedCode 启动的 PTY 终端会自动注入此变量（pty_process.rs），
 * 外部终端不会设置，因此插件在 BedCode 管理的终端外保持静默。
 *
 * HTTP 说明：
 * - 固定使用 127.0.0.1（Windows localhost IPv6 回退会卡 ~2 秒，见
 *   auto_task_hook.py 同名注释）
 * - 终态推送（completed/interrupted）为队列调度链的唯一触发信号，
 *   失败自动重试 3 次，避免静默丢终态卡死调度
 * - payload 携带 event_time（UTC，毫秒 3 位固定宽度），宿主据此拒绝
 *   迟到的旧事件推送（与宿主 SQLite 毫秒格式一致，字典序比较正确）
 *
 * 注意：
 * - opencode 子 agent（Task 工具）与主 agent 同进程运行，事件共享同一
 *   session；子 agent 步骤保持 busy，session.status 回 idle 即整个 run
 *   收敛，无需 pi 扩展那样的子进程静默判定
 * - 插件仅收到 location.directory 与启动目录匹配的会话事件（opencode
 *   按目录路由），天然限定在 BedCode 注入 BEDCODE_SESSION_ID 的会话
 * - 自动授权不在此实现：opencode 用 permissions 配置（opencode.json
 *   的 allow 列表）声明式放行，比 hook 拦截更简单
 */

import type { Plugin } from "@opencode-ai/plugin"

// 部署时由宿主按当前端口改写（hooks.rs replace_opencode_plugin_port），勿手改
const BEDCODE_PORT = 8765 // @bedcode-port
// 模板版本标记：内容升级时递增，宿主据此对旧部署副本自动重部署（hooks.rs）
// @bedcode-template-version 1

const PLUGIN_ID = "com.bedcode.auto-task"
const HOST = "127.0.0.1"
const HTTP_TIMEOUT_MS = 3000
// 终态推送（completed/interrupted）丢失会中断队列调度链，必须重试保证送达
const HTTP_RETRY_ATTEMPTS = 3
const HTTP_RETRY_DELAY_MS = 500

/** 事件发生时刻（UTC，毫秒精度、固定宽度）：与宿主 SQLite strftime 格式一致 */
function eventTime(): string {
  const now = new Date()
  const ms = String(now.getMilliseconds()).padStart(3, "0")
  return `${now.toISOString().slice(0, 19).replace("T", " ")}.${ms}`
}

/** 截断过长的 prompt 用于 reason */
function promptPreview(text: string): string {
  return text.length > 100 ? `${text.slice(0, 100)}...` : text
}

/**
 * 推送任务状态到 BedCode 桌面端 HTTP API
 * 失败不阻塞主流程，仅记录日志；终态推送自动重试。
 */
async function push(status: string, reason: string, questions?: unknown): Promise<void> {
  // 仅在 BedCode 启动的 PTY 终端中生效（外部终端无此环境变量）
  const bedcodeSessionId = process.env.BEDCODE_SESSION_ID
  if (!bedcodeSessionId) return

  // 宿主以 bedcode 会话 ID 键控任务行（on_input_submitted / 队列出队写入），
  // session_id 与 bedcode_session_id 同值即可命中（见 state.rs handle_update_task_status）
  const payload: Record<string, unknown> = {
    session_id: bedcodeSessionId,
    status,
    reason: reason || "",
    event_time: eventTime(),
    bedcode_session_id: bedcodeSessionId,
  }
  if (questions) payload.questions = questions

  for (let attempt = 1; attempt <= HTTP_RETRY_ATTEMPTS; attempt++) {
    try {
      const resp = await fetch(
        `http://${HOST}:${BEDCODE_PORT}/api/plugin/${PLUGIN_ID}/task-status`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
          signal: AbortSignal.timeout(HTTP_TIMEOUT_MS),
        },
      )
      if (!resp.ok) {
        console.warn(`[bedcode] push failed (${resp.status}): ${await resp.text()}`)
      }
      return
    } catch (e) {
      if (attempt < HTTP_RETRY_ATTEMPTS) {
        await new Promise((r) => setTimeout(r, HTTP_RETRY_DELAY_MS))
      } else {
        console.warn(`[bedcode] push failed after ${HTTP_RETRY_ATTEMPTS} attempts: ${e}`)
      }
    }
  }
}

/**
 * permission.asked → PluginQuestion 兼容格式（复用宿主提问面板 DTO）：
 * 权限请求没有选项结构，按 notification 的 Allow/Deny 双选项构造。
 */
function permissionQuestions(properties: Record<string, unknown>): unknown[] {
  const permission = String(properties.permission ?? "")
  if (!permission) return []
  return [
    {
      question: `Permission requested: ${permission}`,
      header: "opencode permission",
      multi_select: false,
      options: [
        { label: "Allow", description: "Approve the request" },
        { label: "Deny", description: "Reject the request" },
      ],
    },
  ]
}

export const BedCodeTaskHook: Plugin = async () => {
  return {
    event: async ({ event }) => {
      const properties = (event.properties ?? {}) as Record<string, unknown>

      switch (event.type) {
        // 会话启动 → idle：宿主据此放行 waiting 态队列任务
        case "session.created":
          void push("idle", "Session started")
          break

        // 用户提交 prompt（含队列自动投递）→ 任务进入执行状态
        case "session.next.prompted": {
          const text = (properties.prompt as { text?: string } | undefined)?.text ?? ""
          void push("in_progress", `User submitted: ${promptPreview(text)}`)
          break
        }

        // 工具被调用 → 任务进行中
        case "session.next.tool.called": {
          const tool = String(properties.tool ?? "")
          void push("in_progress", `Tool ${tool} called`)
          break
        }

        // run 状态机：busy/retry 运行中，idle 为完全收敛（终态由 Stop 等价语义判定）
        case "session.status": {
          const status = (properties.status as { type?: string } | undefined)?.type
          if (status === "idle") {
            void push("completed", "Task completed")
          } else if (status === "retry") {
            void push("in_progress", "Auto retry in progress")
          } else {
            void push("in_progress", "Response running")
          }
          break
        }

        // session.status 的 deprecated 前身：兼容旧版 opencode
        case "session.idle":
          void push("completed", "Task completed")
          break

        // 权限请求需要用户决策 → asking（自动放行的请求不会发布此事件）
        case "permission.asked": {
          const questions = permissionQuestions(properties)
          void push("asking", "Waiting for user permission", questions)
          break
        }

        // 用户已回复授权 → 恢复执行
        case "permission.replied":
          void push("in_progress", "User replied to permission")
          break

        // 用户中断（Ctrl+C / Esc）→ interrupted；其余错误 agent 会恢复 → 保持进行中
        case "session.error": {
          const error = (properties.error ?? {}) as { name?: string; data?: { message?: string } }
          if (error.name === "MessageAbortedError") {
            void push("interrupted", "User interrupted the run")
          } else {
            void push("in_progress", `Run error (recovering): ${error.name ?? "unknown"}`)
          }
          break
        }

        // 步骤失败会触发自动重试 → 任务仍在进行
        case "session.next.step.failed":
          void push("in_progress", "Step failed, retrying")
          break
      }
    },
  }
}
