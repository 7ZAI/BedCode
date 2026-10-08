# 票 05 · host-database/storage 机制对齐 13 原语（阶段 1 第三票）

> 状态：**05a（机制对齐）已完成（2026-10-07）**；**05b（host-storage 迁主库表）
> 代码已完成（2026-10-07 晚间；SQL 语义独立验证 5/5 绿，宿主编译验证被并行会话
> 在途 egress.rs 阻塞——见 §6）**；05c（插件三方调用点）已核实**无需代码改动**（见 §7）。
> D4 用户拍板：**统一 13 原语** + 「移动端引入 db 存宿主与插件数据，对齐桌面端」。

## 1. 目标与本票范围

spec 票 05：「移动端 db 域对齐 13 原语语义（权限门/表名前缀/护栏/属主分区），
`bedcode_plugins.db` 迁移；插件三方调用点同批迁」。

**05a（本票）**：
- ✅ WIT：host-database 增 3 函数（execute-params/query-params/execute-batch）+ 新增
  host-plugin-database interface（5 函数）——13 原语形状齐
- ✅ SDK：HostDatabase trait 3 新方法 + HostPluginDatabase trait 5 方法 + wasm_host 绑定
- ✅ 宿主：component.rs 两个 impl + **add_to_linker 注册（根因修复）** + host_impl/db.rs
  重写（参数化/批处理/结果集护栏/裸事务拒绝/插件私有库）+ WasmHostContext.plugin_dbs
- ✅ 主库 schema：`plugin/db_schema.rs`（5 表幂等）+ lib.rs 接线（打开 bedcode_plugins.db 后建表）
- ⏳ 05b：host-storage 迁主库 plugin_storage 表（现为 PluginStorage 文件落盘）
- ⏳ 05c：插件三方（ai-chatbox/auto-task/file-transfer）调用点迁移

## 2. 改动清单（12 文件）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| WIT | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | host-database +3 函数；新增 host-plugin-database（5）；world 加 import |
| SDK | `src/host/database.rs` | HostDatabase +3 方法；新增 HostPluginDatabase trait（5） |
| SDK | `src/wasm_host.rs` | impl 绑定（+import HostPluginDatabase + host_plugin_database 模块） |
| SDK | `src/host/mod.rs` | pub use HostPluginDatabase + HostApi 聚合 |
| 宿主 | `component.rs` | host_database impl +3；host_plugin_database impl（新）；**add_to_linker 注册 host_plugin_database（根因）** |
| 宿主 | `host_impl/db.rs`（重写） | 主库 5 函数（execute/query/params/batch + 前缀校验 + 裸事务拒绝）+ 插件库 5 函数（plugin_db_conn 惰性打开/缓存/属主分区）+ 结果集护栏（10,000 行/32MB）+ execute-batch（64 语句/事务/回滚） |
| 宿主 | `wasm_runtime.rs` | WasmHostContext 加 `plugin_dbs` 字段（new 内初始化，调用点零改动） |
| 宿主 | `host_impl.rs` | db re-export 改显式（含 plugin_db_*） |
| 宿主 | `plugin/db_schema.rs`（新） | SCHEMA_SQL（settings/plugin_storage/plugin_secrets/plugin_auth_policies/plugin_auth_records，幂等）+ init_schema + 幂等测试 |
| 宿主 | `lib.rs` | 打开 bedcode_plugins.db 后 init_schema |
| 宿主 | `plugin.rs` | 声明 db_schema 模块 |
| 测试 | db.rs 内联 + db_schema.rs | 5 + 2 测试（参数化往返/前缀纵深/batch 回滚/裸事务拒绝/插件库缓存/幂等建表/主键约束） |

## 3. 与桌面 13 原语语义对齐 + 差异点名

| 语义 | 桌面 | 移动端 | 差异 |
| --- | --- | --- | --- |
| 权限门 | 函数级 check_permission | 统一 PERMISSION_STORAGE 门禁 | 移动端既有形态，5+5 函数全覆盖 |
| 表名前缀纵深 | SQLite authorizer 回调（AuthorizerGuard） | 正则提取表名 + 前缀校验（wasm_host::validate_sql_table_prefix） | **实现强度差异**：正则覆盖主库表名校验（CREATE/INSERT/SELECT 等语句的表名），语义护栏等价；authorizer 可拦截表达式/列级访问，移动端未移植（交付点名） |
| 结果集护栏 | 行数 10,000 / 字节 32MB | 同值移植（push_row_capped） | 无 |
| 裸事务拒绝 | execute 层 + batch 层 | 同（db_execute/params + batch 内） | 无 |
| execute-batch | 事务内顺序 + 64 语句上限 | 同 | 无 |
| 插件私有库 | plugin_db(plugin_id)（端口） | host_ctx.plugin_dbs 惰性打开（plugins/<sanitized_id>.db）+ 连接缓存 + 生命周期回收 | 库文件位置不同（移动端 plugins/ 目录）；回收机制：host_ctx 每次激活新建，连接随 Drop 关闭（无需显式 purge） |
| 语句超时护栏 | progress handler（rusqlite hooks） | **未移植**（rusqlite 未开 hooks feature） | 护栏缺口留 D1 审计项（SQL 由主库 Mutex 串行，无并发阻塞风险；慢查询仍可能长时间占用连接） |

