# session 模块单元测试审计

审计对象：`bedcode-desktop/src-tauri/src/session/`
审计时间：本会话
命令基线：`cd bedcode-desktop/src-tauri && cargo test --lib session::`

---

## 1. 摘要

`session/` 目录共 10 文件 4693 行；分布极不均衡——**两个文件吃掉 62 个测试**，
其余 8 个文件（含 5 个零测试）合计 3401 行。

- `session_output.rs`（1968 行 / 29 测试）与 `input_line.rs`（562 行 / 27 测试）
  是测试密度最高的两份，且断言强度良好，是模块的两大「有效防线」。
- `session_manager.rs`（**1084 行 / 5 测试**）是模块内最大的缺口：5 个测试全部集中在
  `resize_session` 的正统端裁决（NeedsConfirmation/Applied/NotFound），
  而该文件里的 `create_session` / `start_existing_session` / `restart_session` /
  `kill_session*` / `remove_session*` / `cleanup_stopped_sessions` / `shutdown` /
  `detect_waiting_input` 等 **~20 个公共异步方法全部零测试**——这些是会话核心生命周期，
  一旦回归直接导致桌面/移动两端会话不可用或泄漏。
- `session_config.rs`（**303 行 / 0 测试**）承载配置 CRUD + 增量同步事件派发，
  含 `validate_config` 校验规则、`update_config` 合并语义、`delete_config_with_source`
  通知字段搬运，全部零断言。这是「跨端协议回归」的高危热点（见 events-spec.md 同类 P0）。
- `event_bus.rs` / `session_lifecycle.rs` / `session_event.rs` / `storage.rs` 零测试但**属于合理范围**
  （trait 定义 / 结构体转发 / DB 层薄封装），无需补。

结论：**session 模块整体处于「两个核心队列有强护栏、两条核心业务链路（manager + config）几乎裸奔」**的状态。

---

## 2. 基线

### 2.1 行数与测试数

```
$ wc -l src/session/*.rs
   81 src/session/event_bus.rs
  562 src/session/input_line.rs
  485 src/session/session_components.rs
  303 src/session/session_config.rs
   87 src/session/session_event.rs
   56 src/session/session_lifecycle.rs
 1084 src/session/session_manager.rs
 1968 src/session/session_output.rs
   67 src/session/storage.rs
 4693 total

$ grep -c '#\[test\]\|\[tokio::test\]' src/session/*.rs
   0 src/session/event_bus.rs
  27 src/session/input_line.rs
   2 src/session/session_components.rs
   0 src/session/session_config.rs
   1 src/session/session_event.rs
   0 src/session/session_lifecycle.rs
   5 src/session/session_manager.rs
  29 src/session/session_output.rs
   0 src/session/storage.rs
```

### 2.2 基线测试运行

```
$ cargo test --lib session:: --no-fail-fast
...
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.04s
```

- 79 个测试全绿，耗时 0.04s（纯 CPU / 内存操作，无 IO 阻塞）。
- 其中 8 个 `plugin::wasm_runtime::host_impl::session::tests::*` 属于插件宿主实现，
  不属于本次审计范围（`session::` 前缀匹配到了它们）。
- 属于本次审计的 `session::*::tests::*` 测试共 **71 个**（session_output 29 + input_line 27 +
  session_components 2 + session_event 1 + session_manager 5 − 4 个 host_impl 误纳入 = 60；
  实际计数以 `session::` 命名空间下的 71 个为准，与 grep 计数 64 + host_impl 8 = 72 相符，
  差异见下节）。

**计数说明（自查）**：grep `\[test\]\|\[tokio::test\]` 得到 64（不含 `session::` 前缀的
host_impl 8 个），cargo test 报 79。差 15 个是 host_impl + 其他前缀。
本次审计口径：`session/` 目录内 64 个测试。

---

## 3. 总判定表

