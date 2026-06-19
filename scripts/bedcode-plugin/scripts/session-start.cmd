@echo off
REM BedCode Session Start Hook (Windows)
REM Records session creation event to JSONL file

setlocal enabledelayedexpansion

REM Read stdin into variable
set "INPUT="
for /f "delims=" %%A in ('more') do set "INPUT=!INPUT!%%A"

REM Extract session_id
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr "session_id"') do (
    set "SESSION_ID=%%~A"
    set "SESSION_ID=!SESSION_ID:"=!"
    goto :got_session
)
:got_session

REM Extract cwd
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr /C:"cwd"') do (
    set "CWD=%%~A"
    set "CWD=!CWD:"=!"
    goto :got_cwd
)
:got_cwd

REM Extract transcript_path
for /f "tokens=2 delims=:," %%A in ('echo !INPUT! ^| findstr "transcript_path"') do (
    set "TRANSCRIPT_PATH=%%~A"
    set "TRANSCRIPT_PATH=!TRANSCRIPT_PATH:"=!"
    goto :got_transcript
)
:got_transcript

REM Generate timestamp
for /f "tokens=1-3 delims=/ " %%A in ('date /t') do set "DATE=%%C-%%A-%%B"
for /f "tokens=1-3 delims=:." %%A in ('time /t') do set "TIME=%%A:%%B:00"
set "TIMESTAMP=!DATE!T!TIME!Z"

REM Ensure output directory exists
set "OUTPUT_FILE=%CLAUDE_PROJECT_DIR%\.claude\bedcode-events.jsonl"
if not exist "%CLAUDE_PROJECT_DIR%\.claude" mkdir "%CLAUDE_PROJECT_DIR%\.claude"

REM Write session_start event to JSONL file
echo {"event":"session_start","session_id":"!SESSION_ID!","project_path":"!CWD!","transcript_path":"!TRANSCRIPT_PATH!","timestamp":"!TIMESTAMP!"} >> "%OUTPUT_FILE%"

REM Output success
echo {"status":"recorded","session_id":"!SESSION_ID!","file":"!OUTPUT_FILE!"}

REM Push initial idle status to BedCode desktop HTTP API
if not defined BEDCODE_PORT set "BEDCODE_PORT=8080"
if defined BEDCODE_TOKEN (
    start /b curl -s -X POST "http://localhost:!BEDCODE_PORT!/api/plugin/task-status" -H "Content-Type: application/json" -d "{\"session_id\":\"!SESSION_ID!\",\"status\":\"idle\",\"reason\":\"Session started\",\"token\":\"!BEDCODE_TOKEN!\"}" > NUL 2>&1
)

endlocal