## 4. 根因修复记录（重要）

**症状**：host-plugin-database 的 5 个 host_impl 函数编译期报 `never used` dead code
warning，尽管 component.rs 的 impl 明确调用。
**根因**：`add_to_linker` 未注册 host_plugin_database → bindgen trait 未实例化 →
dead_code 分析把 impl 方法体的调用视为 unused（host_database 因已注册而计数正常）。
**修复**：component.rs linker 注册表加
`bedcode::plugin::host_plugin_database::add_to_linker`。
**验证**：PROBE 实验（impl 里引用不存在函数报 E0425 证明 impl 被编译）+ 注册后 0 warning。
**教训**：新增 WIT interface 必须同时完成「impl + add_to_linker 注册」——不注册不仅
lint 报 dead code，插件运行期 import 也会在 linker 层失败。

## 5. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| SDK `cargo check` | ✅ 0 error |
| 宿主 `cargo check --lib` | ✅ **0 warning 0 error**（45.92s） |
| db 针对性测试（`cargo test --lib db`） | ✅ **7 passed**（参数化往返/前缀纵深/batch 回滚/裸事务拒绝/插件库缓存/schema 幂等/主键约束） |
| 全量 lib 测试 | ✅ **373 passed / 0 failed**（49.71s） |
| `cargo fmt --check`（本任务文件） | ✅ 干净（全仓 Diff 均为并行会话在途文件） |
| `cargo clippy --lib` | ✅ 无本任务新增（剩余为既有基线） |
| 五同步点 | ① SDK 常量（PERMISSION_STORAGE 既有复用）② manifest 校验（loader 既有）③ 前端（WASM 面，无前端 capability）④ 宿主能力清单（component.rs impl + add_to_linker ✅）⑤ 权限门（require_storage_permission ✅） |

## 6. 遗留（后续票）

- **05b（已完成 2026-10-07 晚间）**：host-storage 迁主库 plugin_storage 表。
  - 改动：`plugin/storage.rs` 重写为 DB-backed（`PluginStorage::new(plugin_db)`，
    get/set/delete/clear_plugin 走 `plugin_storage` 表 upsert/分区删除；`flush` 退役）；
    `PluginStorage::migrate_file_store_to_db` 启动迁移（`plugins/*.json` → 表，
    INSERT OR IGNORE 不覆盖 DB 新值，成功导入后删文件——旧读路径删除，fail-visible ①）；
    `lib.rs` 重排（DB 开库+init_schema → 05b 迁移 → peer_migration(DB-backed) →
    PluginManager）；`peer_migration.rs` 改收 `&PluginStorage`（去掉 `flush` 调用）；
    测试构造点 7 处改 `PluginStorage::test_storage()`（内存库+schema）。
  - 验证：SQL 语义独立验证 5/5 绿（往返/upsert/属主隔离/clear 分区/迁移幂等+
    不覆盖 DB/`.db` 不碰）。**宿主实测（并行会话 egress.rs 落地后）**：
    `cargo check --lib` 0 error（2 warning 均在并行会话 egress.rs）；针对性测试全绿——
    storage 6 / db 9 / fs_auth 4 / approval 6 / mdns 11 / http 31 / manager 9 /
    loader 1 / component 10；**全量 `cargo test --lib` 380 passed / 6 failed——
    6 个失败全部是并行会话 egress.rs 新增测试（default_tier_consults_records 等，
    非本票文件，按 AGENTS §11 不碰）**。修复 2 处被忽略的 build_host_ctx 调用点
    （manager.rs attach_wasm / loader.rs）与 manager setup_manager 缺 init_schema
    （05b 后测试镜像生产启动顺序）。插件三 crate lib 编译：file-transfer ✓ /
    ai-chatbox ✓ / auto-task ✗（后者的 WasmHost/WasmPlugin/wasm_entry 导入失败为
    票 04 SDK 导出改名在途，与 storage 无关）；插件**测试**被票 04 MockHost 缺
    5 新方法阻塞（非本票）。
- **05c（已核实无需代码改动，2026-10-07 晚间）**：插件三方调用点迁移。
  - 核实结论：ai-chatbox / auto-task / file-transfer 的 Rust 侧**均不使用
    host-database / host-plugin-database**（全仓 `grep` 无 db_execute/db_query/
    HostDatabase 引用），只用 host-storage 3 原语（`settings_store` / `transfer_store` /
    `roots_registry` / `device_bridge` / `peer.rs` 的 `storage_get/set/delete`）——
    05b 把 host-storage 真源从文件换到主库表后，插件 WIT/调用点零改动即可读面切换
    （三插件 manifest 均已含 `storage` 权限）。
  - 原票三项中「参数化优先」「表迁 plugin-database」均不适用（无 host-database 用量）；
    「resume-all-transfers 相关」属票 06 同批，随票 06 处理。
  - 插件三 crate 生产代码对不变 SDK 编译：file-transfer ✓ / ai-chatbox ✓ /
    auto-task ✗（WasmHost/WasmPlugin/wasm_entry 导入失败——票 04 SDK 导出改名在途）；
    三 crate 的**测试**均被票 04 MockHost 缺 5 新方法阻塞，非本票引入。
- 表名前缀纵深强度（authorizer）与语句超时护栏：随 D1 审计项（D1 慢查询阻塞）另行立项。
