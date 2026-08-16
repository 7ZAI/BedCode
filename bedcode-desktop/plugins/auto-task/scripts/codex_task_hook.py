#!/usr/bin/env python3
"""BedCode Codex Hooks - Hook 脚本

统一入口脚本，处理 Codex 生命周期 hook 事件，同步对话任务状态到 BedCode
桌面端（auto_task_hook.py 的 Codex 等效实现）。
跨平台（Windows/macOS/Linux），零外部依赖（仅标准库）。

部署方式：
    由宿主写入项目 `<repo>/.codex/hooks.json` + `codex_task_hook.py`
    （hooks.rs ensure_codex_hooks），Codex 从项目 `.codex/` 配置层自动发现。

用法:
    python3 codex_task_hook.py session-start        # SessionStart
    python3 codex_task_hook.py user-prompt-submit   # UserPromptSubmit
    python3 codex_task_hook.py permission-request   # PermissionRequest
    python3 codex_task_hook.py post-tool-use        # PostToolUse
    python3 codex_task_hook.py subagent-start       # SubagentStart
    python3 codex_task_hook.py subagent-stop        # SubagentStop
    python3 codex_task_hook.py stop                 # Stop
    python3 codex_task_hook.py session-end          # SessionEnd

环境变量:
    BEDCODE_PORT        - HTTP API 端口（默认 8765，由 hooks.json 命令前缀注入）
    BEDCODE_SESSION_ID  - BedCode PTY 会话 ID（由 pty_process.rs 启动时注入）

生效条件:
    仅当 BEDCODE_SESSION_ID 环境变量存在时 hook 才生效。
    BedCode 启动的 PTY 终端会自动注入此变量，外部终端不会设置，
    因此 hook 只在 BedCode 管理的终端中激活，不影响外部终端使用。

状态机（对应 auto_task_hook.py 的 Claude Code 语义）:
    SessionStart(startup/resume/clear) → idle
    UserPromptSubmit                  → in_progress
    PermissionRequest(手动模式)        → asking
    PermissionRequest(自动模式)        → 返回 allow，不推 asking（请求本不会出现）
    PostToolUse                       → in_progress（Codex 无 PostToolUseFailure，
                                        工具失败同样触发本事件）
    Stop(stop_hook_active)            → in_progress（hook 续行中）
    Stop(子 agent 运行中)              → in_progress（等价 Claude background_tasks）
    Stop(asking)                      → interrupted（用户拒绝/中断）
    Stop(其他)                        → completed
    SubagentStart / SubagentStop      → 不推送，仅维护计数
    SessionEnd                        → interrupted / 不推送（按当前状态保守判定）

    Codex 与 Claude 的差异处理:
    - Stop 载荷无 background_tasks 字段：子 agent 运行状态由
      SubagentStart/SubagentStop 本地计数文件替代（按 BEDCODE_SESSION_ID 隔离）
    - SessionEnd reason 目前只有 "other"：去掉 Claude 的 prompt_input_exit/clear
      等映射，终态/空闲一律跳过，其余保守 interrupted
    - 无 Notification / AskUserQuestion：等待用户授权由 PermissionRequest
      承担，asking 仅在手动模式推送

HTTP 说明:
    所有请求固定使用 127.0.0.1（Windows localhost 的 IPv6 回退会卡 ~2 秒，
    且 Stop 的 GET+POST 背靠背请求会因此并发竞态，completed 推送被宿主
    中间件以 Content-Type 错误静默丢弃 → 队列永不调度下一任务）。
    completed/interrupted 为调度链唯一触发信号，push 失败自动重试 3 次。

信任说明:
    Codex 的非托管 hooks 必须先经用户信任（/hooks 按 hash 审核）才会运行，
    且项目 `.codex/` 配置层本身需被信任；首次使用需用户确认一次。

模板版本标记：内容升级时递增，宿主据此对旧部署副本自动重部署（hooks.rs）。
# @bedcode-template-version 1
"""

import json
import logging
import os
import re
import sys
import tempfile
import time
from datetime import datetime, timezone
from logging.handlers import TimedRotatingFileHandler
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen

