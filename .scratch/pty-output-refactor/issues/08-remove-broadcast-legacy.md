# 08 — 删除广播兼容通道与死代码

**What to build:** 迁移完成后清理：删除 `output_broadcast`（pty_reader.rs/pty_process.rs）、`FrontendOutputHandler` + `frontend_output_handler.rs`、PtyOutputEvent broadcast 分发；`commands/session.rs` 删 `get_session_output_history`、`session.rs` 删 `OutputCache`/`DefaultOutputCache`（无调用方）；`session_manager.rs` 删 FrontendOutputHandler::spawn。确认无索引残留后 cargo test 全绿。

**Spec:** §5.5、§8（P3 部分）

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] output_broadcast + FrontendOutputHandler 删除（含 emit 路径）
- [ ] get_session_output_history / OutputCache 删除
- [ ] 全量 cargo test 通过；grep 确认无残留引用

## Comments