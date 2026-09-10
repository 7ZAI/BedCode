# BedCode 日志与排障手册（排障知识）

> 从主 `AGENTS.md` 外链的日志实现细节与排障知识。编码时只需遵守主文档「安全、日志与可观测性红线」一节；本节是排查日志问题 / 了解落盘机制时才读。

---

## 1. 异步写盘与测试隔离（Rust）

- 文件层走 `tracing-appender` non_blocking（有界缓冲 20000 行，溢出丢行并计数告警）；控制台 stdout 可同步；panic hook 裸文件写
- 测试用 `with_default` + 临时目录，禁止污染真实日志目录（`%LOCALAPPDATA%\com.bedcode.app\logs\`）

---

## 2. 落盘路径

| 端 | 落盘 |
| --- | --- |
| 桌面端 | 始终写文件 `%LOCALAPPDATA%\com.bedcode.app\logs\`：`runtime.*.log` 全级别（dev 强制 debug）/ `error.*.log` 仅 ERROR / `frontend.*.log` 仅 dev，按天轮转 |
| 移动端 | `pnpm run tauri:android:dev:log` 落盘 `bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log`（可 grep）；release 走 logcat |

**前端 console 日志（仅 debug）**：`logger.*` → `report_frontend_log` → tracing（target=`frontend`），release 自动剥离。

---

## 3. 插件 WASM 日志

- target 固定 `bedcode_lib::plugin::plugin_log`，`[plugin:xxx]` 前缀
- per-plugin 级别：`BEDCODE_PLUGIN_LOG=id=level`（filter 不能按字段过滤，per-plugin 级别需在 emit_plugin_log 入口做宿主侧阈值映射）
- WASM trap 必须带 backtrace：`Config::wasm_backtrace_max_frames(Some(32))` **不得关闭**（wasmtime 47.0.3 `default` features 已含 backtrace，零编译成本）
- 插件 wasm 恒 `--release` 构建（保留 names section 函数名、无 DWARF 行号）
- 调试模式（dev）：`BEDCODE_PLUGIN_DEBUG=1` → debug profile wasm + `wasm_backtrace_details Environment` + 燃料联动放大，见 `.scratch/plugin-wasm-logging/spec.md`

---

## 4. 移动端无 dev 日志排查

- 现象：落盘停在 `Starting: Intent` 后无 logcat 行
- 首查 tauri CLI 是否卡 `adb shell pidof` 轮询（adb client 37.0.1 fd0 bug，见 `.scratch/adb-fd0-bug/bug-report.md`；`dev-run.js` 预检自愈）
- 链路问题优先 grep 两端 `runtime.*.log` 关键 tag：`file_service` / `peer_changed` / `MessageBus` / `reqwest::connect`
- 排查参考：`docs/knowledge/mobile-desktop-auth.md`（认证链路）、`docs/knowledge/sdk-publish.md`

---

## 5. 已确立的日志基线（改动时保持）

- token 只记长度不落明文（`token.length()` 模式）
- 心跳/WS 日志克制：Pong 低频、无帧级日志
- 前端 console relay 两端一致（16KB 截断 + 批量 + release 剥离）
- dev 落盘无 ANSI 可 grep