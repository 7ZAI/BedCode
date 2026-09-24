# 02: AppEvent 方法 + HostSyncEvent + SyncEvent 线格式对齐（双轨 expand）

**What to build:** 宿主获得统一事件发送协议：任何可同步广播的事件实现 `AppEvent`（`source_device` / `validate` / `to_sync_payload`），经 `publish` 进入 matcher。SDK `SyncEvent` 与出站 `SyncPayload` **同一 wire**（消灭内部标签 PascalCase → data 嵌套的改写）。新旧两路并存：同一逻辑事件分别走 `HostSyncEvent` 与旧 `DesktopSyncEvent`，移动端收到的 JSON 逐字节全等。

**Blocked by:** 01（SDK 类型与 re-export 就绪）.

**Status:** done（2026-09-24 落地：`AppEvent` 三方法 + `publish`/`PublishError` +
`HostSyncEvent` 薄适配；SDK `SyncEvent` 换 adjacently tagged snake_case；双轨 8 变体
wire 逐字节全等锁已跑绿。实施细节、三处偏离 spec 的裁定与覆盖损失见票末）

- [x] `AppEvent` 增加 `source_device()`（默认 None）、`validate()`（默认 Ok）、`to_sync_payload()`；`publish<E: AppEvent>(e)` 统一入口（内部走既有 matcher 源分发）
- [x] SDK `SyncEvent` serde 对齐 `SyncPayload`：`tag = "type", content = "data", rename_all = "snake_case"`；字段类型对齐（状态等原样 wire 透传，不引入宿主 `SessionStatus` 解析）
- [x] 插件 `broadcast_sync(SyncEvent)` 产出的新格式与旧格式对照表落地；**宿主反序列化切到新 `SyncEvent`**（旧内部标签产物拒绝并 fail-visible，内置插件随包重建）
- [x] `HostSyncEvent(pub SyncEvent)` 实现 `AppEvent`：`to_sync_payload` 为字段级近恒等转换；`source_device` 从载荷提取
- [x] 集成/单测证明双轨：新路径与旧路径生成的 `Message::SyncData` wire 全等；`From`/`DesktopSyncEvent` 仍可编译运行
- [x] 门禁：针对性 `cargo test` 过滤绿；不改移动端

---

## 实施记录（2026-09-24）

### 一、新增/改动面

| 位置 | 内容 |
| --- | --- |
| SDK `events.rs` | `SyncEvent` 换 `tag="type", content="data", rename_all="snake_case"`；`SessionCreated.session: wire::SessionSummary`（原 `Value`）、`Session*StatusChanged.old/new_status: String`（原 `Value`）；测试面按新格式重写 + 三把对齐锁 |
| 宿主 `events/app_event.rs` | `AppEvent` 三方法 + `publish` + `PublishError`（含 6 条入口用例） |
| 宿主 `events/host_sync_event.rs`（新） | `HostSyncEvent(pub SyncEvent)`：`source_device` / `validate` / `to_sync_payload`（4 条锁） |
| 宿主 `events/sync_event.rs` | 镜像 `From` 适配新字段类型；`to_sync_payload` 显式答 `None`（双轨期不接线，票 04 随镜像删除） |
| 宿主 `events/matcher.rs` | 三个测试假事件补 `to_sync_payload` |
| 插件 `session/{mod,model}.rs` + `launch.rs` | 产出口类型化（见第四节） |

### 二、新旧格式对照表（插件 → 宿主这一跳的 JSON）

旧格式一列的**证据**是 HEAD 版 `events.rs` 自己的测试断言（如
`test_sync_task_queue_changed` 断言 `{"type":"TaskQueueChanged","session_id":"s1",…}`），
新格式一列的证据是本票的 SDK 锁用例。

