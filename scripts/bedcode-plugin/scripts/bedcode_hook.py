#!/usr/bin/env python3
"""BedCode Claude Code Plugin - Hook 脚本

统一入口脚本，处理 SessionStart / Stop / SubagentStop 事件。
跨平台（Windows/macOS/Linux），零外部依赖（仅标准库）。

用法:
    python3 bedcode_hook.py session-start   # SessionStart hook
    python3 bedcode_hook.py write-event     # Stop / SubagentStop hook

环境变量:
    CLAUDE_PROJECT_DIR  - 项目根目录（Claude Code 自动设置）
    CLAUDE_PLUGIN_ROOT  - 插件根目录（Claude Code 自动设置）
    BEDCODE_TOKEN       - HTTP API 认证 token（存在时才推送状态）
    BEDCODE_PORT        - HTTP API 端口（默认 8080）
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


# ==================== HTTP Push ====================


def push_task_status(session_id, status, reason, logger):
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

    payload = json.dumps({
        "session_id": session_id,
        "status": status,
        "reason": reason or "",
        "token": token,
    }).encode("utf-8")

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
        print("Usage: python3 bedcode_hook.py <session-start|write-event>", file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    if command not in ("session-start", "write-event"):
        print("Unknown command: {}. Use session-start or write-event".format(command), file=sys.stderr)
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
    elif command == "write-event":
        handle_write_event(data, logger)


if __name__ == "__main__":
    main()
