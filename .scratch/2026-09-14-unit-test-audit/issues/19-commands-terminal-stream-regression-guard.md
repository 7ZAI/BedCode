# 19-commands-terminal-stream-regression-guard

> Status: `done`（2026-09-15 修复）
> Blocked by: 无
> 关联 spec: [commands-spec.md](../commands-spec.md) §5.4、§8 P1

## What to build

为 `terminal_stream` 补 client_id 分配逻辑 + ack 错误路径 + 会话不存在回归锁单元测试。

## 根因

`terminal_stream.rs` 零测试。关键风险点：

1. **`CHANNEL_CLIENT_COUNTER` 分配逻辑**：每次订阅分配唯一 `channel-{session}-{n}` ID。若 `fetch_add(1)` 被改坏（如 `fetch_add(0)`），所有订阅 client_id 相同，`unsubscribe` 会误删其他订阅——这是注释中明确记录的历史 bug（「曾导致重订阅后输出流断、终端无回显」）。
2. **`subscribe_terminal_channel` 会话不存在路径**：返回 `AppError::NotFound`，但此路径无测试守着。
3. **`terminal_channel_ack` ack 错误路径**：会话不存在 / offset 回退时无测试覆盖。

## 修复方向

1. **client_id 分配测试**：
   - 连续两次 `subscribe_terminal_channel` → client_id 递增（`channel-sess-0` → `channel-sess-1`）
   - 验证 client_id 格式 `channel-{session_id}-{n}`
2. **会话不存在测试**：
   - `subscribe_terminal_channel("nonexistent", ...)` → `Err(NotFound)`
   - `terminal_channel_ack("nonexistent", 0)` → `Ok(())`（ack 对不存在会话静默成功，需确认设计意图）
3. **任务链回收测试**：
   - `unsubscribe_terminal_channel` 后 `GlobalOutputManager` 不再向该 client_id 推送

> 注意：`terminal_stream` 依赖 `GlobalOutputManager::global()` 和 `AppConfig::global()`，测试需构造临时 session 或用 mock。可参考 `tests/pty_session_chain.rs` 的集成测试模式。

## 影响面

- 补测试不影响生产代码。
- 若发现 ack 对不存在会话的行为与设计意图不符，需修正。

## 验收清单

- [ ] client_id 递增测试通过
- [ ] 会话不存在 subscribe → Err 测试通过
- [ ] unsubscribe 后不再推送测试通过
- [ ] `cargo test --lib commands::terminal_stream` 通过

## Comments

2026-09-14 审计创建。P1：历史 bug 回归锁缺失（通道订阅误删导致终端无回显）。
