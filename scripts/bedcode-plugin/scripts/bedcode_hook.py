#!/usr/bin/env python3
"""BedCode Claude Code Plugin - Hook 脚本

统一入口脚本，处理 SessionStart / PreToolUse / Stop / SubagentStop 事件。
跨平台（Windows/macOS/Linux），零外部依赖（仅标准库）。

用法:
    python3 bedcode_hook.py session-start       # SessionStart hook
    python3 bedcode_hook.py pre-tool-use        # PreToolUse hook (权限请求 + AskUserQuestion)
    python3 bedcode_hook.py write-event         # Stop / SubagentStop hook

环境变量:
    CLAUDE_PROJECT_DIR  - 项目根目录（Claude Code 自动设置）
    CLAUDE_PLUGIN_ROOT  - 插件根目录（Claude Code 自动设置）
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
        # fallback: 尝试从插件根目录推断
        plugin_root = os.environ.get("CLAUDE_PLUGIN_ROOT", "")
        if plugin_root:
            project_dir = str(Path(plugin_root).parent.parent)
        else:
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


def push_task_status(session_id, status, reason, logger, questions=None):
    """推送任务状态到 BedCode 桌面端 HTTP API。

    仅在 BEDCODE_TOKEN 环境变量存在时推送。
    失败不阻塞主流程，仅记录日志。
    """
    token = os.environ.get("BEDCODE_TOKEN", "")
    if not token:
        logger.debug("BEDCODE_TOKEN not set, skip HTTP push")
        return

    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://localhost:{}/api/plugin/task-status".format(port)

    payload_dict = {
        "session_id": session_id,
        "status": status,
        "reason": reason or "",
        "token": token,
    }
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
    url = "http://localhost:{}/api/plugin/session-mode?session_id={}&token={}".format(
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


# ==================== Status Parsing ====================


def parse_status_from_prompt_result(tool_result):
    """从 prompt hook 返回的 tool_result 中提取 status。

    prompt hook 返回格式如: {"status": "completed", "reason": "Task done"}
    可能被 markdown 代码块包裹，需要提取 JSON 部分。
    """
    if not tool_result or not isinstance(tool_result, str):
        return None, None

    # 尝试提取 JSON（可能被 ```json ... ``` 包裹）
    json_match = re.search(r'\{[^}]*"status"[^}]*\}', tool_result)
    if not json_match:
        return None, None

    try:
        parsed = json.loads(json_match.group(0))
        status = parsed.get("status", "")
        reason = parsed.get("reason", "")
        if status in VALID_STATUSES:
            return status, reason
    except (json.JSONDecodeError, ValueError):
        pass

    return None, None


def infer_status_from_reason(reason):
    """根据 Claude Code 的 reason 字段推断任务状态。

    Claude Code Stop hook 的 reason 字段取值:
    - "complete" / ":complete" → 任务完成
    - "tool_use" → 工具调用中
    - 其他 → 默认 in_progress
    """
    if not reason:
        return "in_progress", "Unknown reason"

    if reason in ("complete", ":complete"):
        return "completed", "Task completed"
    if reason == "tool_use":
        return "in_progress", "Tool use in progress"

    return "in_progress", reason


# ==================== Hook Handlers ====================


def handle_session_start(data, logger):
    """处理 SessionStart 事件。

    记录会话启动信息，推送 idle 状态到桌面端。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("session_start: missing session_id")
        sys.exit(2)

    cwd = data.get("cwd", "")
    source = data.get("source", "")
    permission_mode = data.get("permission_mode", "")

    logger.info(
        "HOOK session_start: session_id={} project={} source={} permission={}".format(
            session_id, cwd, source, permission_mode
        )
    )

    # SessionStart 时推送 idle 状态
    push_task_status(session_id, "idle", "Session started", logger)

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

    检查会话是否开启自动授权模式：
    - 自动模式 + AskUserQuestion：自动选择推荐选项并返回 permissionDecision: "allow"
    - 自动模式 + 其他工具：直接返回 permissionDecision: "allow"
    - 手动模式：不干预，走 Claude Code 原生交互流程
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

    if not auto_approve:
        # 手动模式：不输出任何内容，走 Claude Code 原生交互
        logger.info("pre_tool_use: manual mode, no auto-approve")
        return

    # 自动授权模式
    if tool_name == "AskUserQuestion":
        # AskUserQuestion：构造 answers，选推荐选项（第一个选项）
        tool_input = data.get("tool_input", {})
        questions = tool_input.get("questions", [])
        answers = {}

        for q in questions:
            header = q.get("header", "")
            options = q.get("options", [])
            if options:
                # 选择第一个选项（Claude Code 推荐项）
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

        # 同时推送 asking 状态到桌面端（保留任务状态通知链路）
        reason = "Auto-answered by BedCode"
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
    else:
        # 其他工具：直接允许
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
    优先使用 prompt hook 的分析结果，fallback 到 reason 字段推断。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("write_event: missing session_id")
        sys.exit(2)

    hook_event = data.get("hook_event_name", "Stop")
    reason = data.get("reason", "")
    tool_result = data.get("tool_result", "")

    # 确定事件类型
    event_type = "subagent_stop" if hook_event == "SubagentStop" else "stop"

    # 优先从 prompt hook 结果解析状态
    status, status_reason = parse_status_from_prompt_result(tool_result)

    # Fallback: 从 reason 字段推断
    if not status:
        status, status_reason = infer_status_from_reason(reason)

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

    # 从 stdin 读取 hook 输入
    try:
        raw_input = sys.stdin.read()
        data = json.loads(raw_input) if raw_input.strip() else {}
    except json.JSONDecodeError as e:
        print("Invalid JSON input: {}".format(e), file=sys.stderr)
        sys.exit(2)

    logger = setup_logging()

    if command == "session-start":
        handle_session_start(data, logger)
    elif command == "pre-tool-use":
        handle_pre_tool_use(data, logger)
    elif command == "write-event":
        handle_write_event(data, logger)


if __name__ == "__main__":
    main()
