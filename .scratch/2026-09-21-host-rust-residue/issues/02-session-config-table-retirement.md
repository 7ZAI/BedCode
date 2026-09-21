# 02: `session_configs` 表与 legacy 配置通道退役（含 marker 守卫）

**What to build:** 退役主库 `session_configs` 业务表、宿主 `SessionConfigManager` 与
`host-session.config-*` 三原语（**⑤ 修正：连同 `config-list` 共四条**）。它们在
v21 之后只剩一个用途：**插件的一次性 legacy 迁移通道**——`com.bedcode.session` 激活时经
`LegacyConfigSource`（`plugins/session/rust/src/config/ops.rs`）读老库配置行导入插件私有库
（`plugin_meta` marker 幂等）。迁移覆盖到位后整段退役。
**（2026-09-21 前提修正：写下这句时 `config-list` 另有 5 处 task 域运行时消费者，
「只剩迁移用途」并不成立；本批已把 5 处改读私有库真源，见 Comments ⑤。）**

**决策依据:** ADR 0022 裁剪线（业务数据真源在插件、宿主不留副本）；宿主业务清零规格
决策 3「业务数据真源进插件、宿主不留副本（宿主表退役为 contract）」；
`.scratch/2026-09-19-terminal-session-plugin` 票 08/09（真源迁私有库、投影退役）。

**Blocked by:** 数据侧确认——**各安装点 migration marker 必须已跑过**（本票先在代码里加守卫，
见下①，再由发布侧确认升级路径覆盖后再删表）

**Status:** 阶段 A done（2026-09-21）；**阶段 B 前置 done**（2026-09-21，见 Comments ⑤——
config-list 的运行时消费者已改读插件真源，主库表已无任何写者）；阶段 B 删除本体 **blocked**
（待发布侧确认各安装点 migration marker 已跑过）

## 阶段 A：加「未迁移则仍可迁移」守卫（可立即做，不改行为）

- [x] 宿主启动/插件激活路径：`session_configs` 仍有行且插件 marker 未置位时，**保留**迁移通道
      （现状即如此，本阶段只把它显式化：加一条 `info!` 结构化日志计数 legacy 行数 + marker 态，
      供发布侧判断「还有多少未迁移安装」）
- [x] 加一条测试：marker 未跑 + 有 legacy 行 → 迁移执行且幂等（现用例
      `test_session_config_private_store_closed_loop` 已覆盖，补「marker 已跑 + legacy 新增行 →
      不再导入」的反例编号注释）

## 阶段 B：删除（发布侧确认后）

- [ ] 插件侧：`LegacyConfigSource`（`plugins/session/rust/src/config/ops.rs`）与
      `config/store.rs::MIGRATION_MARKER` 相关分支退役；`plugins/session/rust/src/lib.rs`
      的 `config-list/get` 调用点清理（**只剩迁移用途，见 Comments ⑤：运行时消费者已迁走**）
- [ ] WIT（ABI **v23**——v22 已被票 04 的 `host-platform.reveal-in-dir` 占用，
      见本目录 04 的实施记录；desktop-only）：删 `host-session.config-upsert` / `config-get` /
      `config-delete`（**外加 `config-list`：它同为迁移读取面，票面原写「三原语」是漏计**）；
      SDK `HostSession` 四方法 + `wasm_host.rs` 绑定删除
- [ ] 宿主：`host_impl/session.rs` 四实现 + `component.rs` 四转发；
      `SessionConfigManager`（`session/session_config.rs`）+ `SessionContext.config_manager`
      + `db/schema.sql` 的 `session_configs` 表 + `db/operations.rs` 相关 CRUD + `AppContext`
      / `lib.rs` 装配 + `host_impl/tests::build_host_ctx` 两库分离逻辑清理；
      `Database::count_legacy_session_configs`（阶段 A 的观测信号）随之删除
