# 04 — pty_reader 补数据投递断言 + Err / 队列关闭 / running=false 分支

**What to build:** `pty_reader.rs` 的 3 个单测里 2 个只断言生命周期状态、从不检查任何字节到达 `GlobalOutputManager::on_output`。变异测试（把 `on_output` 整段删掉）下 16 个单测仍全绿。补上数据投递断言与 4 个未覆盖分支。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] `reads_output_and_reports_stopped_on_eof` 增加数据断言：订阅 `GlobalOutputManager`，断言收到的字节拼回等于输入负载（顺序 + 完整）
- [ ] `Ok(0)` 非 EOF 分支：假 Reader 先返回 `Ok(0)` 再吐数据，断言不产生空 `OutputEvent`、不死循环（当前代码无 `if n > 0` 守卫）
- [ ] `Err(e)` 分支：假 Reader 返回 `Err` → 生命周期事件为 `PtySessionStatus::Error`（当前两个假 Reader 从不返回 Err，Error 路径 0% 覆盖）
- [ ] `running=false` 中途退出：读循环被运行标志打断 → 生命周期为 `Stopped`
- [ ] 队列关闭路径：消费者提前退出导致 `blocking_send` 返回 `Err` → 读循环退出、生命周期正常发送
- [ ] 默认 pause 粘合层：`start()`（`pause_check == None`）走 `GlobalOutputManager::global().should_pause(&sid)`，至少一个测试构造真实未注册会话验证返回 false 不暂停
- [ ] `cargo test --lib pty::pty_reader` 通过

## 证据

变异测试（审查时已回滚）：

| 变异体 | 单测结果 | 集成测试结果 |
|---|---|---|
| `pty_reader.rs:66` `global_manager.on_output(event).await;` → `drop(_event);` | **16 passed** | `pty_session_chain_flow` 场景 2 FAILED（20s 超时） |

即**单测对「PTY 输出彻底丢失」的覆盖率为 0**，只有集成测试是防线。

`reads_output_and_reports_stopped_on_eof` 的现状：灌 9000 字节负载，注释明写「强制分多次 read → 多轮输出」，但断言只有：

```rust
let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
assert_eq!(status, PtySessionStatus::Stopped);
```

消费者任务是 `tauri::async_runtime::spawn` 发射后不管，测试也不 `yield_now` 或等待它完成，因此即使数据全丢也不影响这个断言。

## 建议形态

`GlobalOutputManager::new()` 可单独构造（非仅 `global()` 单例），`session_output.rs:1030` 的 `mod tests` 已有 11 处 `GlobalOutputManager::new()` 用例可参考构造方式；`register_session(session_id) -> Arc<SessionOutputManager>`（`session_output.rs:888`）、`subscribe(...)`（`session_output.rs:965`）是拿广播接收者的入口。

**关键约束**：`PtyReader` 的消费者任务硬编码 `GlobalOutputManager::global()`（`:63-64`），且 `start()` 走默认 pause 闭包时用 `GlobalOutputManager::global().should_pause(&sid)`（`:50-53`；`start_with_pause` seam 可注入 pause 判定，但 `on_output` 投递目标无注入缝）——单测构造自己的 `GlobalOutputManager::new()` 实例**不会被 PtyReader 使用**。因此必须走 `global()`：`register_session` 后用全局唯一 session_id（如 `"itest-delivery-<nanos>"`）避免与其他测试串扰。

具体步骤：

1. `GlobalOutputManager::global().register_session(sid)` 拿 `Arc<SessionOutputManager>`（需在 `PtyReader::start` 之前，否则首帧事件会以 "session not found" 被丢弃）
2. 订阅输出帧广播（`subscribe`），**必须在 PtyReader 启动前完成**，否则丢前几条
3. `pty_reader.wait()` 只 join 读线程，消费者任务是异步的 → 需要短暂 `tokio::time::sleep` + `yield_now` 排空，或把消费者任务句柄暴露出来 `join` 掉
4. 断言拼接后的字节 == 输入负载（`OutputEvent.data` 是 `Vec<u8>`；`start_offset` 恒为 0，由 `on_output` 的串行临界区按 `max_offset` 分配，此处只验内容顺序）

## 相关代码规范附带项（见 spec §7，可同票处理）

- `pty_reader.rs:116` `let _ = lifecycle_tx.send(exit_status);` —— EOF/Error 最终判定发送失败静默忽略（违反 AGENTS.md §6「重要路径禁止 `let _ =` 静默忽略错误」）。建议改为显式 `warn!` 带 `session_id`
- `pty_reader.rs:63` `tauri::async_runtime::spawn` 未用 `spawn_with_error_boundary()` 包装（违反 §6；全 `src/` 45 处裸 spawn，仅 14 个文件用了包装）
- `pty_reader.rs:125` `let _ = handle.join();` —— join 错误忽略

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §5.2
