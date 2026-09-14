# Upstream bug report: adb client SIGABRT / "server didn't ACK" when fd 0 is closed

> 提交入口：https://issuetracker.google.com/issues/new （组件选 Android → Platform Tools）
> 粘贴下方英文内容即可；本文件为草稿留存，非项目 issue。

---

## Title

adb client fails to connect to a running server and crashes a new fork-server (SIGABRT, "ADB server didn't ACK") when fd 0 (stdin) is closed

## Component

Android → Platform Tools (adb)

## Description

**Environment**

- platform-tools 37.0.1 (`Android Debug Bridge version 1.0.41 / Version 37.0.1-15733141`)
- Linux x86_64 (Ubuntu, kernel 7.0.0-30-generic)
- adb server already running and healthy, listening on `127.0.0.1:5037`

**Steps to reproduce (100% deterministic)**

1. Start the adb server normally with any command (e.g. `adb devices`).
2. Run any client command with **fd 0 closed** (bash):

```bash
adb shell pidof -s com.example.app 0<&-
```

**Observed**

```
* daemon not running; starting now at tcp:5037
ADB server didn't ACK
Full server startup log: /tmp/adb.1000.log
...
could not install *smartsocket* listener: Address already in use
* failed to start daemon
adb: cannot connect to daemon     (client exit code 1)
```

- The client incorrectly concludes the daemon is not running, forks a new
  `adb -L tcp:5037 fork-server server --reply-fd 3` child,
- the child fails `bind(127.0.0.1:5037)` with `EADDRINUSE` (retried 6 times) and then **aborts (SIGABRT, core dumped)**,
- the client exits 1 even though the original server is alive and answering other clients on the same socket at the same time.

3. The identical command with any open stdin (`< /dev/null`, a pipe, or a tty) succeeds instantly.

**strace evidence**

With fd 0 closed, the client's TCP socket to the server lands on **fd 0**:

```
connect(0, {sa_family=AF_INET, sin_port=htons(5037), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
```

The smart-socket handshake then fails, and the forked child shows:

```
bind(12, {sa_family=AF_INET, sin_port=htons(5037), ...}) = -1 EADDRINUSE
... (repeated x6) ...
--- SIGABRT {si_code=SI_TKILL} ---
```

**Expected behavior**

The client should pick a free descriptor for its control socket regardless of whether fd 0 is open, and talk to the already-running server instead of trying (and failing) to start a second one.

**Impact**

Any tool that spawns `adb` with stdin closed is affected. Concrete example: Rust's `duct` crate closes stdin by default for commands created with `stdout_capture()/stderr_capture()`, and the **Tauri CLI (`tauri android dev`)** uses exactly that pattern for its `adb shell pidof <package>` polling loop — the loop never succeeds, so the logcat forward never starts and the app's logs never reach the developer console. Because the loop retries every 2 seconds, this also produces a continuous fork-server SIGABRT crash storm (one core dump every few seconds, picked up by crash reporters such as apport).

**Workaround**

Wrap `adb` in a shim that re-opens fd 0 on `/dev/null` when it is absent:

```bash
#!/bin/bash
if [ ! -e /proc/self/fd/0 ]; then exec 0</dev/null; fi
exec /path/to/adb.real "$@"
```

---

## 2026-09-09 复现与落地修复

**复现**（Redmi K70E / Android 16，`tauri android dev`）：症状与上面 Impact 完全一致——
落盘日志（`.dev-logs/android-dev.*.log`）停在 `Starting: Intent`，控制台无任何 app 日志；
`/tmp/adb.1000.log` 每 ~2 秒一条 fork-server 崩溃（`could not install *smartsocket* listener: Address already in use` + SIGABRT）；
CLI 进程每 2 秒 spawn 一次 `adb shell pidof -s com.bedcode.mobile`（fd0 为 duct 给的管道，adb 阻塞在 stdin 读、永不 connect），
loop 永不 success → logcat 永不 attach。09-07 曾正常（当时 adb server 由 CLI 自举、无运行中 server 触发 EADDRINUSE 路径）；
09-08 09:19 server 重启后必现。与设备无关，是 host 侧 adb client 37.0.1 的 fd0 bug。

**落地修复**（已生效，见 `bedcode-mobile/scripts/`）：
1. `adb-fd0-shim.sh`：`if [ ! -t 0 ]; then exec 0</dev/null; fi; exec "$(dirname "$0")/adb.real" "$@"`
   （比上方草稿多覆盖「stdin 为管道阻塞」的情形，用 `-t` 而非 `/proc/self/fd/0` 存在性判断）。
2. `dev-run.js` 新增 `ensureAdbFd0Shim()` 预检：把 `$ANDROID_HOME/platform-tools/adb` 改名 `adb.real`、
   写入 shim（幂等，platform-tools 更新后下次 dev 会话自愈）。tauri CLI 用**绝对路径** spawn adb，
   PATH 级 shim 无效，必须替换 SDK 内本体。
3. 验证：安装 shim 后卡死的 CLI 进程下一个 pidof 周期即突破，logcat 挂上（`adb.real logcat -s ... --pid`），
   落盘文件恢复 full 日志（Rust tracing `BedCode` + 前端 relay `Tauri/Console`）；adb.1000.log 停止增长。

**维护要点**：platform-tools 升级会覆盖 `adb`（还原真二进制）→ 下次 `pnpm run tauri:android:dev*` 自动重装；
若 `adb.real` 意外丢失则需重新 `sdkmanager` 安装 platform-tools。Windows 不受影响（fd 语义差异）。
