# adb client fd0 bug：无日志 / logcat 不挂载的根因与修复

> 整理自 2026-09-07 adb fd0 bug 专项 bug-report（2026-09-27 迁入 docs）。
> 面向「移动端无 dev 日志排查」（见 `docs/knowledge/logging.md` §4）。

## 现象

`pnpm run tauri:android:dev` 时**落盘日志停在 `Starting: Intent`，控制台无任何 app 日志**
（`.dev-logs/android-dev.*.log`），logcat 永不 attach：

- `/tmp/adb.1000.log` 每 ~2 秒一条 fork-server 崩溃：
  `could not install *smartsocket* listener: Address already in use` + SIGABRT（core dumped）
- CLI 进程每 2 秒 spawn 一次 `adb shell pidof -s com.bedcode.mobile`，loop 永不 success → logcat 永不 attach

## 根因（upstream bug，非本仓库）

**adb client 37.0.1 在 fd 0（stdin）关闭时出问题**：

1. client 错误判定 daemon 未运行，fork 新 `adb fork-server server --reply-fd 3` 子进程；
2. 子进程 `bind(127.0.0.1:5037)` 得 `EADDRINUSE`（重试 6 次）后 **SIGABRT 崩溃**（core dumped）；
3. client 因此退出 1——尽管原 server 一直活着且在同一 socket 上正常应答其他客户端。

触发形态（100% 复现）：`adb shell pidof -s com.example.app 0<&-`（fd 0 关闭）。
strace 证据：fd 0 关闭时 client 的控制 socket 落在 **fd 0** 上
（`connect(0, ... sin_port=htons(5037)) = 0`），smart-socket 握手随即失败。

**与本仓库的关联**：Rust `duct` crate 对 `stdout_capture()/stderr_capture()` 创建的命令**默认关闭
stdin**；`tauri android dev` 的 `adb shell pidof <package>` 轮询循环正是这个模式 → 循环永不 success、
logcat 永不启动；重试每 2 秒一次 → 持续 fork-server SIGABRT 崩溃风暴（被 apport 等崩溃报告器采集）。

**upstream bug 报告草稿**（提交 Google Issue Tracker Android → Platform Tools 用的英文正文）
保留在原专项目录，本文件只记仓库侧处置。

## 落地修复（已生效，`bedcode-mobile/scripts/`）

1. **`adb-fd0-shim.sh`**：`if [ ! -t 0 ]; then exec 0</dev/null; fi; exec "$(dirname "$0")/adb.real" "$@"`
   ——比 upstream 草稿多覆盖「stdin 为管道阻塞」的情形（用 `-t` 而非 `/proc/self/fd/0` 存在性判断）。
2. **`dev-run.js` 新增 `ensureAdbFd0Shim()` 预检**：把 `$ANDROID_HOME/platform-tools/adb` 改名 `adb.real`、
   写入 shim（幂等）。tauri CLI 用**绝对路径** spawn adb，PATH 级 shim 无效，必须替换 SDK 内本体。
3. 验证：安装 shim 后卡死的 CLI 进程下一个 pidof 周期即突破，logcat 挂上
   （`adb.real logcat -s ... --pid`），落盘文件恢复 full 日志；`adb.1000.log` 停止增长。

## 维护要点

- **platform-tools 升级会覆盖 `adb`（还原真二进制）** → 下次 `pnpm run tauri:android:dev*` 自动重装 shim。
- 若 `adb.real` 意外丢失 → 重新 `sdkmanager` 安装 platform-tools。
- **Windows 不受影响**（fd 语义差异）。
- 09-08 曾正常（当时 adb server 由 CLI 自举、无运行中 server 触发 EADDRINUSE 路径）；09-09 复现时
  server 重启后必现。与设备无关，是 host 侧 adb client 37.0.1 的 fd0 bug。
