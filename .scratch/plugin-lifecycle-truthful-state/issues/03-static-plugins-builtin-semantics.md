# 03 — 静态注册插件 builtin 常驻语义修复

**What to build:** 消除「日志称 Static plugin loaded、实际永不激活」的自相矛盾：静态注册插件按内置常驻语义初始化即 Activated，启动通知与命令调用门禁对其真实生效。

设计依据见同目录 `../spec.md` §3.4（采纳方案 A）。

**Blocked by:** None — can start immediately.（建议排在 01 之后落地，减少宿主同名文件编辑冲突）

**Status:** done（2026-08-26，方案 A 已落地）

- [x] 初始化时静态插件直接置 Activated，日志如实打印 builtin 已激活
- [x] 启动通知真正回调静态插件的 `on_startup`（带既有回调超时），失败记 error 日志
- [x] Rust 命令分发门禁对静态插件放行（身份校验通过即可达 handler）
- [x] 单元测试：合成注册的静态插件验证通知回调被调、命令可路由、停用/重启路径不受影响
- [x] workspace `cargo test` 全绿

## 实现记录（2026-08-26）

- **host.rs `PluginHost::new`**：静态注册插件插入即 `Activated` + `activated_at`，日志改为 `Static plugin activated (builtin)`。命令门禁（`invoke_rust_command` → `is_activated`）与启动通知因此自然生效，无需改分发层
- **host.rs `notify_startup`/`notify_shutdown`**：`AppContext::global()` 改 `try_global()`，无头上下文降级跳过 TS-only emit（与统一异常通道同策略），使通知链路可单测；on_startup/on_shutdown 的超时与 `Ok(Err)` 失败均记 error 日志（Result 契约由 issue 01 会话同期接入）
- **host.rs tests**：合成 inventory 条目（`com.bedcode.test-static`，仅编译进 lib 测试二进制）+ 3 个新测试：未激活不回调/激活后回调被调、经 `register_rust_command_handlers` 注册后门禁拒绝→激活放行路由到 handler、停用回落 Deactivated→重启跳过 WASM phase 恢复路由。注：on_startup fn pointer 签名适配了并发会话（issue 01）的 anyhow Result 契约
- 验证：desktop lib 521 绿（含新 3 测试）、SDK crate 74 绿、集成测试 ws_auth_rules/ws_session_route/http_auth_biometric 绿；`pty_session_chain` 偶发、`broadcast_shutdown` 稳定失败为已归档火绒环境级干扰（见 `.scratch/peer-network/issues/03-mdns-discovery-and-capabilities.md`），失败面与本票无交集
