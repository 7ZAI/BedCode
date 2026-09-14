# 07 — PtySessionHandler 死代码决策 + trait 方法测试

**What to build:** `pty_handler.rs` 的 2 个测试只测 `running: AtomicBool` 的 get/set，而 `is_running()` / `set_running()` **生产代码从无任何调用方** —— 即测试覆盖的是死代码；同时 trait 真正被 `SessionManager` 使用的两个方法 `create_session` / `create_session_with_id` 零测试。模块注释自称「将 PTY 操作抽象为 trait，便于测试和替换实现」，实际未兑现。做一个明确决策：删除死代码，或接线使用，并给 trait 方法补测试。

**Blocked by:** 03（`PtySession` 行为测试成形后，trait 方法测试可直接复用其构造与断言模式）

**Status:** done（2026-09-15 修复：删除 running 死字段 + trait 方法补测）

- [ ] 决策：(a) 删除 `running` 字段与 `is_running`/`set_running` + 2 个对应测试（推荐，最诚实）；(b) 在会话停止路径接线（明确谁负责置 false、何时置 false）；(c) 明确保留为未来扩展点并删除测试避免误导
- [ ] 无论选哪个，给 `PtyHandler::create_session` / `create_session_with_id` 补测试（至少断言返回的 `PtySession::id()` 与传入 id 一致、config 字段透传）
- [ ] 若删 `running`：确认 `PtySessionHandler::new()` / `Default` 仍可编译，`session_manager.rs:85`/`93`/`974`（`Arc::new(PtySessionHandler::new())`）与 `:331`/`:430`（`create_session`）、`:330`/`:605`（`create_session_with_id`）均不受影响
- [ ] 更新模块级注释，移除「便于测试和替换实现」的承诺或补上一个真正走 trait 边界的测试
- [ ] `cargo test --lib pty::pty_handler` 通过

## 证据

`grep` 全仓（排除 `pty_handler.rs` 自身与 `pty_process.rs`）：

```
src/system/lifecycle.rs:277:        if supervisor.is_running().await {   ← 无关对象（supervisor）
src/pty.rs:11: pub use pty_handler::{PtyHandler, PtySessionHandler};
src/session/session_manager.rs:52:  pty_handler: Arc<PtySessionHandler>,
src/session/session_manager.rs:85/93/974: Arc::new(PtySessionHandler::new())
src/session/session_manager.rs:331:     self.pty_handler.create_session(launch_config.clone())?
src/session/session_manager.rs:430:     self.pty_handler.create_session(launch_config.clone())?
src/session/session_manager.rs:330:     self.pty_handler.create_session_with_id(sid.to_string(), launch_config.clone())?
src/session/session_manager.rs:605:     self.pty_handler.create_session_with_id(session_id.to_string(), launch_config.clone())?
  ↑ 后者还经插件宿主 host_impl/session.rs:86 → SessionManager::create_session_with_id 间接触达
```

**`is_running()` / `set_running()` 无生产调用方**（`lifecycle.rs:277` 是 `supervisor.is_running()`，不同对象，无关）。而 trait 两个真方法在生产各有调用点（上表 4 处），但**测试覆盖为 0**。

## 为什么需要决策而非直接改

`running` 字段语义模糊：它看起来像是「应用级运行标志」，但 `PtySession` 自己已有 `running: AtomicBool` 管会话级运行状态。两者职责重叠，接线前需要明确「谁来置 false、置 false 后已有会话怎么处理」——这属于设计取舍，按 AGENTS.md §0「不确定的设计取舍先问用户，不猜」。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §5.6
