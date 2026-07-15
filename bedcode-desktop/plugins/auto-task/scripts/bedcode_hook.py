#!/usr/bin/env python3
"""BedCode Claude Code Hooks - Hook 脚本

统一入口脚本，处理 SessionStart / PreToolUse / Stop / SubagentStop 事件。
跨平台（Windows/macOS/Linux），零外部依赖（仅标准库）。

用法:
    python3 bedcode_hook.py session-start       # SessionStart hook
    python3 bedcode_hook.py pre-tool-use        # PreToolUse hook (权限请求 + AskUserQuestion)
    python3 bedcode_hook.py write-event         # Stop / SubagentStop hook

环境变量:
    CLAUDE_PROJECT_DIR  - 项目根目录（Claude Code 自动设置）
    BEDCODE_TOKEN       - HTTP API 认证 token（存在时才推送状态）
    BEDCODE_PORT        - HTTP API 端口（默认 8765）
"""

import json
import logging
import os
import re
import sys
from datetime import datetime, timezone
from logging.handlers import TimedRotatingFileHandler
from pathlib import Path
from urllib.request import Request, urlopen
from urllib.error import URLError

# ==================== Constants ====================

BEDCODE_PORT_DEFAULT = 8765
HTTP_TIMEOUT_SECONDS = 3
LOG_RETENTION_DAYS = 7
VALID_STATUSES = {"idle", "in_progress", "asking", "completed", "interrupted"}

# ==================== Logging ====================


def setup_logging():
    """配置日志系统，写入项目 .claude 目录下的日志文件。

    日志按天轮转，保留 7 天。同时输出到 stderr 供调试。
    """
    project_dir = os.environ.get("CLAUDE_PROJECT_DIR", "")
    if not project_dir:
        project_dir = str(Path.home())

    log_dir = Path(project_dir) / ".claude"
    log_dir.mkdir(parents=True, exist_ok=True)
    log_file = log_dir / "bedcode-plugin.log"

    logger = logging.getLogger("bedcode")
    logger.setLevel(logging.DEBUG)

    # 文件 handler：按天轮转
    file_handler = TimedRotatingFileHandler(
        str(log_file), when="midnight", backupCount=LOG_RETENTION_DAYS, encoding="utf-8"
    )
    file_handler.setFormatter(
        logging.Formatter("[%(asctime)s] [%(levelname)s] %(message)s", datefmt="%Y-%m-%dT%H:%M:%SZ")
    )
    file_handler.formatter.converter = lambda *args: datetime.now(timezone.utc).timetuple()
    logger.addHandler(file_handler)

    # stderr handler：供 Claude Code 调试日志捕获
    stderr_handler = logging.StreamHandler(sys.stderr)
    stderr_handler.setLevel(logging.WARNING)
    stderr_handler.setFormatter(logging.Formatter("[bedcode] %(message)s"))
    logger.addHandler(stderr_handler)

    return logger


# ==================== HTTP Helpers ====================


def push_task_status(session_id, status, reason, logger, questions=None, bedcode_session_id=None):
    """推送任务状态到 BedCode 桌面端 HTTP API。

    仅在 BEDCODE_TOKEN 环境变量存在时推送。
    失败不阻塞主流程，仅记录日志。
    """
    token = os.environ.get("BEDCODE_TOKEN", "")
    if not token:
        logger.debug("BEDCODE_TOKEN not set, skip HTTP push")
        return

    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://localhost:{}/plugin/task-status".format(port)

    payload_dict = {
        "session_id": session_id,
        "status": status,
        "reason": reason or "",
        "token": token,
    }
    # BedCode PTY 会话 ID：用于关联 Claude Code session 和 BedCode PTY session
    if bedcode_session_id:
        payload_dict["bedcode_session_id"] = bedcode_session_id
    if questions:
        payload_dict["questions"] = questions

    payload = json.dumps(payload_dict).encode("utf-8")

    logger.info("HTTP POST {} session_id={} status={}".format(url, session_id, status))

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
    except (URLError, OSError) as e:
        logger.warning("HTTP push failed: {}".format(e))