# ==================== Constants ====================

BEDCODE_PORT_DEFAULT = 8765
# 必须用 127.0.0.1 而非 localhost：Windows 上 localhost 同时解析为 ::1(IPv6) 和
# 127.0.0.1(IPv4)，而宿主服务器只绑定 IPv4 —— 每次连接先尝试 ::1 再回退，实测
# 耗时 ~2 秒；Stop hook 的 GET+POST 背靠背请求因此并发卡顿，POST 到达宿主时帧损坏
# 被中间件以 "Content type error" 静默丢弃（completed 推送丢失 → 队列永不调度下一任务）
HOST = "127.0.0.1"
HTTP_TIMEOUT_SECONDS = 3
# 终态推送（completed/interrupted）一旦丢失会中断队列调度链，必须重试保证送达
HTTP_RETRY_ATTEMPTS = 3
HTTP_RETRY_DELAY_SECONDS = 0.5
LOG_RETENTION_DAYS = 7
PLUGIN_ID = "com.bedcode.auto-task"
PLUGIN_API_PREFIX = "/api/plugin/{}".format(PLUGIN_ID)

# 任务终态集合 — 一旦进入终态，不应被其他事件降级
TERMINAL_STATUSES = {"completed", "interrupted"}

# 子 agent 计数状态目录：默认系统临时目录（按 BEDCODE_SESSION_ID 隔离），
# 测试可覆盖为临时目录；不写入项目目录，避免残留文件干扰用户工作区
STATE_DIR = None

# ==================== Logging ====================


def setup_logging():
    """配置日志系统，写入项目 .codex 目录下的日志文件。

    日志按天轮转，保留 7 天。同时输出到 stderr 供调试。
    Codex hooks 命令以会话 cwd 为工作目录运行，直接取当前目录即可。
    """
    project_dir = os.getcwd() or str(Path.home())
    log_dir = Path(project_dir) / ".codex"
    log_dir.mkdir(parents=True, exist_ok=True)
    log_file = log_dir / "bedcode-plugin.log"

    logger = logging.getLogger("bedcode-codex")
    logger.setLevel(logging.DEBUG)

    file_handler = TimedRotatingFileHandler(
        str(log_file), when="midnight", backupCount=LOG_RETENTION_DAYS, encoding="utf-8"
    )
    file_handler.setFormatter(
        logging.Formatter("[%(asctime)s] [%(levelname)s] %(message)s", datefmt="%Y-%m-%dT%H:%M:%SZ")
    )
    file_handler.formatter.converter = lambda *args: datetime.now(timezone.utc).timetuple()
    logger.addHandler(file_handler)

    stderr_handler = logging.StreamHandler(sys.stderr)
    stderr_handler.setLevel(logging.WARNING)
    stderr_handler.setFormatter(logging.Formatter("[bedcode-codex] %(message)s"))
    logger.addHandler(stderr_handler)

    return logger


# ==================== Subagent State ====================


def subagent_state_file():
    """返回当前 BedCode 会话的子 agent 计数文件路径。

    按 BEDCODE_SESSION_ID 隔离，避免同一项目多个 BedCode 会话互相干扰；
    放在系统临时目录，宿主清理集成时不触碰用户项目文件。
    """
    sid = os.environ.get("BEDCODE_SESSION_ID", "")
    safe_sid = re.sub(r"[^A-Za-z0-9_-]", "_", sid) or "unknown"
    return os.path.join(STATE_DIR or tempfile.gettempdir(), "bedcode-autotask-subagents-{}.json".format(safe_sid))


def _load_subagent_count():
    """读取当前会话的子 agent 计数（文件缺失视为 0）。"""
    try:
        with open(subagent_state_file(), encoding="utf-8") as f:
            data = json.load(f)
            return int(data.get("active", 0))
    except (OSError, ValueError, json.JSONDecodeError):
        return 0


