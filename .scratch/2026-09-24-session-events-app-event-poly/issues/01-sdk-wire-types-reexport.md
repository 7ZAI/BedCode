# 01: SDK 收编 wire 类型，宿主 enums 改 re-export

**What to build:** 会话同步与 WS 控制的线协议类型有单一事实源：`SyncPayload` / `SessionSummary` / `SessionControl*` / `Terminal*` / `KeyCombo` 全部定义在 `bedcode-plugin-api`，宿主 `enums/` 只做 re-export。开发者改字段名只改 SDK 一处，宿主与插件编译期同步；**运行行为零变化**——现有 `DesktopSyncEvent` 广播路径与移动端收到的 wire 逐字节不变。

**Blocked by:** None (can start immediately).

**Status:** done（2026-09-24 落地：SDK 新增 `wire` 模块收编五类 wire，宿主四个 `enums/`
文件缩为 re-export 垫片 + 两把反双份锁；实施记录与本票覆盖损失见票末）

- [x] SDK `bedcode-plugin-api` 收编 `SyncPayload`、`SessionSummary`、`SessionControlPayload`、`SessionControlAction`、`TerminalPayload`、`TerminalAction`、`KeyCombo`；serde 属性与字段与现宿主定义逐字一致（adjacently tagged snake_case + `data` 等）
- [x] 宿主 `enums/sync.rs`、`enums/summary.rs`、`enums/control.rs`、`enums/special_key.rs`（被 Terminal 嵌入的 `KeyCombo` 面）缩为 `pub use bedcode_plugin_api::…`；既有 `crate::enums::*` 导入路径零改动
- [x] 形状锁测试主战场迁到 SDK：全变体 wire JSON 往返 + type 标签锁 + `KeyCombo` parse/serialize；宿主侧保留等价锁或改为依赖 SDK 锁（不出现双份漂移）
- [x] SDK `SyncEvent` **本票不改** wire（仍内部标签）；与 `SyncPayload` 的格式差留给 02
- [x] `DesktopSyncEvent` / `From` / Handler **原样保留**，`cargo test` 全绿证明零行为变化
- [x] 不 bump ABI / 不改 WIT / 不动移动端代码

---

## 实施记录（2026-09-24）

### 一、落点：SDK `bedcode-plugin-api::wire`（不是 spec §4.3 片段里的 `events::`）

新增 `packages/plugin-sdk-desktop/rust/src/wire.rs`（模块入口 + 全量 `pub use`）与
`wire/{sync,summary,control,key}.rs`。与 spec 的 `bedcode_plugin_api::events::SyncPayload`
写法有意偏离，理由：

- `events.rs` 讲的是**插件面**「能发布什么」（`SyncEvent` / `ProcessDoneEvent`），
  `wire` 讲的是**跨端线上形状**（宿主 ↔ 移动端的 `sync_data` / `session_control` /
  `terminal` 帧）。混进 `events` 会让「插件能构造的类型」和「移动端能收到的类型」
  在同一个文件里叠成两套标签口径，正是本专项要消灭的形态。
- 宿主 `enums/{sync,summary,control,special_key}.rs` 与 SDK `wire/` 下的文件**一一对应**，
  垫片可逐文件 diff 核对「搬空了没有」。
- 票 02 的 `AppEvent::to_sync_payload` 返回类型写全路径
  `bedcode_plugin_api::wire::SyncPayload` 即可，不需要为示例片段制造 `events::` 别名。

### 二、逐字搬迁的证据（不是「我看着一样」）

新文件统一 **LF**（`.editorconfig` 的 `end_of_line = lf`；宿主原文件是 CRLF），
所以用 `diff --strip-trailing-cr` 对 `git show HEAD:` 的原件做证伪比对：

| 目标 | 与 HEAD 原件的差异 |
| --- | --- |
| `wire/control.rs` | 仅头注释 + `use super::special_key` → `use super::key`（**测试体零改动**） |
| `wire/key.rs` | 仅头注释（40 余条 parse / to_pty_bytes / serde 用例随文件原样迁入） |
| `wire/sync.rs` | 头注释 + `PluginQuestion` 改从 `crate::events` 取 + 新增测试 |
| `wire/summary.rs` | 头注释 + 新增测试（原件无测试） |

即：字段名、serde 属性、变体顺序逐字未动，「运行行为零变化」是按定义成立的。

### 三、宿主侧的两把反双份锁（`src/enums.rs::tests`）

1. **类型身份锁（编译期）**：`let _: fn(bedcode_plugin_api::wire::SyncPayload) ->
   crate::enums::SyncPayload = |s| s;`（十项，含 `events::PluginQuestion`）。
   宿主路径与 SDK 真源若不是同一个类型，`|s| s` 即 E0308——双份定义各自绿的形态被挡在门外。
