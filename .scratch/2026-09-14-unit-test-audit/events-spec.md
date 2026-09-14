# events 模块单元测试审计

审计对象：`bedcode-desktop/src-tauri/src/events/`
审计时间：2024 (本会话)
命令基线：`cd bedcode-desktop/src-tauri && cargo test --lib events::`

## 1. 摘要

`events/` 目录共 5 个文件 1772 行；只有 `matcher.rs` 有测试（22 个 `#[tokio::test]`），
其余 4 个文件共 618 行 **零测试**。整体事件系统里最"薄"、最容易被改动破坏的是
`SyncEventHandler`（405 行，0 测试）和 `sync_event.rs` 的 `From<SyncEvent>` 转换
（穷尽 match，字段名一改就编译错但值传递错误会静默通过）。

## 2. 基线（磁盘读取，非推测）

### 2.1 行数与测试数

```
$ wc -l src/events/*.rs
   10 src/events/app_event.rs
   55 src/events/forwarder.rs
 1154 src/events/matcher.rs
  148 src/events/sync_event.rs
  405 src/events/sync_handler.rs
 1772 total

$ grep -c "#\[tokio::test\]" src/events/*.rs
   0  app_event.rs
   0  forwarder.rs
  22  matcher.rs
   0  sync_event.rs
   0  sync_handler.rs
```

### 2.2 基线测试运行

```
$ cargo test --lib events::
running 26 tests
... 22 个 events::matcher::tests::* 全部 ok
... 4 个 plugin::wasm_runtime::host_impl::events::tests::* 全部 ok
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 588 filtered out
```

（`plugin::wasm_runtime::host_impl::events::tests` 属于插件宿主实现，不属于本次审计范围。）

## 3. 变异测试（Mutation Test）

**变异点**：把 `matcher.rs::EventMatcher::publish` 改成 no-op（丢弃 event，只返回 `Ok(())`）：

```rust
// 原始：通过 event_sources 找到 source 并 sender.send(event)
// 变异后：
pub async fn publish<E: ...>(&self, event: E) -> Result<...> {
    let _ = event;
    Ok(())
}
```

**变异后测试运行结果**：`24 passed; 2 failed`

变红测试（`matcher.rs`）：
- `test_register_source_and_publish` — 直接调 `matcher.publish(...)` 并断言收到事件
- `test_struct_event_type` — 通过 `matcher.publish(NotificationEvent{...})` 验证结构体事件类型

其余 20 个 matcher 测试仍然通过（因为大多数测试用 `tx.send(...)` 走 broadcast channel
而不是 `matcher.publish`），这印证了测试覆盖面**只覆盖了 matcher 内部转发链路的一部分**。

变异已用 `cp` 备份回滚（`git diff --stat` 为空，见 §8）。

## 4. 判定表

| 文件 | 行数 | 测试数 | 断言强度 | 判定 |
|---|---:|---:|---|---|
| `app_event.rs` | 10 | 0 | n/a（空 trait） | ✅ 可接受 |
| `forwarder.rs` | 55 | 0 | — | ⚠️ 弱覆盖，可接受（Tauri spawn） |
| `matcher.rs` | 1154 | 22 | 强（含内容相等、状态计数、多类型并发、filter 谓词） | ✅ 优秀，仍有 Lagged/Closed 分支空缺 |
| `sync_event.rs` | 148 | 0 | — | 🔴 缺失，含 `From<SyncEvent>` 类型化转换 |
| `sync_handler.rs` | 405 | 0 | — | 🔴 严重缺失，11 个事件分发分支全部无断言 |

**断言强度说明（matcher.rs）**：
- 用 `assert_eq!(received, Event::X{..})` 校验内容而非仅长度（`test_register_source_and_publish`, `test_multiple_event_types_independent`）
- 用 `assert_eq!(handler_count, N)` 校验状态计数（`test_handler_count_tracking`, `test_clear_removes_everything`）
- 用 `assert_eq!(*ids, vec![10, 99])` 校验 filter 谓词命中集合（`test_filter_with_complex_predicate`）
- 用 `assert_eq!(all_count, 4); assert_eq!(connected_count, 2)` 校验全局 vs 过滤分流（`test_mixed_handler_and_filtered_handler`）
- 未见恒真断言（`assert!(true)` 等），未见快照替代行为断言

**已知 matcher.rs 覆盖缺口**：
1. `ensure_subscription` 里的 `RecvError::Lagged` 分支（`matcher.rs:300`）未测——需要构造超容量 channel
2. `RecvError::Closed` 分支（`matcher.rs:296`）未测——需要 drop 所有 receiver 后观察任务退出
3. `register_filtered` 直接调用路径（`matcher.rs:226`）未测，只测了 `on_filter` 封装（`matcher.rs:214`）
4. `global_matcher()` 单例（`matcher.rs:445`）未测（可接受，进程级全局状态不易回滚）
5. 变异测试证明 `publish` 破坏时**只有 2 个测试变红**——说明绝大多数 matcher 测试都走 `tx.send` 通道，`matcher.publish` 路径的独立度较低

## 5. 逐文件结论

### 5.1 `app_event.rs`（10 行）
```rust
pub trait AppEvent: Clone + Send + Sync + Debug {}
```
纯标记 trait，无任何可执行逻辑。**零测试合理**，无需补。

### 5.2 `forwarder.rs`（55 行）
`EventForwarder::start()` 拉起两个 `tauri::async_runtime::spawn` 任务，把
`SessionManager::subscribe_status()` / `subscribe_restart()` 的接收结果
`app_handle.emit(event::SESSION_STATUS_CHANGED/SESSION_RESTARTED, &event)`。

