# 08 — 删除广播兼容通道与死代码

**What to build:** 迁移完成后清理：删除 `output_broadcast`（pty_reader.rs/pty_process.rs）、`FrontendOutputHandler` + `frontend_output_handler.rs`、PtyOutputEvent broadcast 分发；`commands/session.rs` 删 `get_session_output_history`、`session.rs` 删 `OutputCache`/`DefaultOutputCache`（无调用方）；`session_manager.rs` 删 FrontendOutputHandler::spawn。确认无索引残留后 cargo test 全绿。

**Spec:** §5.5、§8（P3 部分）

**Blocked by:** 07

**Status:** done

- [x] output_broadcast + FrontendOutputHandler 删除（含 emit 路径）
- [x] get_session_output_history / OutputCache 删除
- [x] 全量 cargo test 通过；grep 确认无残留引用

## Comments

- 2026-08-19 完成，提交 `feat(desktop): P1 ticket08 删除广播兼容通道与死代码`
- 删除面（Rust 8 文件 + 前端 5 文件）：
  - `pty/frontend_output_handler.rs`、`pty/pty_output_listener.rs`（更早死代码，lib.rs 仅剩注释）、`pty/pty_output.rs`（PtyOutputEvent 无消费方后整个删除，含 3 个单测）
  - `pty/pty_reader.rs`：PtyReader::start 删 output_broadcast 参数与 send 逻辑；测试同步裁剪（事件断言删除，保留 lifecycle 断言）
  - `pty/pty_process.rs`：删 output_broadcast 字段 / subscribe_output() / 测试订阅行
  - `session/event_bus.rs`：删 SessionEvent::Output 变体 + output_tx + output_sender()（status/restart 通道保留）
  - `session/session_manager.rs`：删 app_handle 字段 + set_app_handle + 3 处 FrontendOutputHandler::spawn + output_tx() + subscribe_output()（lib.rs 同步删 set_app_handle 块）
  - `session/session_output.rs`：删 OutputCache/DefaultOutputCache/OutputHistoryResponse/From<OutputEvent> + SessionOutputManager::get_history + GlobalOutputManager::get_history
  - `system/config.rs`：删 `channels.output_broadcast_capacity` 配置项全链（描述/分组/字段/default/parse/map/3 处测试断言）
  - 前端：model.ts PtyOutputEvent interface、useDesktopCommands re-export、fixtures/session.ts makePtyOutputEvent、drift.test.ts 条目、settings.ts output_broadcast_capacity
  - 注释清理：events/forwarder.rs ×3、plugin/host/commands.rs ×1、lib.rs ×1
- 移动端 config.rs 的 output_broadcast_capacity 不动（ticket 09 范围，独立文件）
- **验证**：cargo check 无 error；`cargo test -j 2` 549 lib + 8 集成全绿（556→549 恰为 3 个删除文件的 7 个测试）；`npx vitest run` 46 文件 415 测试全绿（416→415 为 drift 条目）
- 教训：整批 edit 中单个 oldText 不匹配会整批失败；GlobalOutputManager::get_history 首次替换 newText 误保留原内容导致残留，靠 grep 复检发现——删除类改动必须 grep 残留 + 跑测试双验证