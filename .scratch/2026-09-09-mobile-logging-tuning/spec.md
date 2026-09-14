# 移动端日志优化（mobile-logging-tuning）

> Status: resolved（2026-09-09）
> 范围：bedcode-mobile；配套桌面端 `.scratch/desktop-logging-overhaul/`

## 探索结论（移动端日志架构）

- **移动端日志 = Android logcat**：`android_logger`（log crate 实现）→ adb logcat；`tracing::` 宏经 tracing 的 `"log"` feature 桥接到 log crate。**无 tracing-subscriber 生态**（无文件层/reload/JSON/span 列表）。
- **dev 电脑端落盘**：`pnpm run tauri:android:dev:log` → `scripts/android-dev-log.js` tee 转发 logcat 到 `.dev-logs/android-dev.YYYY-MM-DD.log`（无 ANSI、按天、本地日期对齐）。
- **前端 console relay**：`commands/dev_logs.rs`（16KB 截断 + 批量，与桌面端一致）+ `src/utils/devConsoleRelay.ts`（dev 构建生效，release 剥离）。
- **关键约束**：logcat 主缓冲是系统级环形（每 app 默认 ~256KB-1MB），高频日志会被系统静默丢弃；移动端 Rust 无法直接写电脑磁盘。

## 已落地（P0×2）

1. **release 日志级别收敛**（`src-tauri/src/lib.rs`）：`android_logger` 级别 `cfg!(debug_assertions)` 区分——dev=Debug（开发期全量），**release=Info**（logcat 环形缓冲保护 + 减噪 + 避免泄露内部调试信息）。启动日志带 level 输出。
2. **.dev-logs 按天清理**（`scripts/android-dev-log.js`）：新增 `cleanupOldLogs()`，保留 14 天，启动时按 mtime 删除超期 `android-dev.*.log`（单文件失败不阻断；此前无限增长）。

## 审计确认的良好现状（未改动）

- **token 无泄露**：`state.rs` / `auth/manager.rs` 全部 `token.length()` 模式，不落明文。
- **热点克制**：心跳 Ping/Pong 低频间隔；重连事件低频（attempt N/M）；mDNS Found/Removed 事件驱动；无帧级/每 tick 日志（connection 82 处调用均为低频点）。
- **前端 relay 一致性**：16KB 截断 + 批量 + release 剥离。

## 明确不做（及理由）

- **tracing-android 引入**：log crate 桥是移动端正道（logcat 即载体），引入 subscriber 生态收益低、成本高。
- **设置页日志区**：移动端用户看不到 logcat，设置页提供级别控制无意义。
- **span 插桩**：移动端是控制端（发起方），链路价值有限；服务侧（桌面端）span 已覆盖（见 desktop-logging-overhaul 05）。

## 验证

- `cargo check --lib` ✓、`cargo test --lib` 251 全绿 ✓、`node --check scripts/android-dev-log.js` ✓
- 清理逻辑 mock 验证：15/20 天前文件被删、当天保留、非目标文件（other.txt）不动 ✓
- lens diagnostics：无 blocker（js lint 提示均为脚本本职/既有代码）
