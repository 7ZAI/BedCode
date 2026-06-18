@echo off
REM BedCode Write Event Script (Windows)
REM Writes stop/subagent_stop events to JSONL file after prompt analysis

setlocal enabledelayedexpansion

REM Read stdin into variable
set "INPUT="
for /f "delims=" %%A in ('more') do set "INPUT=!INPUT!%%A"

REM Validate input
if "!INPUT!"=="" (
    echo {"error": "missing input"} >&2
    exit /b 2
)

REM Extract session_id using simple string parsing
set "SESSION_ID="
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr "session_id" 2^>nul') do (
    set "SESSION_ID=%%~A"
    set "SESSION_ID=!SESSION_ID:"=!"
    set "SESSION_ID=!SESSION_ID: =!"
    goto :got_session
)
:got_session

REM Validate session_id
if "!SESSION_ID!"=="" (
    echo {"error": "missing session_id"} >&2
    exit /b 2
)

REM Extract hook_event_name
set "HOOK_EVENT=Stop"
echo !INPUT! | findstr /C:"SubagentStop" >nul 2>&1 && set "HOOK_EVENT=SubagentStop"

REM Extract reason field
set "REASON="
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr "\"reason\"" 2^>nul') do (
    set "REASON=%%~A"
    set "REASON=!REASON:"=!"
    set "REASON=!REASON: =!"
    goto :got_reason
)
:got_reason

REM Initialize status
set "STATUS=unknown"
set "STATUS_REASON=!REASON!"

REM Try to parse prompt hook result from tool_result
REM The prompt hook output will be in tool_result field
set "TOOL_RESULT="
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr "\"tool_result\"" 2^>nul') do (
    set "TOOL_RESULT=!INPUT:*tool_result:=!"
    goto :got_tool_result
)
:got_tool_result

REM Simple status detection from tool_result content
if defined TOOL_RESULT (
    echo !TOOL_RESULT! | findstr /C:"\"status\":.*completed" >nul 2>&1 && set "STATUS=completed" && set "STATUS_REASON=Task completed"
    if "!STATUS!"=="unknown" echo !TOOL_RESULT! | findstr /C:"\"status\":.*asking" >nul 2>&1 && set "STATUS=asking" && set "STATUS_REASON=Waiting for user input"
    if "!STATUS!"=="unknown" echo !TOOL_RESULT! | findstr /C:"\"status\":.*in_progress" >nul 2>&1 && set "STATUS=in_progress" && set "STATUS_REASON=Task ongoing"
    if "!STATUS!"=="unknown" echo !TOOL_RESULT! | findstr /C:"\"status\":.*interrupted" >nul 2>&1 && set "STATUS=interrupted" && set "STATUS_REASON=Task interrupted"
)

REM Fallback: use reason field if we couldn't determine status
if "!STATUS!"=="unknown" if defined REASON (
    if "!REASON!"=="complete" set "STATUS=completed" && set "STATUS_REASON=Task completed"
    if "!REASON!"==":complete" set "STATUS=completed" && set "STATUS_REASON=Task completed"
    if "!REASON!"=="tool_use" set "STATUS=in_progress" && set "STATUS_REASON=Tool use in progress"
    if "!STATUS!"=="unknown" set "STATUS=in_progress" && set "STATUS_REASON=!REASON!"
)

REM Determine event type
set "EVENT_TYPE=stop"
if "!HOOK_EVENT!"=="SubagentStop" set "EVENT_TYPE=subagent_stop"

REM Generate timestamp in ISO 8601 format
for /f "usebackq tokens=1-3 delims=/ " %%A in ('date /t') do (
    set "YEAR=%%C"
    set "MONTH=%%A"
    set "DAY=%%B"
)
for /f "usebackq tokens=1-3 delims=:. " %%A in ('time /t') do (
    set "HOUR=%%A"
    set "MINUTE=%%B"
    set "SECOND=00"
)
REM Pad single digits
if "!MONTH:~1!"=="" set "MONTH=0!MONTH!"
if "!DAY:~1!"=="" set "DAY=0!DAY!"
if "!HOUR:~1!"=="" set "HOUR=0!HOUR!"
if "!MINUTE:~1!"=="" set "MINUTE=0!MINUTE!"
set "TIMESTAMP=!YEAR!-!MONTH!-!DAY!T!HOUR!:!MINUTE!:!SECOND!Z"

REM Ensure output directory exists
if not defined CLAUDE_PROJECT_DIR set "CLAUDE_PROJECT_DIR=%USERPROFILE%"
set "OUTPUT_DIR=%CLAUDE_PROJECT_DIR%\.claude"
set "OUTPUT_FILE=%OUTPUT_DIR%\bedcode-events.jsonl"
if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"

REM Write event to JSONL file
echo {"event":"!EVENT_TYPE!","session_id":"!SESSION_ID!","status":"!STATUS!","reason":"!STATUS_REASON!","timestamp":"!TIMESTAMP!"} >> "%OUTPUT_FILE%"

REM Output success for debugging
echo {"event":"!EVENT_TYPE!","status":"!STATUS!","written":true}

endlocal