def query_session_mode(session_id, logger):
    """查询会话自动授权模式。

    通过 HTTP GET /api/plugin/session-mode 查询。
    返回 True 表示自动授权模式，False 表示手动模式。
    查询失败默认返回 False（手动模式，安全优先）。
    """
    token = os.environ.get("BEDCODE_TOKEN", "")
    if not token:
        logger.debug("BEDCODE_TOKEN not set, skip session mode query")
        return False

    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://localhost:{}/plugin/session-mode?session_id={}&token={}".format(
        port, session_id, token
    )

    logger.info("HTTP GET {} session_id={}".format(url, session_id))

    try:
        req = Request(url, method="GET")
        with urlopen(req, timeout=HTTP_TIMEOUT_SECONDS) as resp:
            body = resp.read().decode("utf-8")
            logger.info("HTTP response: {} {}".format(resp.status, body[:200]))
            result = json.loads(body)
            # 解析响应：ApiResponse { code: 0, data: { session_id, auto_approve } }
            if result.get("code") == 0 and result.get("data"):
                return result["data"].get("auto_approve", False)
            return False
    except (URLError, OSError, json.JSONDecodeError, ValueError) as e:
        logger.warning("HTTP session mode query failed: {}".format(e))
        return False


# ==================== Fallback JSON Parsing ====================


def extract_fields_from_raw_json(raw_text):
    """从损坏的 JSON 文本中用正则提取关键字段。

    Stop/SubagentStop 的 stdin 可能包含超长 transcript 导致 JSON 解析失败，
    但 session_id / reason / stop_hook_active 通常在 JSON 前部且格式简单，
    可以通过正则安全提取。
    """
    fields = {}

    # session_id: UUID 格式，8-4-4-4-12 hex chars
    m = re.search(r'"session_id"\s*:\s*"([0-9a-f-]{36})"', raw_text)
    if m:
        fields["session_id"] = m.group(1)

    # reason: 字符串值（可能包含转义字符，取到引号前）
    m = re.search(r'"reason"\s*:\s*"((?:[^"\\]|\\.)*?)"', raw_text)
    if m:
        fields["reason"] = m.group(1).replace("\\n", "\n").replace("\\t", "\t").replace("\\\"", "\"")

    # stop_hook_active: boolean
    m = re.search(r'"stop_hook_active"\s*:\s*(true|false)', raw_text)
    if m:
        fields["stop_hook_active"] = m.group(1) == "true"

    # hook_event_name: 字符串（Stop 或 SubagentStop）
    m = re.search(r'"hook_event_name"\s*:\s*"((?:[^"\\]|\\.)*?)"', raw_text)
    if m:
        fields["hook_event_name"] = m.group(1)

    return fields


# ==================== Status Parsing ====================


def infer_status_from_reason(reason):
    """根据 Claude Code 的 reason 字段推断任务状态。

    Claude Code Stop hook 的 reason 字段取值:
    - "complete" / ":complete" → 任务完成
    - "tool_use" → 工具调用中
    - 其他 → 默认 completed（Stop 事件触发时通常任务已结束）
    """
    if not reason:
        return "completed", "Task stopped"

    if reason in ("complete", ":complete"):
        return "completed", "Task completed"
    if reason == "tool_use":
        return "in_progress", "Tool use in progress"

    return "completed", reason


# ==================== Hook Handlers ====================


