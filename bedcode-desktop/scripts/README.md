# BedCode Claude Code Hooks

Monitor Claude Code sessions through the BedCode desktop app.

## Requirements

- Python 3.6+（系统自带，零额外依赖）
- BedCode desktop app running（可选，用于状态推送）

## How It Works

Hooks 通过全局 `~/.claude/settings.json` 配置，BedCode 桌面端启动时自动注入。
Hook 脚本自动复制到 `~/.claude/auto_task_hook.py`，全局生效。

1. **SessionStart**: 当 Claude Code 启动新会话时，hook 记录会话信息并推送 `idle` 状态到桌面端

2. **PreToolUse**: 查询会话自动授权模式：
   - 自动模式 + AskUserQuestion：自动选择推荐选项
   - 自动模式 + 其他工具：直接允许
   - 手动模式：不干预，但仍推送 asking 状态同步进度

3. **Stop / SubagentStop**: prompt hook 分析任务状态，command hook 推送到桌面端：
   - `completed` - 任务完成
   - `in_progress` - 任务进行中
   - `asking` - 等待用户输入
   - `interrupted` - 任务中断

4. **Logging**: 所有 hook 事件和 HTTP 请求记录到 `.claude/bedcode-plugin.log`，按天轮转保留 7 天

5. **HTTP Push**: 任务状态变更推送到 `POST /api/plugin/com.bedcode.auto-task/task-status`。端点由网关中间件对 `/api/plugin/*` 路径本地放行（无凭证），hook 仅在 BedCode 注入 `BEDCODE_SESSION_ID` 的 PTY 中生效，信任边界为本地网络

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `BEDCODE_SESSION_ID` | Auto | - | BedCode PTY 会话 ID（由 pty_process.rs 注入到进程环境变量）。hook 仅在此变量存在时生效，并随每次状态推送携带，宿主据此关联任务行 |
| `BEDCODE_PORT` | Auto | `8765` | HTTP API 端口（由插件 hooks.rs 注入到 hook 命令环境变量） |

> 认证说明：历史版本曾使用 `BEDCODE_TOKEN` 做 HTTP API 认证，现已被移除——网关中间件对 `/api/plugin/*` 路径放行、handler 不校验凭证，仅检查插件激活状态。服务监听 `0.0.0.0`，插件端点对局域网内设备可达，安全性依赖本地网络信任。

## Files

```
scripts/
├── auto_task_hook.py      # Hook 脚本（Python，零外部依赖）
└── README.md            # 本文件
```

## Configuration

Hooks 配置位于全局 `~/.claude/settings.json`，由 BedCode 桌面端启动时自动生成和注入。

手动配置示例（通常不需要）：

```json
{
  "hooks": {
    "SessionStart": [{
      "matcher": "",
      "hooks": [{
        "type": "command",
        "command": "BEDCODE_PORT=8765 python \"~/.claude/auto_task_hook.py\" session-start",
        "timeout": 5
      }]
    }]
  }
}
```

## Event Log

事件记录在 `~/.claude/bedcode-plugin.log`：

```
[2026-06-20T10:00:00Z] [INFO] HOOK session_start: session_id=abc123 project=/path source=startup permission=default
[2026-06-20T10:00:00Z] [INFO] HTTP POST http://localhost:8765/api/plugin/task-status session_id=abc123 status=idle
[2026-06-20T10:00:00Z] [INFO] HTTP response: 200 {"code":0,"data":null}
[2026-06-20T10:30:00Z] [INFO] HOOK stop: session_id=abc123 status=completed reason=Task done
```