def _save_subagent_count(count):
    """持久化当前会话的子 agent 计数（失败不阻塞主流程）。"""
    try:
        with open(subagent_state_file(), "w", encoding="utf-8") as f:
            json.dump({"active": count}, f)
    except OSError:
        pass


def active_subagents():
    """当前会话运行中的子 agent 数量。"""
    return _load_subagent_count()


# ==================== HTTP Helpers ====================


def bedcode_session_id():
    """BedCode PTY 会话 ID（宿主注入，缺失返回 None）。"""
    return os.environ.get("BEDCODE_SESSION_ID", "") or None


def push_task_status(session_id, status, reason, logger, questions=None, bedcode_session_id=None):
    """推送任务状态到 BedCode 桌面端 HTTP API。

    失败不阻塞主流程，仅记录日志；终态推送自动重试。
    session_id 与 bedcode_session_id 均取 BedCode PTY 会话 ID ——
    宿主以该 ID 键控任务行（on_input_submitted / 队列出队写入），
    与 pi/opencode 适配器同策略（见 state.rs handle_update_task_status）。
    """
    # 仅在 BedCode 启动的 PTY 终端中生效（外部终端无此环境变量）
    if not os.environ.get("BEDCODE_SESSION_ID", ""):
        return

    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://{}:{}{}/task-status".format(HOST, port, PLUGIN_API_PREFIX)

    if bedcode_session_id is None:
        bedcode_session_id = os.environ.get("BEDCODE_SESSION_ID", "") or None

    now_utc = datetime.now(timezone.utc)
    event_time = now_utc.strftime("%Y-%m-%d %H:%M:%S") + ".{:03d}".format(now_utc.microsecond // 1000)
    payload_dict = {
        "session_id": session_id,
        "status": status,
        "reason": reason or "",
        "event_time": event_time,
    }
    if bedcode_session_id:
        payload_dict["bedcode_session_id"] = bedcode_session_id
    if questions:
        payload_dict["questions"] = questions

    payload = json.dumps(payload_dict).encode("utf-8")
    logger.info("HTTP POST {} session_id={} status={}".format(url, session_id, status))

    # 终态推送（completed/interrupted）是队列调度的唯一触发信号，丢失即卡死调度链；
    # 对瞬时故障（超时/连接重置/中间件拒绝）重试，避免静默丢终态
    for attempt in range(1, HTTP_RETRY_ATTEMPTS + 1):
        try:
            req = Request(
                url,
                data=payload,
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urlopen(req, timeout=HTTP_TIMEOUT_SECONDS) as resp:
                body = resp.read().decode("utf-8")
                logger.info("HTTP response: {} {}".format(resp.status, body[:200]))
                return
        except (URLError, OSError) as e:
            if attempt < HTTP_RETRY_ATTEMPTS:
                logger.warning(
                    "HTTP push failed (attempt {}/{}): {}".format(attempt, HTTP_RETRY_ATTEMPTS, e)
                )
                time.sleep(HTTP_RETRY_DELAY_SECONDS)
            else:
                logger.warning(
                    "HTTP push failed after {} attempts: {}".format(HTTP_RETRY_ATTEMPTS, e)
                )


def query_session_mode(session_id, logger):
    """查询会话自动授权模式。

    通过 HTTP GET /api/plugin/com.bedcode.auto-task/session-mode 查询。
    返回 True 表示自动授权模式，False 表示手动模式。
    查询失败默认返回 False（手动模式，安全优先）。
    """
    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://{}:{}{}/session-mode?session_id={}".format(
        HOST, port, PLUGIN_API_PREFIX, session_id
    )

    logger.info("HTTP GET {} session_id={}".format(url, session_id))

    try:
        req = Request(url, method="GET")
        with urlopen(req, timeout=HTTP_TIMEOUT_SECONDS) as resp:
            body = resp.read().decode("utf-8")
            result = json.loads(body)
            if result.get("code") == 0 and result.get("data"):
                return result["data"].get("auto_approve", False)
            return False
    except (URLError, OSError, json.JSONDecodeError, ValueError) as e:
        logger.warning("HTTP session mode query failed: {}".format(e))
        return False


def query_task_status(session_id, logger):
    """查询当前任务状态。

    通过 HTTP GET /api/plugin/com.bedcode.auto-task/task-status 查询。
    用于终止 hook 判断当前状态，避免盲目覆盖。
    查询失败返回 None（未知状态，由调用方决定默认行为）。
    """
    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://{}:{}{}/task-status?session_id={}".format(
        HOST, port, PLUGIN_API_PREFIX, session_id
    )

    logger.debug("HTTP GET {} session_id={}".format(url, session_id))

    try:
        req = Request(url, method="GET")
        with urlopen(req, timeout=HTTP_TIMEOUT_SECONDS) as resp:
            body = resp.read().decode("utf-8")
            result = json.loads(body)
            if result.get("code") == 0 and result.get("data"):
                return result["data"].get("task_status")
            return None
    except (URLError, OSError, json.JSONDecodeError, ValueError) as e:
        logger.warning("HTTP task status query failed: {}".format(e))
        return None