def handle_session_start(data, logger):
    """处理 SessionStart 事件。

    记录会话启动信息，推送 idle 状态到桌面端。
    同时读取 BEDCODE_SESSION_ID 环境变量建立 Claude Code session 与 BedCode PTY session 的映射。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("session_start: missing session_id")
        sys.exit(2)

    cwd = data.get("cwd", "")
    source = data.get("source", "")
    permission_mode = data.get("permission_mode", "")

    # 读取 BedCode PTY 会话 ID（由 pty_process.rs 启动时注入）
    bedcode_session_id = os.environ.get("BEDCODE_SESSION_ID", "")

    logger.info(
        "HOOK session_start: session_id={} bedcode_sid={} project={} source={} permission={}".format(
            session_id, bedcode_session_id or "N/A", cwd, source, permission_mode
        )
    )

    # SessionStart 时推送 idle 状态，携带 BedCode PTY 会话 ID
    push_task_status(session_id, "idle", "Session started", logger, bedcode_session_id=bedcode_session_id or None)

    # SessionStart hook 可返回 JSON 提供额外上下文
    output = {
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": "BedCode plugin active",
            "sessionTitle": Path(cwd).name if cwd else "bedcode",
        }
    }
    print(json.dumps(output))


def handle_pre_tool_use(data, logger):
    """处理 PreToolUse 事件。

    查询会话自动授权模式：
    - 自动模式 + AskUserQuestion：自动选择推荐选项并返回 permissionDecision: "allow"
    - 自动模式 + 其他工具：直接返回 permissionDecision: "allow"
    - 手动模式：不干预工具调用，但仍推送 asking 状态到桌面端同步任务进度
    """
    session_id = data.get("session_id", "")
    tool_name = data.get("tool_name", "")

    if not session_id:
        logger.error("pre_tool_use: missing session_id")
        return

    logger.info(
        "HOOK pre_tool_use: session_id={} tool_name={}".format(session_id, tool_name)
    )

    # 查询会话自动授权模式
    auto_approve = query_session_mode(session_id, logger)

    # AskUserQuestion 时始终推送 asking 状态到桌面端（无论手动/自动模式）
    if tool_name == "AskUserQuestion":
        tool_input = data.get("tool_input", {})
        questions = tool_input.get("questions", [])

        # 推送 asking 状态到桌面端
        reason = "Auto-answered by BedCode" if auto_approve else "Waiting for user input"
        questions_data = []
        for q in questions:
            question = {
                "question": q.get("question", ""),
                "header": q.get("header", ""),
                "multi_select": q.get("multiSelect", False),
                "options": [],
            }
            for opt in q.get("options", []):
                question["options"].append({
                    "label": opt.get("label", ""),
                    "description": opt.get("description", ""),
                })
            questions_data.append(question)
        push_task_status(session_id, "asking", reason, logger, questions=questions_data)

        if not auto_approve:
            # 手动模式：不干预，走 Claude Code 原生交互
            logger.info("pre_tool_use: manual mode, pushed asking status, no auto-approve")
            return

        # 自动模式：构造 answers，选推荐选项
        answers = {}
        for q in questions:
            header = q.get("header", "")
            options = q.get("options", [])
            if options:
                answers[header] = options[0].get("label", "")

        output = {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": {
                    **tool_input,
                    "answers": answers,
                },
            }
        }
        logger.info(
            "pre_tool_use: auto-approve AskUserQuestion, answers={}".format(answers)
        )
        print(json.dumps(output))
    else:
        # 非 AskUserQuestion 工具
        if not auto_approve:
            # 手动模式：不干预，走 Claude Code 原生交互
            logger.info("pre_tool_use: manual mode, tool={}, no auto-approve".format(tool_name))
            return

        # 自动模式：直接允许
        output = {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "BedCode auto-approve mode",
            }
        }
        logger.info(
            "pre_tool_use: auto-approve tool={}".format(tool_name)
        )
        print(json.dumps(output))


def handle_write_event(data, logger):
    """处理 Stop / SubagentStop 事件。

    解析任务状态并推送到桌面端。
    从 reason 字段和 last_assistant_message 推断状态。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("write_event: missing session_id, data keys={}".format(list(data.keys())))
        sys.exit(2)

    hook_event = data.get("hook_event_name", "Stop")
    reason = data.get("reason", "")
    stop_hook_active = data.get("stop_hook_active", False)

    # 确定事件类型
    event_type = "subagent_stop" if hook_event == "SubagentStop" else "stop"

    # 从 reason 字段推断状态
    status, status_reason = infer_status_from_reason(reason)

    # stop_hook_active=true 表示 hook 已触发过续行，说明任务仍在进行
    if stop_hook_active:
        status = "in_progress"
        status_reason = "Stop hook triggered continuation"

    logger.info(
        "HOOK {}: session_id={} status={} reason={}".format(
            event_type, session_id, status, status_reason
        )
    )

    # 推送状态到桌面端
    push_task_status(session_id, status, status_reason, logger)


# ==================== Main ====================


def main():
    if len(sys.argv) < 2:
        print("Usage: python3 bedcode_hook.py <session-start|pre-tool-use|write-event>", file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    if command not in ("session-start", "pre-tool-use", "write-event"):
        print("Unknown command: {}. Use session-start, pre-tool-use, or write-event".format(command), file=sys.stderr)
        sys.exit(1)

    # 先初始化日志，确保异常处理中可用
    logger = setup_logging()

    # 从 stdin 读取 hook 输入
    try:
        raw_input = sys.stdin.read()
        data = json.loads(raw_input) if raw_input.strip() else {}
    except json.JSONDecodeError as e:
        # Stop/SubagentStop 的 stdin 可能包含超长 transcript 导致解析失败
        # 用正则从损坏的 JSON 中提取关键字段，避免丢失 session_id
        logger.error("JSON parse error: {}, input length={}".format(e, len(raw_input) if raw_input else 0))
        data = extract_fields_from_raw_json(raw_input) if raw_input else {}
        if data:
            logger.info("Extracted fields from raw JSON: {}".format(list(data.keys())))

    if command == "session-start":
        handle_session_start(data, logger)
    elif command == "pre-tool-use":
        handle_pre_tool_use(data, logger)
    elif command == "write-event":
        handle_write_event(data, logger)


if __name__ == "__main__":
    main()