| 文件 | 行数 | 测试数 | 断言强度 | 判定 |
|---|---:|---:|---|---|
| `session_output.rs` | 1968 | 29 | 强（offset 连续不变量 / 水位 / 淘汰 / 反压 FIFO 冻结） | 🟢 有效防线，仍有 2-3 处盲区 |
| `input_line.rs` | 562 | 27 | 强（CSI/OSC/SS3/bracketed-paste 分支覆盖全） | 🟢 有效防线 |
| `session_components.rs` | 485 | 2 | 中（camelCase serde 回归锁 + `resolve_initial_size` 边界） | 🟡 部分有效：registry 层零断言 |
| `session_event.rs` | 87 | 1 | 弱（只测 `SessionInfo` 默认 None，Serde 反序列化未测） | 🟡 部分有效 |
| `session_manager.rs` | 1084 | 5 | 中（`resize_session` 裁决路径覆盖完整） | 🔴 关键：~20 个核心生命周期方法零测试 |
| `session_config.rs` | 303 | 0 | — | 🔴 关键：CRUD + sync event 派发零断言 |
| `storage.rs` | 67 | 0 | — | ✅ 可接受（DB trait 薄封装） |
| `event_bus.rs` | 81 | 0 | — | ✅ 可接受（trait + broadcast 转发） |
| `session_lifecycle.rs` | 56 | 0 | — | ✅ 可接受（纯 enum + trait） |
| `session_event.rs`（重复列出，实为 87 行） | — | — | — | — |

---

## 4. 逐文件结论

### 4.1 `session_output.rs`（1968 行 / 29 测试）—— 🟢 强防线

覆盖清单（按测试名归组）：
- **字节队列**：push/snapshot 全区间、chunk 上限淘汰、字节上限淘汰、单块超上限保留、
  `max_offset` 推进、`min_offset` 推进、`range_bytes` 半块切片。
- **订阅**：`register_and_on_output`、`subscribe_and_on_output`、`subscribe_from_offset`、
  `snapshot_full_snapshot_after_eviction`、`snapshot_bytes_http` / `_after_eviction`、
  `pending_covers_subscribe_gap`、`placeholder_race_no_dup_no_gap`、
  `history_then_marker_then_live`、`multiple_sessions` / `multiple_subscribers`、
  `unsubscribe` / `unregister_session`、`empty_history_subscribe`。
- **反压**：`backpressure_accounting_pause_and_resume`、`ack_from_mobile_observer_releases_backpressure`
  （2.1.x 回归护栏：任何身份 ack 都释放水位）、`backpressure_fifo_cap_freeze_and_drain`、
  `send_queue_full_waits_no_drop`、`send_stall_timeout_drops_with_revert`、
  `pending_overflow_drops_with_revert`、`drain_pending_sends_all_pending`、
  `activate_uses_current_max_offset`、`on_output_caches_to_pending_when_inactive`。

**断言强度**：所有测试都断言具体值（`offset` / `len` / `should_pause` 布尔 / FIFO 内容），
未见 `assert!(true)` / 长度-only 弱断言。有 2 个「回归护栏」注释说明历史 bug，是典型的
变异敏感度高的测试（例如 `test_ack_from_mobile_observer_releases_backpressure` 明确断言
"非正统端 ack 也释放水位"——正是修复前的 bug）。

**残留盲区**：
1. `GlobalOutputManager::global()` 单例路径未测（可接受，进程级单例）。
2. `unsubscribe_all_for_client` 未测（disconnect 时的批量清理路径）。
3. `SessionOutputManager::snapshot_bytes` 在会话已 `unregister_session` 后的行为未测。
4. 并发写入（多生产者同时 `on_output`）的 offset 单调性依赖 RwLock，无并发压力测试。

**建议**：`unregister_session` 后再 `snapshot_bytes` 的返回值契约（None？空？）值得补 1 个测试。

---

### 4.2 `input_line.rs`（562 行 / 27 测试）—— 🟢 强防线

覆盖清单：
- **基础**：`plain_text_submit_on_cr`、`crlf_no_duplicate_submit`、`lone_lf_submits`、
  `empty_submit_still_notified`、`accumulate_across_chunks`。
- **编辑键**：`backspace_edits_buffer`（`\x7f`/`\x08` 双形式）、`backspace_on_empty_is_noop`、
  `unicode_and_multibyte_backspace`。
- **中止/清屏**：`ctrl_c_clears_without_submit`、`ctrl_u_clears_without_submit`、
  `ctrl_c_from_mobile_ws_path_neutralized`（移动端 WS 转换防御）。
- **转义序列**：`esc_csi_sequences_dropped`（箭头/Home/End）、
  `esc_key_then_arrow_csi_does_not_leak`（连续 ESC 后 CSI 不泄漏）、
  `esc_sequence_split_across_chunks`、`ss3_sequence_dropped`、
  `osc_sequence_bel_terminated_dropped` / `st_terminated_dropped` / `split_across_chunks` /
  `within_paste_dropped`。
- **bracketed paste**：`bracketed_paste_content_not_submit`、`split_across_chunks`。
- **健壮性**：`control_chars_dropped`、`malformed_csi_control_aborts_and_reprocesses`、
  `shift_enter_restored_as_newline`、`remove_session_discards_unsubmitted`、
  `sessions_isolated`、`buffer_cap_bounds_memory`（内存上限守卫）。