| 变体 | 旧（内部标签，PascalCase，字段平铺） | 新（= 出站 `SyncPayload`） |
| --- | --- | --- |
| SessionCreated | `{"type":"SessionCreated","session":{…自由 JSON…},"source_device":"d"}` | `{"type":"session_created","data":{"session":{…类型化 SessionSummary…},"source_device":"d"}}` |
| SessionStatusChanged | `{"type":"SessionStatusChanged","session_id":"s","old_status":<Value>,"new_status":<Value>,"session_name":"n"}` | `{"type":"session_status_changed","data":{"session_id":"s","old_status":"running","new_status":"stopped","session_name":"n"}}` |
| SessionStopped | `{"type":"SessionStopped","session_id":…,"session_name":…,"source_device":…}` | `{"type":"session_stopped","data":{"session_id":…,"session_name":…,"source_device":…}}` ← **`source_device` 仅信封字段，出站时被剥** |
| SessionRemoved | 同上 | 同上（同剥 `source_device`） |
| TaskStatusChanged | `{"type":"TaskStatusChanged","session_id":…,"task_status":…[,task_reason,task_questions]}` | `{"type":"task_status_changed","data":{…同字段…}}` |
| SessionModeChanged | `{"type":"SessionModeChanged",…}` | `{"type":"session_mode_changed","data":{…}}` |
| TaskQueueChanged | `{"type":"TaskQueueChanged",…}` | `{"type":"task_queue_changed","data":{…}}` |
| TaskScheduledChanged | `{"type":"TaskScheduledChanged",…}` | `{"type":"task_scheduled_changed","data":{…}}` |

`SyncEvent` ↔ `SyncPayload` 的同构由三条锁钉住：
`sync_event_variants_mirror_sync_payload`（逐变体折算后形状全等 + 例外必须被走到）、
`sync_event_and_sync_payload_label_sets_match`（两侧变体标签集合相等）、
`legacy_internal_tag_format_is_rejected`（旧格式点名拒绝）。

### 三、三处偏离 spec 的裁定（都在代码注释里留了由）

1. **`to_sync_payload` 用同 wire 的 JSON 折算，不做逐变体 match**（`host_sync_event.rs`）。
   spec §4.4 允许「适配层机械 From」，但机械 match 一旦落在宿主，就是「解释表面」
   回宿主的第一块跳板（D6 要防的正是这个）。折算把「两侧变体面一致」从宿主代码
   搬到 SDK 的对齐锁：新增变体漏配 → SDK 锁红，而不是宿主 match 里多一条没人走的臂。
   代价（运行期才发现不同构）由 `validate()` 在发布入口挡住并回 `Err` 给生产者。
2. **`validate()` 挂在 `publish` 入口，不在 Handler 里重复**（spec §4.1 图示把 validate
   画在 Handler，§4.5 又要求「validate 失败 → Err 返回 WASM」——只有入口能同时满足，
   处理器在 `tokio::spawn` 里，Err 已无路可回）。票 03 的 Handler 因此不再调 validate。
3. **`publish` 在无事件源时返回 `Err(NoSource)`**（spec 未规定）。底层
   `EventMatcher::publish` 的「无源即丢弃返回 Ok」原语义保留（既有
   `test_publish_without_source_returns_ok` 不动），统一入口补上显性失败——
   静默 `Ok` 正是 AGENTS §8 点名的「线还在、数据永远是空」形态。
   副作用：`broadcast_sync` 在 AppContext 未 init 时的错误由
   「AppContext not initialized yet」变成点名类型名的 `NoSource`（票 03 落地时一并核）。

`source_device` 的例外（`session_stopped` / `session_removed`）是「近恒等」的唯一出处，
`source_device_is_stripped_from_outbound_data` 双向锁：事件侧带、载荷侧不带、其余字段逐字相同。

### 四、插件产出口（票 02 的「生产者负责」部分）

- `session::model::SessionStatus::wire_name()`：概要的 `status` 是**展示字符串**，
  穷尽 match + `wire_name_matches_serde_tag` 与 serde 标签同源。`Error` 的描述文本
  **不进** `status`（该字段是 String，塞对象会让整条载荷在宿主侧解析失败），
  错误详情留在本域记录与 `session-get` 视图。
- `session::summary_view(record) -> wire::SessionSummary` 成为**类型化真源**；
  `summary_json()` 改为其「去 null 键」投影 → 互调 api / WS `session_list` 的
  逐条形状按字节不变（`Error` 态会话除外：原来是 `{"error":"…"}` 对象，
  现在折叠为 `"error"` 字符串，那本来就把宿主的 `SessionSummary` 解析打挂，属修正）。