- **可测性问题**：`AppHandle` 是 Tauri 全局类型，构造成本高；`Emitter` trait 未抽象为接口，
  无法直接 mock。要写单测需要引入 trait 抽象。
- **风险**：低——只是订阅+broadcast 转发，出错时 tracing::error 已经打了日志。
- **建议**：保持零测试。若要提升，可将 `Emitter` 抽成 trait，注入 mock。

### 5.3 `matcher.rs`（1154 行，22 测试）
最强的一份。分 5 段：
1. 事件发送与订阅（4 测试）
2. 事件处理解耦（5 测试）
3. 多种具体事件类型（4 测试）
4. 过滤器（3 测试）
5. 边界情况与健壮性（6 测试，含 100 并发事件、handler/source 计数）

**判定**：优秀。可补：Lagged / Closed 分支、直接 `register_filtered`、
以及一个"publish 破坏时能被抓到"的独立路径测试（当前只有 2 个测试覆盖 publish 路径）。

### 5.4 `sync_event.rs`（148 行，0 测试）
主要是 `DesktopSyncEvent` 枚举 + `impl AppEvent` + `From<bedcode_plugin_api::events::SyncEvent>`。

- `From<SyncEvent>` 是 **穷尽 match**：SDK `SyncEvent` 新增变体时此处编译失败，是好的静态保障。
- 但 **字段搬运正确性**（`session_id` / `task_status` / `queue_count` / `job_id` 等）
  完全无断言。典型风险：某处 `session_id` 被误写成 `queue_count` 时编译仍能过、但值错乱。
- 建议补：4 个 `From` 转换的字段相等断言（`test_from_sdk_task_status_changed_preserves_fields` 等），
  成本极低（纯纯纯函数），收益高。

### 5.5 `sync_handler.rs`（405 行，0 测试）—— 最严重的缺口

`SyncEventHandler::process_event` 是 11 分支的大 match，每支转成 `SyncPayload::X`
再通过 `ws_manager.broadcast` 或 `broadcast_sync_to_others` 广播：

| 事件 | 是否查库 | 广播策略（`exclude_device`） |
|---|---|---|
| SessionCreated | 是（`session_manager.get_session`） | `Some(source_device)`（默认空串→broadcast 全体） |
| SessionStatusChanged | 是（取 name） | `None`（broadcast 全体） |
| SessionStopped | 是（取 name） | `source_device.as_deref()` |
| SessionRemoved | 否（**硬编码 `String::new()`**） | `source_device.as_deref()` |
| ConfigCreated | 是（`config_manager.get_config`） | `Some(source_device)` |
| ConfigUpdated | 是 | `Some(source_device)` |
| ConfigRemoved | 否 | `source_device.as_deref()` |
| TaskStatusChanged | 否 | `None`（全体） |
| SessionModeChanged | 否 | `None`（全体） |
| TaskQueueChanged | 否 | `None`（全体） |
| TaskScheduledChanged | 否 | `None`（全体） |

**风险点**：
1. **11 个分支零断言**。任何一处 broadcast 目标弄反（如 SessionCreated 忘写 `exclude_device`）
   都会导致客户端收到不该收的广播，或漏收；且编译无警。
2. `handle_session_created` / `handle_config_created` / `handle_config_updated`
   有"session/config not found" warn 后 return 的分支，无测试覆盖。
3. `SessionRemoved` 硬编码 `session_name = String::new()`——按注释说明，
   调用方应在移除前获取名称，但代码里没有任何保障。
4. `EventHandler::handle` 用 `tokio::spawn` 起了异步任务，无法直接 await 完成，
   测试要 sleep 或引入 Notify。

**建议**：把 `SyncEventHandler` 的 WS 广播抽象成可 mock 的 trait（如
`trait SyncBroadcaster { async fn broadcast_all(&self, payload); async fn broadcast_to_others(&self, exclude, payload); }`），
按上表 11 行分支各写 1 个测试（含 exclude_device 三种路径：Some(非空)、Some(空串)、None）。

## 6. 修复优先级

| 优先级 | 项 | 收益 | 成本 |
|---|---|---|---|
| **P0** | 给 `sync_handler.rs` 的 11 个 `handle_*` 分支补测试 | 消除跨端同步协议回归风险 | 需先抽象 WS 广播接口 |
| **P1** | `sync_event.rs` 补 `From<SyncEvent>` 4 个转换的字段等值断言 | 低成本高回报（纯纯纯函数） | 极低 |
| **P2** | `matcher.rs` 补 `Lagged` / `Closed` 分支 | 补齐错误路径 | 中（构造超容量 channel 略繁琐） |
| **P3** | `matcher.rs` 补 `publish` 独立路径断言（当前 22 个测试里只有 2 个覆盖 publish） | 变异敏感度提升 | 低 |
| — | `forwarder.rs` 补测试 | 收益小 | 高（要抽 trait） |
| — | `app_event.rs` 补测试 | 无意义 | — |

## 7. Issue 建议

如果时间允许，把 P0 和 P1 各拆一个 issue：

- `issues/22-events-sync-handler-tests.md`：P0
- `issues/22-events-sync-event-from-tests.md`：P1

（本次审计未创建，见 §9 说明。）

## 8. 收尾验证

```
$ git diff --stat bedcode-desktop/src-tauri/src/events/
（空——变异测试已通过 cp 备份回滚，生产代码零改动）
```

## 9. 说明

- 未创建 issue：本次仅审计；是否开 issue 由父会话决策（AGENTS.md §11 要求开发走 `.scratch/<task>/`，本次报告已按此约定放在 `.scratch/unit-test-audit/`）。
- 未跑 `cargo test` 全量：`events::` 前缀过滤已足够（26 tests，588 filtered out）。
- 未读 skills / code-map（按任务指令跳过）。