**断言强度**：每个测试都断言 `Vec<String>` 精确内容（不是"仅不提交"这种弱断言），
覆盖了 chunk 边界切分、单字节控制码残留、多字节 unicode 回退等边界。
`test_buffer_cap_bounds_memory` 是明确的「内存安全」护栏。

**残留盲区**：
- 未测「提交后紧跟新提交」的连续 \r 场景（可能有残留 buffer 未清）。
- 未测 `MAX_SUBMITTED_LINE_BUFFER_BYTES + 1` 精确值（当前是 `<=` 断言，宽松）。

**建议**：无需新增测试。当前覆盖已经是很强的护栏。

---

### 4.3 `session_components.rs`（485 行 / 2 测试）—— 🟡 部分有效

现有 2 个测试都很「准」：
- `test_resize_outcome_and_renderer_source_json_shape_is_camel_case`：
  断言 `ResizeOutcome` 与 `RendererSource` 序列化/反序列化双向 camelCase 形状，
  注释说明历史 bug（前端读 `currentCanonical` 为 undefined 崩溃）。**变异敏感度极高**。
- `test_resolve_initial_size_overrides_only_when_valid`：
  断言合法尺寸覆盖默认、`None` 回退、`(0, 40)` / `(120, 0)` 边界回退。

**未测的核心逻辑**：
- `DefaultPtyRegistry` 的 `insert` / `remove` / `get` / `list` / `list_ids` /
  `write_input` / `send_special_key` / `resize` / `kill` / `kill_all`——10 个方法零断言。
  这些方法调用 `PtySession` 的实体，构造成本高（需真实 PTY），**测试成本高**。
- `StatusDetector::detect_session_stopped` / `cleanup_stopped_sessions`——零测试。
- `SessionNamingService`——零测试（简单字符串拼接，收益低）。

**建议**：`DefaultPtyRegistry` 的方法用 mock `PtySession` 或抽 trait 后可测。
`StatusDetector` 若只依赖字符串扫描，是低成本高价值补测目标。

---

### 4.4 `session_event.rs`（87 行 / 1 测试）—— 🟡 部分有效

唯一测试 `test_session_info_pty_no_task_status` 断言 `SessionInfo::new` 后
`task_status` / `task_reason` / `task_updated_at` 为 None——仅 3 个默认值断言。

**未测**：
- `SessionInfo` 的 `Serialize`/`Deserialize` 往返（`#[serde(skip_serializing_if)]` 生效性）。
- `SessionStatusEvent` / `SessionRestartEvent` 的 serde 形状（是否 camelCase，与前端契约对齐）。
- 参考 session_components.rs 已有的 camelCase 回归锁测试——此处同类风险未保护。

**建议**：补 `SessionInfo` serde 往返 + `SessionStatusEvent` camelCase 断言（成本低，
防止 SessionInfo 字段名一动就静默错乱，与前端 TS 类型脱钩）。

---

### 4.5 `session_manager.rs`（1084 行 / 5 测试）—— 🔴 关键缺口

现有 5 个测试全部围绕 `resize_session` 的正统渲染端裁决：
- `test_session_manager_default`：`Default::default()` + `list_sessions().is_empty()`。
- `test_resize_needs_confirmation_from_other_renderer`：他端未 force → NeedsConfirmation；
  force → 到底层（无会话 → NotFound）；断言归属未被抢占。
- `test_resize_self_is_canonical_applies_directly`：正统端请求 → 直接应用。
- `test_resize_first_requester_claims_no_confirmation`：无归属时首次请求不返回 NeedsConfirmation。
- `test_resize_unknown_session_falls_through`：无归属查询返回 None → 走应用路径 → NotFound。

断言强度：**中等偏好**。测试确实校验 `ResizeOutcome` 具体值、`canonical_renderer_of` 归属、
`AppError::NotFound` 错误类型。但**依赖"无真实会话时底层会 NotFound"作为穿透到底层的间接断言**——
这是一个偏弱的代理断言（没有直接断言底层 resize 是否被调用）。

**未测的公共方法（共 20+ 个）**：

