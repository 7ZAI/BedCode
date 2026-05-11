@echo off
REM BedCode Session Start Hook (Windows)
REM Records the JSONL log path for remote monitoring

setlocal enabledelayedexpansion

REM CLAUDE_MESSAGE_LOG is set by Claude Code when it starts a session
if defined CLAUDE_MESSAGE_LOG (
    set "SESSION_FILE=%CLAUDE_PROJECT_DIR%\.claude\bedcode-session.json"

    REM Extract session ID from the path (last component of projects directory)
    for %%A in ("%CLAUDE_MESSAGE_LOG%") do set "SESSION_PATH=%%~dpA"
    for %%A in ("%SESSION_PATH:~0,-1%") do set "SESSION_ID=%%~nxA"

    REM Write session info
    echo {"jsonl_path": "%CLAUDE_MESSAGE_LOG%", "session_id": "%SESSION_ID%", "project_path": "%CLAUDE_PROJECT_DIR%"} > "%SESSION_FILE%"

    echo {"status": "recorded", "jsonl_path": "%CLAUDE_MESSAGE_LOG%"}
) else (
    echo {"status": "no_env", "message": "CLAUDE_MESSAGE_LOG not set"}
)

endlocal