# ==================== Fallback JSON Parsing ====================


def extract_fields_from_raw_json(raw_text):
    """从损坏的 JSON 文本中用正则提取关键字段。

    Stop/SubagentStop 的 stdin 可能包含超长消息导致 JSON 解析失败，
    但 session_id / stop_hook_active / hook_event_name 通常在 JSON 前部
    且格式简单，可以通过正则安全提取。
    """
    fields = {}

    m = re.search(r'"session_id"\s*:\s*"([^"]+)"', raw_text)
    if m:
        fields["session_id"] = m.group(1)

    m = re.search(r'"reason"\s*:\s*"((?:[^"\\]|\\.)*?)"', raw_text)
    if m:
        fields["reason"] = m.group(1)

    m = re.search(r'"stop_hook_active"\s*:\s*(true|false)', raw_text)
    if m:
        fields["stop_hook_active"] = m.group(1) == "true"

    m = re.search(r'"hook_event_name"\s*:\s*"([^"]+)"', raw_text)
    if m:
        fields["hook_event_name"] = m.group(1)

    return fields


# ==================== Hook Handlers ====================


def handle_session_start(data, logger):
    """处理 SessionStart 事件：推送 idle（宿主据此放行 waiting 态队列任务）。

    Codex 的 SessionStart 在 startup/resume/clear/compact 时触发；
    matcher 已限定 startup|resume|clear，compact 不会走到这里。
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("session_start: BEDCODE_SESSION_ID not set")
        return

    cwd = data.get("cwd", "")
    source = data.get("source", "")
    logger.info(
        "HOOK session_start: bedcode_sid={} project={} source={}".format(
            sid, cwd, source
        )
    )

    push_task_status(sid, "idle", "Session started", logger, bedcode_session_id=sid)

    output = {
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": "BedCode plugin active",
            "sessionTitle": Path(cwd).name if cwd else "bedcode",
        }
    }
    print(json.dumps(output))


def handle_user_prompt_submit(data, logger):
    """处理 UserPromptSubmit 事件：任务进入执行状态。"""
    sid = bedcode_session_id()
    if not sid:
        logger.error("user_prompt_submit: BEDCODE_SESSION_ID not set")
        return

    prompt = data.get("prompt", "")
    prompt_preview = prompt[:100] + "..." if len(prompt) > 100 else prompt
    logger.info(
        "HOOK user_prompt_submit: bedcode_sid={} prompt={}".format(sid, prompt_preview)
    )

    push_task_status(
        sid, "in_progress", "User submitted: {}".format(prompt_preview), logger,
        bedcode_session_id=sid,
    )


def _permission_questions(data):
    """把 PermissionRequest 输入构造为 PluginQuestion 兼容格式。

    Codex 权限请求没有 Claude 那样的选项结构，按 notification 的
    Allow/Deny 双选项构造，复用宿主提问面板 DTO。
    """
    tool_name = data.get("tool_name", "")
    tool_input = data.get("tool_input") or {}
    description = (tool_input.get("description") or "").strip()
    question = description or "Permission requested: {}".format(tool_name)
    return [
        {
            "question": question,
            "header": "codex permission",
            "multi_select": False,
            "options": [
                {"label": "Allow", "description": "Approve the request"},
                {"label": "Deny", "description": "Reject the request"},
            ],
        }
    ]


def handle_permission_request(data, logger):
    """处理 PermissionRequest 事件。

    Codex 在将要向用户请求审批时触发（如 shell 提权、托管网络审批）：
    - 自动模式：返回 decision.allow 直接放行，不推 asking
      （请求本不会出现，推 asking 会把运行中任务误标为等待输入）
    - 手动模式：推 asking 并保持静默，走 Codex 原生审批流程
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("permission_request: BEDCODE_SESSION_ID not set")
        return

    tool_name = data.get("tool_name", "")
    auto_approve = query_session_mode(sid, logger)

    if not auto_approve:
        questions = _permission_questions(data)
        reason = "Waiting for user permission: {}".format(tool_name)
        push_task_status(
            sid, "asking", reason, logger, questions=questions,
            bedcode_session_id=sid,
        )
        logger.info(
            "permission_request: manual mode, pushed asking status, no auto-approve"
        )
        return

    output = {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": "allow"},
        }
    }
    logger.info(
        "permission_request: auto-approve tool={}".format(tool_name)
    )
    print(json.dumps(output))