- `summary_json_for -> summary_for -> Result<SessionSummary, String>`：取不到概要
  从「发一条只有 id 的空概、由宿主 warn 丢弃」改成「生产者侧 warn + 跳过广播」，
  可观测点从消费者挪回生产者。
- 产物已随包重建：`wasmHash = 85bc6b0fc6e334e1613b8f6e84cde67751daf0487c87de52d3738ed6fe771978`
  （`resources/` 在 .gitignore 内，重建是本地构建步骤，不入库）。

### 五、双轨证据

- `sync_handler.rs::dual_track_sync_data_wire_is_identical`：9 条样本（8 变体 +
  SessionCreated 空源设备一例）分别走旧路（`From` + 变体 match）与新路
  （`HostSyncEvent::to_sync_payload`），`SyncPayload` 序列化结果**逐字节全等**，
  且源设备排除口径一致。（只比 `payload` 段：外层 `Message::sync_data` 由同一段代码
  封装，唯一差异是 `timestamp`，比它会引入时序抖动。）
- `legacy_status_debug_reformat_is_the_only_known_divergence`：唯一已知分叉 =
  旧路径的 `format!("{:?}").to_lowercase()`（`waitingInput` → `waitinginput`、
  `Error(Some("boom"))` → `error(some("boom"))`）。该变体**零生产者**（插件从不广播
  `SessionStatusChanged`），故不构成行为变化；用例同时锁住「旧口径输出」与
  「新口径原样透传」，票 03/04 删旧路时若有人想把它请回来，这里会红。
- `broadcast_sync_rejects_pre_alignment_internal_tag_format`：未重建的旧插件产物
  在宿主侧得到点名「unknown or malformed sync event」，同用例反控新格式能通过解析。

### 六、门禁实跑

| 门禁 | 结果 |
| --- | --- |
| SDK `cargo test --lib` | 167 passed / 0 failed |
| SDK `cargo check --features wasm`（`wasm32-unknown-unknown`） | 通过 |
| SDK `cargo check --features wasm`（`nightly-2026-09-16` / `wasm32-wasip3`） | 通过 |
| 宿主 `cargo test --lib` | **1044 passed / 0 failed**，`[skip]` 计数 0 |
| 插件 native `cargo test --lib -- session::model` | 4 passed / 0 failed |
| 插件 `cargo check --target wasm32-wasip3 --no-default-features --features wasm` | 通过（新增 `wire_name` / 类型化概要均编译） |
| 插件产物 | `node scripts/plugin-build.js --plugin com.bedcode.terminal-session` 重建成功 |
| 宿主集成 target / 前端 vitest / eslint | 未跑（票面把全量回归留在票 04；本票未改前端） |

一次噪音记账：首轮全量 `cargo test --lib` 里
`task_e2e::test_task_submit_events_dispatched_and_status` 红（`snap["state"]` 取到
`Null`），单跑即绿、复跑全量亦绿——符合项目记忆「宿主与插件 cargo test 并发跑会让
时序敏感用例连带红」的形态（当时同机有另一场 cargo 在跑），不据此改判据。

### 七、覆盖损失与未尽项

1. `publish` 目前只有单测经 `global_matcher` 跑通，**生产路径还没切**（票 03）；
   切之前 `broadcast_sync` 仍走 `sync_tx`，`AppEvent::validate` 在生产上暂不生效。
2. 宿主四个集成 target（`broadcast_shutdown` / `pty_session_chain` /
   `http_auth_biometric` / `ws_auth_rules`）仍按 `DesktopSyncEvent` 装配，票 03 切。
3. `SessionStatusChanged` 是「纸面变体」（两侧形状都有、零生产者）。本票按 spec 保留，
   是否退役留给票 04 的 D7 清点一并交代。
4. **SDK 的 cargo 测试不在 CI 门禁里**：`test.yml` 只在两端 `src-tauri` 跑 `cargo test`，
   `sdk-publish.yml` 对 Rust 侧只 `cargo check`。票 01/02 把形状锁主战场迁到 SDK 之后，
   这些锁在合并门禁上不被执行（本地与票末命令会跑）。补一条 CI 步骤（如
   `cargo test --manifest-path bedcode-desktop/packages/plugin-sdk-desktop/rust/Cargo.toml`）
   要改 `.github/workflows/test.yml`，属 CI 变更，**待用户裁定**，本票未动。