- [ ] `config.rs` / `commands` 文档 / AGENTS §7 计数 / CHANGELOG 同步
- [ ] **同批裁决（⑤ 的新发现）**：`events/sync_handler.rs` 的 `handle_config_created` /
      `handle_config_updated` 读的正是这张已无写者的表——表删掉后 `DesktopSyncEvent::Config*`
      三个变体与这两个 handler 一起失去数据源。它们出现在移动端 wire 面上，属本规格
      Out of Scope（须与移动端专项同批），阶段 B 动手前先定「随表一起注销」还是「改由插件投递」
- [ ] 注：宿主 Tauri 侧配置命令面（`create/list/get/delete/update_session_config`）已在票 05
      一并注销（该命令面与插件迁移通道无关——迁移走 `host-session.config-list|get` 原语），
      故阶段 B 只剩「原语 + 表 + 管理器」三件

## 验收

- [x] 阶段 A：桌面 `cargo test --lib` 全绿（1088/0，含新增的计数用例）；插件侧 `legacy_rows`
      在激活日志可观测（`test_session_config_private_store_closed_loop` 之外另加
      `migrate_is_idempotent_and_repeatable` 的三条编号断言）
- [ ] 阶段 B：`grep -rn "session_configs" src-tauri/src` 仅剩迁移期一次性 SQL 或为空；
      `grep -rn "SessionConfigManager" src-tauri/src` 为空；SDK/插件两侧编译与用例全绿
- [ ] 升级路径回归：老库（含 `session_configs` 数据）→ 新版本启动 → 插件私有库可见同 id 配置；
      全新安装（无该表）不受影响

## Comments ④ 阶段 A 实施记录（2026-09-21）

- 信号两处（互补，避免「插件未激活就看不出未迁移安装」）：
  - **宿主启动**：`Database::count_legacy_session_configs()`（表缺失返回 0，阶段 B 删表后仍可调用）
    → `info!(legacy_rows = …, "legacy session_configs rows present at startup")`；
  - **插件激活**：`MigrationReport` 增 `legacy_rows`（本次扫描到的清单长度；`already_migrated`
    时为 0——没扫不报数），激活日志改为
    `session config store ready (legacy_rows=… imported=… skipped_existing=… already_migrated=…)`。
- 用例：宿主 `count_legacy_session_configs_tracks_rows_and_survives_table_drop`（0 行 → 1 行 → 删表归零）；
  插件 `migrate_is_idempotent_and_repeatable` 加 C-02-1/2/3 编号（正例报 2 行；已迁移报 0；
  迁移后 legacy 新增行不导入），`migrate_skips_rows_that_vanished` 断言竞态下扫描数不缩水。
- 行为零变化：迁移时机、一次性语义、幂等语义均未动。

## Comments

### ① 为什么必须带守卫（数据安全）

`session_configs` 是**用户配置的唯一旧副本**。删表前若某安装点尚未跑过迁移，数据即不可恢复。
故顺序固定为「阶段 A 落观测信号 → 发布侧确认 → 阶段 B 删」。**禁止**在阶段 A 直接停机路径删表。

### ② marker 的两个性质（沿用票 08 口径）

一次性（marker 存在即不重跑，否则「插件侧删除被 legacy 复活」）+ 幂等（marker 之外仍按 id
判存在，进程崩溃后重跑收敛）。删表前需确认两条性质在目标版本仍成立。

### ③ 与票 01 的关系

票 01（重启广播总线）与本票无耦合，可并行；但若两票同期实施，建议先 01（纯死代码，风险低）
再 02（数据 + ABI）。

### ⑤ 阶段 B 前置实施记录（2026-09-21）——票面前提修正 + 主库表已无写者

**① 票面前提修正：`config-list` 不是「只剩迁移用途」。** 清点时发现 `com.bedcode.session`
的 task 域有 **5 处运行时**在读 `host-session.config-list`（即读主库那张已冻结的表）：

