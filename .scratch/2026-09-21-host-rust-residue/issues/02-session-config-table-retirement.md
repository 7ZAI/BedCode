# 02: `session_configs` 表与 legacy 配置通道退役（含 marker 守卫）

**What to build:** 退役主库 `session_configs` 业务表、宿主 `SessionConfigManager` 与
`host-session.config-*` 三原语（`config-upsert` / `config-get` / `config-delete`）。它们在
v21 之后只剩一个用途：**插件的一次性 legacy 迁移通道**——`com.bedcode.session` 激活时经
`LegacyConfigSource`（`plugins/session/rust/src/config/ops.rs`）读老库配置行导入插件私有库
（`plugin_meta` marker 幂等）。迁移覆盖到位后整段退役。

**决策依据:** ADR 0022 裁剪线（业务数据真源在插件、宿主不留副本）；宿主业务清零规格
决策 3「业务数据真源进插件、宿主不留副本（宿主表退役为 contract）」；
`.scratch/2026-09-19-terminal-session-plugin` 票 08/09（真源迁私有库、投影退役）。

**Blocked by:** 数据侧确认——**各安装点 migration marker 必须已跑过**（本票先在代码里加守卫，
见下①，再由发布侧确认升级路径覆盖后再删表）

**Status:** 阶段 A done（2026-09-21）；阶段 B **blocked**（待发布侧确认各安装点 migration marker 已跑过）

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
      的 `config-list/get` 调用点清理
- [ ] WIT（ABI **v23**——v22 已被票 04 的 `host-platform.reveal-in-dir` 占用，
      见本目录 04 的实施记录；desktop-only）：删 `host-session.config-upsert` / `config-get` /
      `config-delete`；SDK `HostSession` 三方法 + `wasm_host.rs` 绑定删除
- [ ] 宿主：`host_impl/session.rs` 三实现 + `component.rs` 三转发；
      `SessionConfigManager`（`session/session_config.rs`）+ `SessionContext.config_manager`
      + `db/schema.sql` 的 `session_configs` 表 + `db/operations.rs` 相关 CRUD + `AppContext`
      / `lib.rs` 装配 + `host_impl/tests::build_host_ctx` 两库分离逻辑清理；
      `Database::count_legacy_session_configs`（阶段 A 的观测信号）随之删除
- [ ] `config.rs` / `commands` 文档 / AGENTS §7 计数 / CHANGELOG 同步
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
