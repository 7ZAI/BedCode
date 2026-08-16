"""codex_task_hook.py 单元测试

覆盖 Codex hooks 事件 → BedCode 任务状态机的完整映射：
SessionStart → idle / UserPromptSubmit → in_progress / PermissionRequest → asking
或自动放行 / PostToolUse → in_progress / Stop → completed|interrupted
（含 stop_hook_active 与子 agent 计数保护）/ SessionEnd → interrupted 兜底。

运行：python -m unittest test_codex_task_hook
"""

import contextlib
import io
import json
import logging
import os
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import codex_task_hook as hook  # noqa: E402


class PushCapture:
    """记录 push_task_status 调用的替身，避免真实 HTTP 请求。"""

    def __init__(self):
        self.calls = []

    def __call__(self, session_id, status, reason, logger, questions=None, bedcode_session_id=None):
        self.calls.append(
            {
                "session_id": session_id,
                "status": status,
                "reason": reason,
                "questions": questions,
                "bedcode_session_id": bedcode_session_id,
            }
        )


class CodexTaskHookTest(unittest.TestCase):
    def setUp(self):
        self.logger = logging.getLogger("test-codex-hook")
        self.sid = "bedcode-pty-1"
        os.environ["BEDCODE_SESSION_ID"] = self.sid
        os.environ.pop("BEDCODE_PORT", None)
        self.push = PushCapture()
        self.tmp = tempfile.TemporaryDirectory()
        hook.STATE_DIR = self.tmp.name

        self.push_patcher = mock.patch.object(hook, "push_task_status", self.push)
        self.push_patcher.start()

    def tearDown(self):
        self.push_patcher.stop()
        self.tmp.cleanup()
        os.environ.pop("BEDCODE_SESSION_ID", None)

    def read_state(self):
        with open(hook.subagent_state_file(), encoding="utf-8") as f:
            return json.load(f)

    # ==================== 生命周期事件 ====================

    def test_session_start_pushes_idle_and_prints_context(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            hook.handle_session_start(
                {"session_id": "thr_123", "cwd": "C:/proj", "source": "startup"}, self.logger
            )
        self.assertEqual(self.push.calls[-1]["status"], "idle")
        self.assertEqual(self.push.calls[-1]["bedcode_session_id"], self.sid)
        printed = json.loads(out.getvalue())
        self.assertEqual(
            printed["hookSpecificOutput"]["hookEventName"], "SessionStart"
        )

    def test_user_prompt_submit_pushes_in_progress(self):
        hook.handle_user_prompt_submit(
            {"session_id": "thr_123", "prompt": "修复登录 bug"}, self.logger
        )
        self.assertEqual(self.push.calls[-1]["status"], "in_progress")
        self.assertIn("修复登录 bug", self.push.calls[-1]["reason"])

    def test_user_prompt_preview_truncated(self):
        hook.handle_user_prompt_submit(
            {"session_id": "thr_123", "prompt": "x" * 200}, self.logger
        )
        self.assertIn("x" * 100 + "...", self.push.calls[-1]["reason"])
        self.assertNotIn("x" * 101, self.push.calls[-1]["reason"])

    def test_post_tool_use_pushes_in_progress(self):
        hook.handle_post_tool_use(
            {"session_id": "thr_123", "tool_name": "Bash"}, self.logger
        )
        self.assertEqual(self.push.calls[-1]["status"], "in_progress")
        self.assertIn("Bash", self.push.calls[-1]["reason"])

    # ==================== 权限请求 ====================

    def test_permission_request_manual_pushes_asking_without_decision(self):
        with mock.patch.object(hook, "query_session_mode", return_value=False):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                hook.handle_permission_request(
                    {
                        "session_id": "thr_123",
                        "tool_name": "Bash",
                        "tool_input": {"command": "rm -rf x", "description": "Deleting x"},
                    },
                    self.logger,
                )
        self.assertEqual(self.push.calls[-1]["status"], "asking")
        self.assertEqual(self.push.calls[-1]["questions"][0]["header"], "codex permission")
        self.assertEqual(len(self.push.calls[-1]["questions"][0]["options"]), 2)
        self.assertEqual(out.getvalue(), "")

    def test_permission_request_auto_returns_allow_without_asking(self):
        with mock.patch.object(hook, "query_session_mode", return_value=True):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                hook.handle_permission_request(
                    {
                        "session_id": "thr_123",
                        "tool_name": "Bash",
                        "tool_input": {"command": "cargo test"},
                    },
                    self.logger,
                )
        self.assertEqual(self.push.calls, [])
        printed = json.loads(out.getvalue())
        specific = printed["hookSpecificOutput"]
        self.assertEqual(specific["hookEventName"], "PermissionRequest")
        self.assertEqual(specific["decision"]["behavior"], "allow")

    # ==================== 子 agent 计数 ====================

    def test_subagent_start_stop_tracks_counter_without_pushes(self):
        hook.handle_subagent_start(
            {"session_id": "thr_123", "agent_type": "general"}, self.logger
        )
        self.assertEqual(self.read_state()["active"], 1)
        hook.handle_subagent_stop(
            {"session_id": "thr_123", "agent_type": "general"}, self.logger
        )
        self.assertEqual(self.read_state()["active"], 0)
        self.assertEqual(self.push.calls, [])

    def test_subagent_stop_never_goes_negative(self):
        hook.handle_subagent_stop(
            {"session_id": "thr_123", "agent_type": "general"}, self.logger
        )
        self.assertEqual(self.read_state()["active"], 0)

    # ==================== Stop 终态判定 ====================

    def test_stop_with_stop_hook_active_keeps_in_progress(self):
        hook.handle_stop(
            {"session_id": "thr_123", "stop_hook_active": True}, self.logger
        )
        self.assertEqual(self.push.calls[-1]["status"], "in_progress")

    def test_stop_with_running_subagent_keeps_in_progress(self):
        hook.handle_subagent_start(
            {"session_id": "thr_123", "agent_type": "general"}, self.logger
        )
        with mock.patch.object(hook, "query_task_status", return_value="in_progress"):
            hook.handle_stop(
                {"session_id": "thr_123", "stop_hook_active": False}, self.logger
            )
        self.assertEqual(self.push.calls[-1]["status"], "in_progress")
        self.assertIn("subagent", self.push.calls[-1]["reason"].lower())

    def test_stop_normal_completes(self):
        with mock.patch.object(hook, "query_task_status", return_value="in_progress"):
            hook.handle_stop(
                {"session_id": "thr_123", "stop_hook_active": False}, self.logger
            )
        self.assertEqual(self.push.calls[-1]["status"], "completed")

    def test_stop_while_asking_interrupts(self):
        with mock.patch.object(hook, "query_task_status", return_value="asking"):
            hook.handle_stop(
                {"session_id": "thr_123", "stop_hook_active": False}, self.logger
            )
        self.assertEqual(self.push.calls[-1]["status"], "interrupted")

    def test_stop_skips_when_already_terminal(self):
        with mock.patch.object(hook, "query_task_status", return_value="completed"):
            hook.handle_stop(
                {"session_id": "thr_123", "stop_hook_active": False}, self.logger
            )
        self.assertEqual(self.push.calls, [])

    # ==================== SessionEnd 兜底 ====================

    def test_session_end_interrupts_active_task(self):
        with mock.patch.object(hook, "query_task_status", return_value="in_progress"):
            hook.handle_session_end(
                {"session_id": "thr_123", "reason": "other"}, self.logger
            )
        self.assertEqual(self.push.calls[-1]["status"], "interrupted")

    def test_session_end_skips_idle_or_terminal(self):
        for current in ("idle", "completed", "interrupted", None):
            with self.subTest(current=current):
                self.push.calls.clear()
                with mock.patch.object(hook, "query_task_status", return_value=current):
                    hook.handle_session_end(
                        {"session_id": "thr_123", "reason": "other"}, self.logger
                    )
                self.assertEqual(self.push.calls, [])

    # ==================== 生效条件门控 ====================

    def test_push_is_noop_without_bedcode_session_id(self):
        os.environ.pop("BEDCODE_SESSION_ID", None)
        with mock.patch.object(hook, "urlopen", side_effect=AssertionError("must not call network")):
            hook.push_task_status(
                "bedcode-pty-1", "completed", "Task completed", self.logger
            )

    def test_main_exits_silently_without_bedcode_session_id(self):
        os.environ.pop("BEDCODE_SESSION_ID", None)
        with mock.patch.object(sys, "argv", ["codex_task_hook.py", "stop"]):
            with mock.patch.object(hook, "push_task_status", side_effect=AssertionError("must not push")):
                with self.assertRaises(SystemExit) as ctx:
                    hook.main()
        self.assertEqual(ctx.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