| 消费点 | 用途 |
| --- | --- |
| `task/state.rs::session_command` | 会话启动命令 → `detect_agent`（决定 `on_input_submitted` 是否建任务行） |
| `task/state.rs::session_working_dir` | 会话工程目录（agent 集成 / hooks 定位） |
| `task/state.rs::backfill_working_dirs` | 历史任务行按 `session_id → configId → workingDir` 回填 |
| `task/state.rs::list_running_sessions` | 「当前任务」Tab 的会话标签 workingDir |
| `task/hooks.rs::cleanup_all_agent_integrations` | 遍历全部配置清理项目内 agent 集成 |

而这张表在本批之前**已无任何写者**（唯一写入口 `host-session.config-upsert|delete` 零消费者，
主库投影写随 v21 停写）。两侧一冷一热 = 新装点与迁移后新建的配置对 task 域不可见。
本批把 5 处改读插件私有库真源（`crate::config::list_via_host()`），语义修正且不改 ABI。

**② 顺带注销「只剩壳」的宿主侧（阶段 B 的删除清单因此变短）**

- 宿主投影写入口 `SessionConfigManager::upsert_config` 删除（自带注释「票 09 落地后随旧表退役」，
  读者已于 v21 消失）
- `utils/session_config_bridge.rs` 整文件删除：票 05 注销宿主配置命令面后它失去主体
  （prod 零调用，仅测试引用）→ 4 处测试播种改走插件命令面 `session.config.upsert`
  （新增 `tests::seed_config_in_plugin_store` 助手，生产同构）
- `SessionConfigManager::{from_database, create_config, get_config_by_session_id}` 零消费者删除
- 同域死代码一并清理：`event_bus.rs` 收缩（`SessionEvent` / `SessionEventBus` / `publish` /
  `subscribe` 全无外部消费者，状态广播内联为 `SessionManager::status_tx`）、
  `SessionManager::{resource_dir 字段, cleanup_stopped_sessions}` 删除（`new` / `new_with_handlers`
  随之去掉 `resource_dir` 参数）

**③ 门禁证据**

- 宿主 `cargo test --lib` **1057 passed / 0 failed**，`[skip] = 0`（产物已重出），
  `wasm_runtime::tests::session_e2e` 12 项闭环全绿（含 `test_session_config_private_store_closed_loop`、
  `test_session_task_domain_closed_loop`、`test_business_endpoints_dual_track_closed_loop`）
- 插件 `cargo test` 207 passed / 1 failed：`declares_only_landed_domain_surface` 断言
  `manifest.permissions` 与硬编码期望表的**顺序**不一致（`task:run` 在 manifest 第 11 位、在期望表末位）——
  HEAD 既存红，属 host-task-concurrency 线，本批未代改
- `cargo check --lib` 与 `cargo check --lib --tests` 除既存 warning 外无本批错误
- **既存红登记（非本批引入）**：`cargo test` 的 4 个集成 target（`pty_session_chain` /
  `ws_session_route` / `http_auth_biometric` / `broadcast_shutdown`）在 HEAD 即不能编译——
  引用了已随认证中心退役与 v21 删除的 `QrTokenManager` / `PairingService` /
  `AppContextBuilder::pairing_service` / `SessionManager::{from_database, restart_session}`。
  CHANGELOG「Tests & Quality」里那句「线协议回归零断言改动」对这些 target 已失效，
  修复归认证下沉线，不在本票
- **待补的变异自检**（本批实施时被另一在途会话的未跟踪 `terminal_output_perf.rs` 挡住 lib test
  重编，故未跑）：把 `tests/session_e2e.rs` 任务域探针的播种临时改回主库
  （`cm.create_config_full` 替代 `seed_config_in_plugin_store`）→
  `test_session_task_domain_closed_loop` 必须转红（断言 `taskStatus = in_progress` 拿不到，
  因 agent 判不到 claude）。红了才说明该 fixture 真在测真源链路。
