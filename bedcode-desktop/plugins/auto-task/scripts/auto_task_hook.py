#!/usr/bin/env python3
"""BedCode Claude Code Hooks - Hook 脚本

统一入口脚本，处理所有 Claude Code hook 事件，同步对话任务状态到 BedCode 桌面端。
跨平台（Windows/macOS/Linux），零外部依赖（仅标准库）。

用法:
    python3 auto_task_hook.py session-start       # SessionStart hook
    python3 auto_task_hook.py user-prompt-submit  # UserPromptSubmit hook
    python3 auto_task_hook.py pre-tool-use        # PreToolUse hook (权限请求 + AskUserQuestion)
    python3 auto_task_hook.py post-tool-use       # PostToolUse hook
    python3 auto_task_hook.py post-tool-use-fail  # PostToolUseFailure hook
    python3 auto_task_hook.py notification        # Notification hook
    python3 auto_task_hook.py stop                # Stop hook
    python3 auto_task_hook.py subagent-stop       # SubagentStop hook
    python3 auto_task_hook.py session-end         # SessionEnd hook

环境变量:
    CLAUDE_PROJECT_DIR  - 项目根目录（Claude Code 自动设置）
    BEDCODE_PORT        - HTTP API 端口（默认 8765）
    BEDCODE_SESSION_ID  - BedCode PTY 会话 ID（由 pty_process.rs 启动时注入）

生效条件:
    仅当 BEDCODE_SESSION_ID 环境变量存在时 hook 才生效。
    BedCode 启动的 PTY 终端会自动注入此变量，外部终端不会设置，
    因此 hook 只在 BedCode 管理的终端中激活，不影响外部终端使用。

状态机:
    SessionStart        → idle
    UserPromptSubmit    → in_progress
    PreToolUse(AskUser) → asking
    Notification(perm)  → asking
    Notification(idle_prompt) → 不推送（响应完毕，终态由 Stop hook 负责）
    PostToolUse         → in_progress
    PostToolUseFailure  → in_progress / interrupted
    Stop                → completed / interrupted（根据当前状态判断）；
                          background_tasks 有 running 项（v2.1.145+）→ in_progress
                          （会话暂停等待后台任务完成，任务未结束）
    SubagentStop        → 不推送（子 agent 成功/失败均不影响主任务状态，仅记录日志）
    SessionEnd          → interrupted / 不推送（根据 reason 和当前状态判断）

    注意:
        background_tasks 判定依赖 Claude Code ≥ 2.1.145（Stop 载荷携带该字段，
        用于区分"会话真正结束"与"暂停等待后台任务"——子 agent / 后台 shell 运行中
        主 turn 会提前结束并触发 Stop）。低于此版本的载荷无该字段，判定自动回退
        为原行为（存在主任务未结束时提前 completed 的风险，宿主终态保护会锁死
        任务行导致真实完成无法再同步）。

HTTP 说明:
    所有请求固定使用 127.0.0.1（Windows localhost 的 IPv6 回退会卡 ~2 秒，
    且 Stop 的 GET+POST 背靠背请求会因此并发竞态，completed 推送被宿主
    中间件以 Content-Type 错误静默丢弃 → 队列永不调度下一任务）。
    completed/interrupted 为调度链唯一触发信号，push 失败自动重试 3 次。

终态保护:
    completed / interrupted 为终态，后续事件不覆盖（防止 Stop 与 SessionEnd 冲突）
    idle 状态下 SessionEnd 不推送终态（无任务运行）
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
VALID_STATUSES = {"idle", "in_progress", "asking", "completed", "interrupted"}

# 任务终态集合 — 一旦进入终态，不应被其他事件降级
TERMINAL_STATUSES = {"completed", "interrupted"}

# SessionEnd reason → 任务状态映射
# prompt_input_exit: 用户在输入框退出（Ctrl+C / Esc）
# clear: /clear 命令清空对话
# resume: 会话恢复（非终止，不推送状态）
# logout: 用户登出
# bypass_permissions_disabled: 权限模式切换
# other: 其他原因（可能是正常退出，需根据当前状态判断）
SESSION_END_INTERRUPT_REASONS = {"prompt_input_exit", "clear", "logout", "bypass_permissions_disabled"}

# Notification type → 任务状态映射
# permission_prompt: Claude 等待权限确认
# elicitation_dialog: Claude 弹出交互对话框
# idle_prompt: Claude 响应完毕、空闲等待下一条输入 —— 不属于"向用户提问"，
#              终态由 Stop hook 负责；若在此推 asking 会命中最新一行任务
#              （可能是队列刚调度的下一个任务，in_progress 非终态不受保护），
#              把运行中的任务错误显示为"等待输入"，故单独跳过状态推送
NOTIFICATION_ASKING_TYPES = {"permission_prompt", "elicitation_dialog"}
# 响应完成但空闲等待输入的 notification 类型（仅记录日志，不推送状态）
NOTIFICATION_IDLE_TYPES = {"idle_prompt"}

# AskUserQuestion 自动回复策略：推荐标记关键词（不区分大小写）
# Claude Code 在推荐选项的 label 中会包含这些关键词，如 "Yes (Recommended)"
RECOMMENDED_KEYWORDS = ["(recommended)", "(推荐)", "(首选)"]

# 自动回复 fallback 文本：无选项时的自由回答
AUTO_ANSWER_FALLBACK = "Proceed with best practice"

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

    失败不阻塞主流程，仅记录日志。

    注意：不再携带任务名称/内容 —— 任务记录的创建已移交宿主的
    on_input_submitted 会话扩展方法（避免 Claude Code 输入 hook 双重写表），
    此接口只负责更新已有任务行的状态。

    session_id 是 Claude Code 会话 ID；bedcode_session_id 是 BedCode PTY 会话 ID
    （由宿主注入环境变量，默认读取 BEDCODE_SESSION_ID）。两者同时携带，
    宿主才能把状态更新关联到 on_input_submitted 创建的任务行
    （该行以 bedcode 会话 ID 作为 session_id 键控）。
    """
    port = os.environ.get("BEDCODE_PORT", str(BEDCODE_PORT_DEFAULT))
    url = "http://{}:{}{}/task-status".format(HOST, port, PLUGIN_API_PREFIX)

    # 未显式传入时从环境读取 BedCode PTY 会话 ID：
    # 除 SessionStart 外的所有事件（UserPromptSubmit/PreToolUse/Stop/SessionEnd）
    # 都必须携带，否则宿主无法定位任务行
    if bedcode_session_id is None:
        bedcode_session_id = os.environ.get("BEDCODE_SESSION_ID", "") or None

    # 事件发生时刻（UTC，毫秒精度、固定宽度）：
    # 与宿主 SQLite strftime('%Y-%m-%d %H:%M:%S.%f')（毫秒 3 位）格式完全一致，
    # 字符串字典序比较绝对正确。宿主据此拒绝迟到的旧事件推送 ——
    # HTTP 推送可能被阻塞乱序到达，若无此字段，旧状态可能覆盖已应用的新状态
    now_utc = datetime.now(timezone.utc)
    event_time = now_utc.strftime("%Y-%m-%d %H:%M:%S") + ".{:03d}".format(now_utc.microsecond // 1000)
    payload_dict = {
        "session_id": session_id,
        "status": status,
        "reason": reason or "",
        "event_time": event_time,
    }
    # BedCode PTY 会话 ID：用于关联 Claude Code session 和 BedCode PTY session
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
            logger.info("HTTP response: {} {}".format(resp.status, body[:200]))
            result = json.loads(body)
            # 解析响应：ApiResponse { code: 0, data: { session_id, auto_approve } }
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

    # hook_event_name: 字符串
    m = re.search(r'"hook_event_name"\s*:\s*"((?:[^"\\]|\\.)*?)"', raw_text)
    if m:
        fields["hook_event_name"] = m.group(1)

    # notification_type: 字符串
    m = re.search(r'"notification_type"\s*:\s*"((?:[^"\\]|\\.)*?)"', raw_text)
    if m:
        fields["notification_type"] = m.group(1)

    # is_interrupt: boolean
    m = re.search(r'"is_interrupt"\s*:\s*(true|false)', raw_text)
    if m:
        fields["is_interrupt"] = m.group(1) == "true"

    # background_tasks: 数组内存在 pending/running 项时标记后台任务在运行
    # （Stop 载荷含运行中的子 agent / 后台 shell 时，主 turn 结束 ≠ 任务完成）
    # 数组非贪婪截取，超长被截断时可能漏检，此时按无后台任务处理（兼容旧行为）
    m = re.search(r'"background_tasks"\s*:\s*\[(.*?)\]', raw_text, re.DOTALL)
    if m and re.search(r'"status"\s*:\s*"(?:pending|running)"', m.group(1)):
        fields["background_tasks"] = [{"status": "running"}]

    return fields


# ==================== Auto-Answer Strategy ====================


def select_option(options, logger):
    """从选项列表中选择最佳选项。

    策略优先级：
    1. label 含推荐标记（如 "(Recommended)"）→ 选该选项
    2. 无推荐标记 → 选第一项（Claude Code 通常将推荐选项放在首位）
    3. 无选项 → 返回 None
    """
    if not options:
        return None

    # 优先匹配推荐标记
    for opt in options:
        label = opt.get("label", "")
        label_lower = label.lower()
        for keyword in RECOMMENDED_KEYWORDS:
            if keyword in label_lower:
                logger.info("select_option: picked recommended option: {}".format(label))
                return label

    # 次选第一项
    first_label = options[0].get("label", "")
    logger.info("select_option: picked first option: {}".format(first_label))
    return first_label


def build_auto_answers(questions, logger):
    """为 AskUserQuestion 构造自动回复。

    策略：
    - 有选项：优先选带推荐标记的，次选第一项
    - 无选项：返回 fallback 文本（"Proceed with best practice"）
    - 多选：选推荐项或第一项（多选场景下选一个即可，Claude 会理解）
    """
    answers = {}
    for q in questions:
        header = q.get("header", "")
        options = q.get("options", [])

        selected = select_option(options, logger)
        if selected:
            answers[header] = selected
        else:
            # 无选项的自由回答，按最佳实践处理
            answers[header] = AUTO_ANSWER_FALLBACK
            logger.info("build_auto_answers: no options for '{}', using fallback".format(header))

    return answers


# ==================== Question Extraction ====================


def extract_ask_user_questions(tool_input):
    """从 AskUserQuestion 工具输入中提取问题列表。

    返回格式与 PluginQuestion DTO 对齐：
    [{ question, header, multi_select, options: [{ label, description }] }]
    """
    questions = tool_input.get("questions", [])
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
    return questions_data


def extract_notification_questions(data):
    """从 Notification hook 数据中提取问题信息。

    Notification 的 permission_prompt / elicitation_dialog 类型
    包含 message 和 title，构造为单问题格式以复用 PluginQuestion DTO。
    """
    message = data.get("message", "")
    title = data.get("title", "")
    notification_type = data.get("notification_type", "")

    if not message:
        return None

    # 构造为 PluginQuestion 兼容格式
    return [{
        "question": message,
        "header": title or notification_type,
        "multi_select": False,
        "options": [
            {"label": "Allow", "description": "Approve the request"},
            {"label": "Deny", "description": "Reject the request"},
        ],
    }]


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


def handle_user_prompt_submit(data, logger):
    """处理 UserPromptSubmit 事件。

    用户提交新 prompt 时触发，标记任务进入执行状态。
    这是"对话任务开始执行"的精确信号。

    任务记录的创建已由宿主的 on_input_submitted 会话扩展方法完成
    （用户按下回车时写入 task_history），此 hook 不再携带任务内容写表，
    仅推送状态变更，避免与宿主双重写表。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("user_prompt_submit: missing session_id")
        return

    prompt = data.get("prompt", "")
    # 截断过长的 prompt 用于 reason
    prompt_preview = prompt[:100] + "..." if len(prompt) > 100 else prompt

    logger.info(
        "HOOK user_prompt_submit: session_id={} prompt={}".format(session_id, prompt_preview)
    )

    push_task_status(session_id, "in_progress", "User submitted: {}".format(prompt_preview), logger)


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
        questions_data = extract_ask_user_questions(tool_input)

        # 推送 asking 状态到桌面端
        reason = "Auto-answered by BedCode" if auto_approve else "Waiting for user input"
        push_task_status(session_id, "asking", reason, logger, questions=questions_data)

        if not auto_approve:
            # 手动模式：不干预，走 Claude Code 原生交互
            logger.info("pre_tool_use: manual mode, pushed asking status, no auto-approve")
            return

        # 自动模式：按策略构造 answers
        answers = build_auto_answers(tool_input.get("questions", []), logger)

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


def handle_post_tool_use(data, logger):
    """处理 PostToolUse 事件。

    工具执行成功后触发，标记任务继续执行中。
    """
    session_id = data.get("session_id", "")
    tool_name = data.get("tool_name", "")

    if not session_id:
        logger.error("post_tool_use: missing session_id")
        return

    logger.info(
        "HOOK post_tool_use: session_id={} tool_name={}".format(session_id, tool_name)
    )

    # 工具执行成功，任务仍在进行
    push_task_status(session_id, "in_progress", "Tool {} completed".format(tool_name), logger)


def handle_post_tool_use_failure(data, logger):
    """处理 PostToolUseFailure 事件。

    工具执行失败时触发。
    - is_interrupt=true：用户主动中断，标记 interrupted
    - is_interrupt=false：工具执行出错，任务仍在进行（Claude 会尝试恢复）
    """
    session_id = data.get("session_id", "")
    tool_name = data.get("tool_name", "")
    is_interrupt = data.get("is_interrupt", False)
    error = data.get("error", "")

    if not session_id:
        logger.error("post_tool_use_failure: missing session_id")
        return

    logger.info(
        "HOOK post_tool_use_failure: session_id={} tool_name={} is_interrupt={} error={}".format(
            session_id, tool_name, is_interrupt, error[:100]
        )
    )

    if is_interrupt:
        push_task_status(session_id, "interrupted", "User interrupted: {}".format(tool_name), logger)
    else:
        # 工具失败但非中断，Claude 会继续尝试
        push_task_status(session_id, "in_progress", "Tool {} failed: {}".format(tool_name, error[:80]), logger)


def handle_notification(data, logger):
    """处理 Notification 事件。

    根据 notification_type 推送不同状态：
    - permission_prompt / idle_prompt / elicitation_dialog → asking
    - auth_success / elicitation_complete / elicitation_response → in_progress
    - 其他 → 仅记录日志
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("notification: missing session_id")
        return

    notification_type = data.get("notification_type", "")
    message = data.get("message", "")
    title = data.get("title", "")

    logger.info(
        "HOOK notification: session_id={} type={} title={} message={}".format(
            session_id, notification_type, title, message[:100]
        )
    )

    if notification_type in NOTIFICATION_ASKING_TYPES:
        # 需要用户交互的通知 → asking
        questions_data = extract_notification_questions(data)
        reason = "Waiting for user action: {}".format(notification_type)
        push_task_status(session_id, "asking", reason, logger, questions=questions_data)
    elif notification_type in NOTIFICATION_IDLE_TYPES:
        # Claude 响应完毕、空闲等待下一条输入：终态由 Stop hook 负责，
        # 此处不推送（推 asking 会误伤队列刚调度的下一任务，见常量注释）
        logger.info(
            "notification: type={} is response-complete idle, skip status push (Stop hook owns terminal state)".format(
                notification_type
            )
        )
    elif notification_type in ("auth_success", "elicitation_complete", "elicitation_response"):
        # 用户已完成交互 → in_progress
        push_task_status(session_id, "in_progress", "User responded: {}".format(notification_type), logger)
    else:
        logger.debug("notification: unhandled type={}, skip status push".format(notification_type))


def handle_stop(data, logger):
    """处理 Stop 事件。

    主 agent 完成响应时触发。
    需结合当前状态判断终态：
    - 已在终态（completed/interrupted）→ 不覆盖，防止 Stop 与 SessionEnd 冲突
    - stop_hook_active=true → in_progress（hook 续行中）
    - background_tasks 有 pending/running 项（Claude Code v2.1.145+）→ in_progress：
      会话只是暂停等待后台任务（子 agent / 后台 shell）完成，任务并未结束。
      主 agent 调用子 agent（Task 工具）后 turn 提前结束触发 Stop，此时推 completed
      会提前终态：任务行被宿主终态保护锁死，后续 in_progress 全部被丢弃，
      真正的完成再也无法同步（详见 863f6f79 会话实测）。
      不依赖脚本侧跟踪子 agent 状态 —— Stop 载荷自带的 background_tasks 即为权威信号。
    - 当前 asking → interrupted（用户在等待输入时停止，视为中断）
    - 其他 → completed（正常完成）

    中断判定不依赖子 agent：PostToolUseFailure(is_interrupt) / asking 态 Stop /
    SessionEnd 中断 reason 各自独立处理，无子 agent 的任务同样覆盖。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("stop: missing session_id, data keys={}".format(list(data.keys())))
        sys.exit(2)

    stop_hook_active = data.get("stop_hook_active", False)

    if stop_hook_active:
        status = "in_progress"
        reason = "Stop hook triggered continuation"
    else:
        # 后台任务（子 agent local_agent / 后台 shell local_bash 等）仍在运行：
        # 会话暂停等待后台工作，不是任务完成。旧版 Claude Code 无此字段时为空列表，
        # 走原有逻辑（兼容）
        background_tasks = data.get("background_tasks") or []
        running_bg = [
            t for t in background_tasks
            if isinstance(t, dict) and t.get("status") in ("pending", "running")
        ]
        if running_bg:
            status = "in_progress"
            reason = "Stopped, {} background task(s) still running".format(len(running_bg))
            logger.info(
                "HOOK stop: session_id={} background tasks still running: {}".format(
                    session_id,
                    [t.get("description") or t.get("agent_type") or t.get("type") or "?" for t in running_bg],
                )
            )
        else:
            current = query_task_status(session_id, logger)
            if current in TERMINAL_STATUSES:
                # 已在终态，Stop 是冗余信号（SessionEnd 或之前的 PostToolUseFailure 已标记）
                logger.info(
                    "HOOK stop: session_id={} skipped, already in terminal status '{}'".format(
                        session_id, current
                    )
                )
                return
            elif current == "asking":
                # 从 asking 状态停止 = 用户拒绝回答 / 中断
                status = "interrupted"
                reason = "Stopped while waiting for user input"
            else:
                status = "completed"
                reason = "Task completed"

    logger.info(
        "HOOK stop: session_id={} status={} stop_hook_active={}".format(
            session_id, status, stop_hook_active
        )
    )

    push_task_status(session_id, status, reason, logger)


def handle_subagent_stop(data, logger):
    """处理 SubagentStop 事件。

    子 agent 完成响应时触发，仅记录日志，不推送任何任务状态。
    子 agent 成功/失败均不代表主任务状态：
    - 子 agent 完成 ≠ 主任务完成（主 agent 还会继续工作）
    - 子 agent 失败 ≠ 主任务失败（主 agent 会处理结果继续执行）
    主任务终态（completed/interrupted）只由主 agent 的 Stop / SessionEnd 判定。
    若在此推送 in_progress，可能晚于主任务已推送的终态到达，把完成状态
    降级回执行中（宿主终态保护虽能拦截，但脚本侧不应依赖宿主兜底），
    且会推进 event_time 时序基线，干扰后续事件排序。
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("subagent_stop: missing session_id, data keys={}".format(list(data.keys())))
        sys.exit(2)

    agent_type = data.get("agent_type", "")
    logger.info(
        "HOOK subagent_stop: session_id={} agent_type={} skipped, no status push "
        "(subagent does not affect main task)".format(session_id, agent_type)
    )


def handle_session_end(data, logger):
    """处理 SessionEnd 事件。

    会话结束时触发，根据 reason 和当前状态判断终态：
    - resume → 不推送（会话恢复，非终止）
    - prompt_input_exit / clear / logout / bypass_permissions_disabled → 先查当前状态，
      终态或 idle 则跳过（/clear 是上下文清理副作用，不得改写前一任务终态），
      否则 interrupted
    - other + 当前已完成 → 不覆盖（Stop 已正确标记 completed）
    - other + 当前 idle → 不推送（无任务运行，无需标记）
    - other + 其他状态 → interrupted（保守处理）
    """
    session_id = data.get("session_id", "")
    if not session_id:
        logger.error("session_end: missing session_id")
        return

    reason = data.get("reason", "other")

    if reason == "resume":
        # 会话恢复，非终止，不推送状态
        logger.info("HOOK session_end: session_id={} reason=resume, skipped".format(session_id))
        return

    if reason in SESSION_END_INTERRUPT_REASONS:
        # 已知副作用场景：/clear 会触发 SessionEnd(reason=clear)，此时前一任务
        # 通常已 completed。无守卫地推 interrupted 会把正常完成状态翻成中断，
        # 与 "other" 分支同样先查当前状态：终态或 idle 一律跳过
        current = query_task_status(session_id, logger)
        if current in TERMINAL_STATUSES or current in ("idle", None):
            logger.info(
                "HOOK session_end: session_id={} reason={} skipped, current status '{}' "
                "is terminal or idle".format(session_id, reason, current)
            )
            return
        status = "interrupted"
        status_reason = "Session ended: {}".format(reason)
    elif reason == "other":
        # other 可能是正常退出，需根据当前状态判断
        current = query_task_status(session_id, logger)
        if current in TERMINAL_STATUSES:
            # 已在终态（Stop 已标记 completed 或之前已 interrupted），不覆盖
            logger.info(
                "HOOK session_end: session_id={} reason=other skipped, already in terminal status '{}'".format(
                    session_id, current
                )
            )
            return
        elif current == "idle" or current is None:
            # 无任务运行或查询失败，不推送终态
            logger.info(
                "HOOK session_end: session_id={} reason=other skipped, no active task (current={})".format(
                    session_id, current
                )
            )
            return
        else:
            # in_progress / asking → 会话异常退出
            status = "interrupted"
            status_reason = "Session ended unexpectedly (current: {})".format(current)
    else:
        # 未知 reason，保守标记 interrupted
        status = "interrupted"
        status_reason = "Session ended: {}".format(reason)

    logger.info(
        "HOOK session_end: session_id={} reason={} status={}".format(
            session_id, reason, status
        )
    )

    push_task_status(session_id, status, status_reason, logger)


# ==================== Main ====================

# 命令 → handler 映射
COMMAND_HANDLERS = {
    "session-start": handle_session_start,
    "user-prompt-submit": handle_user_prompt_submit,
    "pre-tool-use": handle_pre_tool_use,
    "post-tool-use": handle_post_tool_use,
    "post-tool-use-fail": handle_post_tool_use_failure,
    "notification": handle_notification,
    "stop": handle_stop,
    "subagent-stop": handle_subagent_stop,
    "session-end": handle_session_end,
}


def main():
    # Windows 中文系统默认编码为 GBK/CP936，而 Claude Code 通过 stdin 传入 UTF-8 编码的 JSON。
    # 必须在读取 stdin 之前强制 UTF-8，否则中文字符会被错误解码为乱码
    # （如 "你好" UTF-8 字节 E4 BD A0 E5 A5 BD 被按 GBK 解码为 "浣犲ソ"）
    if sys.platform == "win32":
        sys.stdin.reconfigure(encoding="utf-8")
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")

    if len(sys.argv) < 2:
        print("Usage: python3 auto_task_hook.py <command>", file=sys.stderr)
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

    COMMAND_HANDLERS[command](data, logger)


if __name__ == "__main__":
    main()