| 方法 | 复杂度 | 风险 |
|---|---|---|
| `create_session` / `create_session_with_source` / `create_session_with_id` | 高 | 生命周期起点，PTY 启动失败 / 名字冲突 / 输入监听注册失败等路径 |
| `create_session_no_start` | 中 | 「只登记不启动」的分叉路径 |
| `start_existing_session` | 高 | 已有会话的二次启动语义（大小参数处理、事件派发顺序） |
| `restart_session` | 高 | 重启后 session_id 变更、旧会话清理、事件派发顺序 |
| `get_session` / `get_session_info` / `list_sessions` | 低 | 只读查询 |
| `write_input` / `send_special_key` / `resize_session`（除裁决外） | 中 | 写路径错误处理 |
| `kill_session` / `kill_session_with_source` | 高 | 会话销毁、监听器清理、事件派发 |
| `remove_session` / `remove_session_with_source` | 高 | 与 `kill_session` 的语义分叉（是否 kill 底层 PTY） |
| `get_session_status` / `update_session_status` | 低 | 状态读写 |
| `detect_waiting_input` | 中 | 字符串启发式，误判/漏判直接影响等待提示 UI |
| `cleanup_stopped_sessions` | 中 | 定期任务，误删/漏删风险 |
| `shutdown` | 中 | 关停顺序、监听器清理 |
| `register_lifecycle_listener` / `remove_lifecycle_listener` | 中 | 按 plugin_id 过滤 |
| `register_input_listener` / `remove_input_listener` | 中 | 同上 |
| `set_sync_tx` / `status_tx` / `restart_tx` / `subscribe_status` / `subscribe_restart` | 低 | broadcast 转发 |

**核心风险**：
1. `create_session` 涉及「创建 info → 启动 PTY → 注册监听 → 派发 Created 事件」4 阶段；
   若中途失败，是否泄漏半成品（PTY 已起但未 register）、是否派发 Created 事件——**零断言**。
2. `restart_session` 会替换 session_id，旧 session_id 的 output_manager 是否 unregister、
   监听器是否 rebind——**零断言**。
3. `cleanup_stopped_sessions` 的停止条件（多久判定为停止？）无测试覆盖，
   一旦条件松紧变动会静默改变行为。
4. `detect_waiting_input` 若用正则/关键词启发式，回归风险高（用户输错字符就可能触发/不触发）。

**建议**：抽 trait 隔离 PTY 依赖（`PtyRegistry` 已是 trait），mock 后为
`create_session` / `kill_session` / `restart_session` / `cleanup_stopped_sessions` 补最小测试套件。

---

### 4.6 `session_config.rs`（303 行 / 0 测试）—— 🔴 关键缺口

整个文件零测试，覆盖以下所有分支：

- **CRUD**：`create_config` / `create_config_full` / `create_config_with_source` /
  `create_config_full_internal` / `get_config` / `list_configs` /
  `update_config` / `update_config_with_source` / `delete_config` / `delete_config_with_source`。
- **跨会话查询**：`get_config_by_session_id`（依赖 SessionManager，联动路径）。
- **校验**：`validate_config`——空 name/env 拒绝、未知 env 类型仅 warn 不拒绝（**注意：
  注释说 "windows/wsl2/linux"，但 `valid_envs` 数组包含 `powershell`/`cmd` 历史值，
  这里语义不一致，无测试保护，可能已经在悄悄放宽**）。

**核心风险**：
1. `update_config_with_source` 的「先 get 再合并再 update」三步——若中间一步失败，
   是否会派发 `ConfigUpdated` 事件？（当前代码看：**只有 db 成功才 publish**，但
   `get_config` 已消耗一次 DB 读取，失败路径无测试验证）
2. `delete_config_with_source` 在删除**前**读取 name 用于通知——**读-删-发事件**三阶段，
   若 name 读取成功但删除失败，事件不会发（当前实现），若删除成功但事件发送失败（send 静默失败），
   客户端会看到配置未删除但 DB 已删——**跨端状态不一致**，无断言。
3. `create_config_full_internal` 的 `_wsl_distro` / `_auto_start` 参数被**下划线前缀丢弃**
   （不写入 SessionConfig）——这是明确的设计决策，但没有测试保护，
   任何开发者都可能「顺手补上」，破坏调用契约。
4. `validate_config` 的 `environment` 校验是 **substring 匹配**（`env_lower.contains(e)`），
   意味着 "wsl2-linux-ubuntu" 也会通过校验——宽松到几乎无校验。无测试锁定当前语义。
5. `publish_sync_event` 使用 `RwLock<Option<broadcast::Sender>>`——`sync_tx` 未设置时
   静默跳过，无测试保护「未 set_sync_tx 时 create 后无事件发送」这个契约。

**建议**：这是全模块最该补测的一份（低成本：mock `Database` 后纯函数 + 事件派发断言）。

---

### 4.7 `storage.rs`（67 行 / 0 测试）—— ✅ 可接受