2. **源层面 grep 锁**：`wire_shim_files_contain_no_definitions` 逐行扫四个垫片
   （加既有的 `plugin.rs`），非注释行只允许 `pub use` 及其展开条目，出现
   `pub enum` / `struct` / `impl` / `#[derive` / `fn` 等即列违规。

**变异自检**（判这条锁不是恒真）：对 HEAD 的旧内容跑同一套判据，命中数
`sync=4 / summary=2 / control=14 / special_key=64`，现网四个文件均 0；
`plugin.rs` 在 HEAD 本就是垫片 → 两侧都 0（是真阴不是假阴）。

宿主侧不再复制 SDK 的形状锁：`Message::SyncData` 的往返套件
（`server/websocket/message.rs::json_round_trip_preserves_all_variants` 等 67 条）
经 re-export 路径**实际执行** SDK 代码，是票面「改为依赖 SDK 锁」的落点。

### 四、跨端形状锁（移动端零改动的前提下）

SDK 新增 `wire::sync::mobile_parallel_copy_shape_lock`：十条**逐字抄自**
`bedcode-mobile/src-tauri/src/enums/sync.rs` + `sumary.rs` 的样例 JSON 进真源反序列化，
再序列化后按 `flatten_data` 语义比对；`every_variant_label_round_trips` 另钉住
「桌面当前只发这 8 个变体」的清单，`config_variants_are_not_produced` 钉住移动端多出的
`Config*` 三变体不得被桌面悄悄恢复（恢复即宿主重新解释配置语义）。

`flatten_data` 的存在是双端序列化属性的真实差异：移动端 `TaskQueueChanged` 的
`task_id`/`status` 只有 `#[serde(default)]` 没有 `skip_serializing_if` → 缺省出 `null`，
桌面出「键缺失」。二者语义等价，逐字比对会假红，故按语义比对并在用例注释里写明缘由。

### 五、纠正 spec 的一处事实错误

spec §4.2 表格写 `SessionSummary` 是 **camelCase** —— **不成立**。实测三方
（宿主 `enums/summary.rs`、移动端 `enums/sumary.rs`、插件产出口
`plugins/terminal-session/rust/src/session/mod.rs::summary_json`）**都是 snake_case**
（`created_at` / `session_type` / `config_id` / `task_status` / `task_reason`）。
`wire/summary.rs::wire_keys_are_locked` 把真实键名钉死。
连带：SDK `events.rs` 里 `SyncEvent::SessionCreated` 的文档注释同样写着 camelCase，
属陈旧注释——它在 SyncEvent 面上，随票 02 一并按实测 wire 修正（本票不动 SyncEvent）。

### 六、`KeyCombo` 的引擎面随类型一起进 SDK

票面只点名「被 Terminal 嵌入的 `KeyCombo` 面」，但 `pub use` 是整类型搬迁，
`to_pty_bytes` 走不了旁路：宿主 `pty/pty_process.rs:send_special_key` 是**真生产调用点**
（`interrupt()` 在 355 行用它发 Ctrl+C），不是死代码。故按 spec §8 风险表既定的
「re-export 后调用方零改动」处理，引擎路径不迁。

**留下的重复**：插件 `keys.rs`（票 06 下沉时自带一份KeyCode/KeyCombo + 转义规则）。
本票按最小改动不合并它，已在宿主 `enums/special_key.rs` 的注释里记账为后续去重项——
去重方向应是插件改吃 SDK 定义，而不是再造第三份。

### 七、门禁实跑

| 门禁 | 结果 |
| --- | --- |
| SDK `cargo test --lib` | 162 passed / 0 failed（迁入的 40 余条 + 新增形状锁） |
| 宿主 `cargo test --lib` | **1031 passed / 0 failed**，`[skip]` 计数 0 |
| 宿主 `cargo check --lib --tests` | 全测试目标编译通过，无本票文件的告警 |
| SDK `cargo check --features wasm`（stable / `wasm32-unknown-unknown`） | 通过 |
| SDK `cargo check --features wasm`（`nightly-2026-09-16` / `wasm32-wasip3`） | 通过（插件编译目标） |
| 宿主集成 target（`tests/*.rs`）| **未跑**——票 01 零运行时变化，全量回归按 spec §7 留票 04 |
| 前端 `pnpm run test:run` / `eslint` | 无涉（未改前端） |

### 八、并发在途记账

本票执行期间 `src-tauri/src/wasm_core/manager/runtime.rs` 被同 worktree 的另一场会话
改坏过一瞬（`WASIP3_NIGHTLY` 常量暂缺 → lib test target 7 条 E0425，全不在本票文件），
未碰对侧文件；对侧落地后复跑即绿。上表的宿主 1031/0 是复跑后的数。

