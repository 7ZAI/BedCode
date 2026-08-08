/**
 * BedCode Pi Task Hook Extension
 *
 * pi 扩展：同步对话任务状态到 BedCode 桌面端（auto_task_hook.py 的 pi 等效实现）。
 * 部署到项目 `.pi/extensions/` 后由 pi 自动发现加载（无需注册配置）。
 *
 * 事件映射（对应 Claude Code hooks 的状态机）：
 *   session_start        → idle（pi 启动 / /new 重建会话后推送，
 *                          宿主据此放行 waiting 态队列任务，等价 SessionStart）
 *   before_agent_start   → in_progress（用户提交 prompt，等价 UserPromptSubmit）
 *   tool_execution_end   → in_progress（工具执行结束，等价 PostToolUse）
 *   agent_settled        → completed（run 完全收敛：自动重试/压缩/排队续跑
 *                          均结束后触发，等价 Stop + background_tasks 判定）
 *   session_shutdown     → interrupted（仅 quit：退出 pi 时任务未完成视为中断；
 *                          new/fork/resume 只是会话切换，pi 仍运行，不推送）
 *
 * 生效条件：仅当 BEDCODE_SESSION_ID 环境变量存在时推送状态。
 * BedCode 启动的 PTY 终端会自动注入此变量（pty_process.rs），
 * 外部终端不会设置，因此扩展在 BedCode 管理的终端外保持静默。
 *
 * HTTP 说明：
 * - 固定使用 127.0.0.1（Windows localhost IPv6 回退会卡 ~2 秒，见
 *   auto_task_hook.py 同名注释）
 * - 终态推送（completed/interrupted）为队列调度链的唯一触发信号，
 *   失败自动重试 3 次，避免静默丢终态卡死调度
 * - payload 携带 event_time（UTC，毫秒 3 位固定宽度），宿主据此拒绝
 *   迟到的旧事件推送（与宿主 SQLite 毫秒格式一致，字典序比较正确）
 *
 * 注意：pi 仅在项目被信任后加载项目级扩展（--approve / trust 流程），
 * 首次启动需用户在终端确认信任。
 */

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent"

// 部署时由宿主按当前端口改写（hooks.rs replace_pi_extension_port），勿手改
const BEDCODE_PORT = 8765 // @bedcode-port
// 模板版本标记：内容升级时递增，宿主据此对旧部署副本自动重部署（hooks.rs）
// @bedcode-template-version 2

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

/**
 * subagent 子进程检测：subagent 扩展以 `pi --mode json -p --no-session`
 * 派生独立 pi 进程执行委派任务（继承 BEDCODE_SESSION_ID 并同样加载本扩展）。
 * 其会话生命周期与主会话任务无关——session_start/agent_settled/进程退出
 * 会把主任务状态污染（subagent 结束时 completed+interrupted 毫秒级连推，
 * 提前落终态且误标中断），故子进程中本扩展整体静默。
 */
const IS_SUBAGENT_PROCESS = process.argv.includes("--no-session")

/**
 * 推送任务状态到 BedCode 桌面端 HTTP API
 * 失败不阻塞主流程，仅记录日志；终态推送自动重试。
 */
async function push(status: string, reason: string): Promise<void> {
  // 仅在 BedCode 启动的 PTY 终端中生效（外部终端无此环境变量）
  const bedcodeSessionId = process.env.BEDCODE_SESSION_ID
  if (!bedcodeSessionId) return

  // subagent 子进程整体静默（见 IS_SUBAGENT_PROCESS 注释）
  if (IS_SUBAGENT_PROCESS) return

  // 宿主以 bedcode 会话 ID 键控任务行（on_input_submitted / 队列出队写入），
  // session_id 与 bedcode_session_id 同值即可命中（见 state.rs handle_update_task_status）
  const payload = {
    session_id: bedcodeSessionId,
    status,
    reason: reason || "",
    event_time: eventTime(),
    bedcode_session_id: bedcodeSessionId,
  }

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

/** 截断过长的 prompt 用于 reason */
function promptPreview(prompt: string): string {
  return prompt.length > 100 ? `${prompt.slice(0, 100)}...` : prompt
}

export default function (pi: ExtensionAPI) {
  // 会话启动 / 重建（/new 后 pi 重新加载扩展并再次触发 session_start）：
  // 推送 idle，宿主驱动 waiting 态队列任务放行
  pi.on("session_start", () => {
    void push("idle", "Session started")
  })

  // 用户提交 prompt（含队列自动投递）→ 任务进入执行状态
  pi.on("before_agent_start", (event) => {
    void push("in_progress", `User submitted: ${promptPreview(event.prompt)}`)
  })

  // 工具执行结束（成功/失败均视为任务进行中，与 PostToolUse 语义一致）
  pi.on("tool_execution_end", (event) => {
    void push("in_progress", `Tool ${event.toolName} completed`)
  })

  // run 完全收敛（自动重试/压缩/排队续跑均结束）→ 正常完成
  pi.on("agent_settled", () => {
    void push("completed", "Task completed")
  })

  // 退出 pi → 会话中断；new/fork/resume 仅切换会话（pi 仍运行），不推送
  pi.on("session_shutdown", (event) => {
    if (event.reason === "quit") {
      void push("interrupted", "Session ended: quit")
    }
  })
}