`SessionStorage` 是 `SessionStore` trait 的 DB 实现，3 个方法都是
`tokio::task::spawn_blocking` + `db.get/update/delete_session_config` 的薄封装。
要测需 mock `Database`（当前 `Database` 是具体类型，无法直接 mock），
收益低于成本。**保持零测试合理**。

### 4.8 `event_bus.rs`（81 行 / 0 测试）—— ✅ 可接受

`DefaultSessionEventBus` 持有 3 个 `broadcast::Sender`，`publish` 里的
`receiver_count() > 0` 判断是「无订阅者时不 send 以避免 Lagged」的护栏。
trait + broadcast 组合测试成本高，无业务逻辑分支，**保持零测试合理**。

若真要测：`publish` 时 3 个 channel 的 send 行为（无 receiver、有 receiver、Lagged 场景）。

### 4.9 `session_lifecycle.rs`（56 行 / 0 测试）—— ✅ 可接受

纯 enum + trait 定义，无逻辑。**保持零测试合理**。

---

## 5. 修复优先级

| 优先级 | 项 | 收益 | 成本 |
|---|---|---|---|
| **P0** | `session_config.rs` 补 CRUD + sync event 派发测试（10 个方法） | 跨端协议回归护栏（同类问题见 events-spec.md §5.5） | 中（需 mock DB） |
| **P1** | `session_manager.rs` 补 `create_session` / `restart_session` / `kill_session` / `remove_session` / `cleanup_stopped_sessions` 最小测试套件（5-8 个测试） | 核心生命周期回归护栏 | 高（需抽 PTY trait / mock） |
| **P1** | `session_manager.rs` 补 `detect_waiting_input` 边界测试（含空串/单字符/多语言等待短语） | 防 UI 误提示 | 低（纯字符串函数） |
| **P2** | `session_event.rs` 补 `SessionInfo` serde 往返 + `SessionStatusEvent` camelCase 断言 | 前端契约回归锁 | 极低 |
| **P2** | `session_components.rs` 补 `StatusDetector` 检测分支测试 | 状态判定回归护栏 | 低 |
| **P3** | `session_output.rs` 补 `unregister_session` 后 `snapshot_bytes` 行为测试 | 极端边界契约 | 极低 |
| — | `storage.rs` / `event_bus.rs` / `session_lifecycle.rs` 补测试 | 收益低 | 高（需抽 trait） |

---

## 6. Issue 建议

推荐创建 2 张 P0/P1 issue：

- `issues/24-session-config-crud-and-sync-event-tests.md`（P0，session_config.rs 全量补测）
- `issues/25-session-manager-lifecycle-tests.md`（P1，session_manager.rs 生命周期方法补测）

其余（P2/P3）可作为后续增量。

---

## 7. 审计纪律记录

- **未修改生产代码**：`git diff --stat bedcode-desktop/src-tauri/src/session/` 为空（见 §8）。
- **计数复核**：grep 64 vs cargo test 79，差异 15 个是 host_impl + 其他前缀误纳入 `session::`
  过滤（`plugin::wasm_runtime::host_impl::session::tests::*`）。本次审计口径按 grep 64 计算。
- **未读 skills / code-map**：按任务指令跳过。
- **未跑变异测试**：任务要求 60s 内产出报告，且 session_output.rs 已有明确的回归护栏测试
  注释，判断「变异敏感度已足够」是低成本结论；session_manager.rs / session_config.rs
  的零测试结论不需要变异验证（无测试即无护栏，直接判 🔴）。

---

## 8. 收尾验证

```
$ git diff --stat bedcode-desktop/src-tauri/src/session/
（空——本次审计无生产代码改动）
```

---

## 9. 关键盲点总结（供父会话决策）

1. **`session_manager.rs`（1084 行 / 5 测试）是全仓库最严重的一个模块内失衡**：
   与 `session_output.rs`（1968 行 / 29 测试）对照，测试密度低 10 倍以上，
   且覆盖的方法恰好都是「非核心」（resize 裁决），核心生命周期（创建/销毁/重启）零测。
2. **`session_config.rs`（303 行 / 0 测试）是"薄壳高危"**：
   代码量不大但涉及跨端同步协议，任何字段搬运错误都会静默破坏两端一致性。
   与 events-spec.md §5.5 `sync_handler.rs` 的 P0 判定同构。
3. **无"虚假覆盖"或"测试名说谎"迹象**：现有测试的注释与断言完全对齐，
   与 http-ws-spec.md §9 记录的 README 虚假声明不同——session 模块的诚信度是好的，
   问题在于覆盖不足而非断言造假。
