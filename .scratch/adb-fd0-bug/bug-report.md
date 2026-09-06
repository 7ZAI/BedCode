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