def handle_post_tool_use(data, logger):
    """处理 PostToolUse 事件：工具执行结束（成功/失败均触发），任务仍在进行。"""
    sid = bedcode_session_id()
    if not sid:
        logger.error("post_tool_use: BEDCODE_SESSION_ID not set")
        return

    tool_name = data.get("tool_name", "")
    logger.info("HOOK post_tool_use: bedcode_sid={} tool={}".format(sid, tool_name))

    push_task_status(
        sid, "in_progress", "Tool {} completed".format(tool_name), logger,
        bedcode_session_id=sid,
    )


def handle_subagent_start(data, logger):
    """处理 SubagentStart 事件：子 agent 计数 +1，不推送状态。

    子 agent 运行期间主 turn 可能提前结束触发 Stop，若此时推 completed
    会提前终态：任务行被宿主终态保护锁死，真正的完成再也无法同步。
    计数由 Stop 判定消费（等价 Claude Stop 载荷的 background_tasks）。
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("subagent_start: BEDCODE_SESSION_ID not set")
        return

    count = _load_subagent_count()
    _save_subagent_count(count + 1)
    logger.info(
        "HOOK subagent_start: bedcode_sid={} agent_type={} active={}".format(
            sid, data.get("agent_type", ""), count + 1
        )
    )


def handle_subagent_stop(data, logger):
    """处理 SubagentStop 事件：子 agent 计数 -1，不推送状态。

    子 agent 成功/失败均不代表主任务状态，主任务终态只由 Stop / SessionEnd 判定。
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("subagent_stop: BEDCODE_SESSION_ID not set")
        return

    count = max(0, _load_subagent_count() - 1)
    _save_subagent_count(count)
    logger.info(
        "HOOK subagent_stop: bedcode_sid={} agent_type={} active={}".format(
            sid, data.get("agent_type", ""), count
        )
    )


def handle_stop(data, logger):
    """处理 Stop 事件，判定主任务终态。

    优先级：
    - stop_hook_active=true → in_progress（hook 续行中）
    - 子 agent 计数 > 0 → in_progress（会话暂停等待子 agent，任务未结束）
    - 当前 asking → interrupted（用户在等待输入时停止 = 拒绝/中断）
    - 当前终态 → 跳过（Stop 是冗余信号）
    - 其他 → completed（正常完成）
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("stop: BEDCODE_SESSION_ID not set")
        return

    stop_hook_active = data.get("stop_hook_active", False)
    if stop_hook_active:
        status = "in_progress"
        reason = "Stop hook triggered continuation"
    else:
        running_subagents = active_subagents()
        if running_subagents > 0:
            status = "in_progress"
            reason = "Stopped, {} subagent(s) still running".format(running_subagents)
            logger.info(
                "HOOK stop: bedcode_sid={} subagents still running, task continues".format(sid)
            )
        else:
            current = query_task_status(sid, logger)
            if current in TERMINAL_STATUSES:
                logger.info(
                    "HOOK stop: bedcode_sid={} skipped, already in terminal status '{}'".format(
                        sid, current
                    )
                )
                return
            elif current == "asking":
                status = "interrupted"
                reason = "Stopped while waiting for user input"
            else:
                status = "completed"
                reason = "Task completed"

    logger.info(
        "HOOK stop: bedcode_sid={} status={} stop_hook_active={}".format(
            sid, status, stop_hook_active
        )
    )
    push_task_status(sid, status, reason, logger, bedcode_session_id=sid)


def handle_session_end(data, logger):
    """处理 SessionEnd 事件：会话结束时按当前状态保守判定。

    Codex 的 SessionEnd reason 目前只有 "other"（无 Claude 的
    prompt_input_exit/clear/logout 细分）：
    - 当前终态 → 跳过（Stop 已正确标记）
    - 当前 idle / 查询失败 → 跳过（无任务运行）
    - 其他（in_progress/asking）→ interrupted（会话异常退出）
    """
    sid = bedcode_session_id()
    if not sid:
        logger.error("session_end: BEDCODE_SESSION_ID not set")
        return

    reason = data.get("reason", "other")
    current = query_task_status(sid, logger)
    if current in TERMINAL_STATUSES or current in ("idle", None):
        logger.info(
            "HOOK session_end: bedcode_sid={} reason={} skipped, current status '{}' "
            "is terminal or idle".format(sid, reason, current)
        )
        return

    status = "interrupted"
    status_reason = "Session ended: {} (current: {})".format(reason, current)
    logger.info(
        "HOOK session_end: bedcode_sid={} reason={} status={}".format(
            sid, reason, status
        )
    )
    push_task_status(sid, status, status_reason, logger, bedcode_session_id=sid)


# ==================== Main ====================

COMMAND_HANDLERS = {
    "session-start": handle_session_start,
    "user-prompt-submit": handle_user_prompt_submit,
    "permission-request": handle_permission_request,
    "post-tool-use": handle_post_tool_use,
    "subagent-start": handle_subagent_start,
    "subagent-stop": handle_subagent_stop,
    "stop": handle_stop,
    "session-end": handle_session_end,
}


def main():
    # Windows 中文系统默认编码为 GBK/CP936，而 Codex 通过 stdin 传入 UTF-8 编码的 JSON。
    # 必须在读取 stdin 之前强制 UTF-8，否则中文字符会被错误解码为乱码
    if sys.platform == "win32":
        sys.stdin.reconfigure(encoding="utf-8")
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")

    if len(sys.argv) < 2:
        print("Usage: python3 codex_task_hook.py <command>", file=sys.stderr)
        print("Commands: {}".format(", ".join(COMMAND_HANDLERS.keys())), file=sys.stderr)
        sys.exit(1)

    # 仅在 BedCode 启动的 PTY 终端中生效
    # BEDCODE_SESSION_ID 由 pty_process.rs 启动时注入，外部终端不会设置此变量
    if not os.environ.get("BEDCODE_SESSION_ID", ""):
        sys.exit(0)

    command = sys.argv[1]
    if command not in COMMAND_HANDLERS:
        print("Unknown command: {}. Available: {}".format(command, ", ".join(COMMAND_HANDLERS.keys())), file=sys.stderr)
        sys.exit(1)

    logger = setup_logging()

    try:
        raw_input = sys.stdin.read()
        data = json.loads(raw_input) if raw_input.strip() else {}
    except json.JSONDecodeError as e:
        logger.error("JSON parse error: {}, input length={}".format(e, len(raw_input) if raw_input else 0))
        data = extract_fields_from_raw_json(raw_input) if raw_input else {}
        if data:
            logger.info("Extracted fields from raw JSON: {}".format(list(data.keys())))

    COMMAND_HANDLERS[command](data, logger)


if __name__ == "__main__":
    main()
