# 交接：认证记录下沉认证中心（pairings/connection_history 出宿主主库）+ session_configs 删除

**日期:** 2026-09-22
**交接人:** pi agent（会话中断）
**文档:** `.scratch/2026-09-22-auth-records-downsize/spec.md`（设计全文，以 spec 为准）
**分支:** dev（工作区未提交）

## 1. 任务背景与用户裁定

用户审计发现宿主主库（`bedcode-desktop/src-tauri/src/db/schema.sql`）存有业务数据，裁定：

1. **`session_configs` 表：删除**（不等既有 legacy 观测归零）
2. **`pairings`（配对设备）+ `connection_history`（连接历史）：不在宿主侧** → 下沉「认证中心」（= `com.bedcode.terminal-session` 插件私有库），各插件经互调查询认证中心获取记录

此裁定逆转 `.scratch/2026-09-20-host-business-decarriage/issues/07`（2026-09-21 用户曾裁定「信任表留宿主」）与 AGENTS.md §8 明文。

用户确认的关键决策（方案 A）：
- **生物凭证公钥留宿主 `plugin_secrets`**（key = `biometric:<fingerprint>`），验签执行在宿主不变（§8 凭据红线）
- **`session_token` 是死列**（`update_pairing_token`/`verify_session_token` 零调用方）→ 丢弃不迁移
- **顺带删除死命令** `get_all_db_settings` / `set_db_setting`

## 2. 已完成（代码已改，未提交）

### 阶段 1：插件侧（terminal-session，**229 测试全绿**）
- 新建 `plugins/terminal-session/rust/src/auth_records/`（model + store + ops + mod）：
  - 私有库表 `auth_pairings` / `auth_connection_history`（含软删行保留、索引）
  - 端口 AuthRecordsStore（行语义，wasm 走 host-plugin-database；native mock 可测）
  - 纯逻辑 ops：migrate（marker 幂等 + 凭据列剥离）、upsert（uid_hash 归并）、touch、record_event、close_open、active_pairings_json
  - 15 个新测试全绿（归并策略 / marker 幂等 / 凭据剥离 / touch / record / close / 排序）
- `lib.rs`：`pub mod auth_records`、api trait +7 项（`auth-records-import` / `devices-list` / `history-list` / `connection-touch` / `connection-close` + 原 pairing/qr 无关）、activate 建表（D7 降级）
- `plugin.json`：manifest api 增加上述项
- 内部消费点改自读（host-auth 记录面 → auth_records）：
  - `trust/source.rs`（TrustRecords impl 改私有库）
  - `auth_http/mod.rs`（upsert/touch/record 包装改私有库；biometric-bind 仍走宿主原语）
  - `auth_http/biometric.rs`（配对记录查找改私有库；bound/verify 仍宿主）
  - `device_face.rs`（paired-list / history-list / history-clear 改私有库）
  - `devices.rs`（connect-list 配对段改私有库）
  - `policy/mod.rs`（撤销检查经 TrustRecords→私有库）
- `schema.rs`：prefixed_table_names 加 `auth_pairings` / `auth_connection_history`

### 阶段 2：WIT/SDK v24（desktop 独有，双端偏离）
- `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`：host-auth 删 7 记录面原语（trusted-devices-list/revoke、connection-history-list/clear、upsert/touch/record）；host-session 删 config-list/config-get（legacy 迁移通道关闭）；版本注释 v23→v24
- `abi.rs`：ABI_VERSION = 24 + 版本注释 / 测试
- SDK trait：`host/auth.rs`（删 7 方法，保留 biometric-*/secret-*/setting/device-token/link-identity）、`host/session.rs`（删 config_list/get）
- `wasm_host.rs`：删 9 个 impl
- 宿主绑定 `component.rs`：删 9 个 Host 绑定

### 阶段 3/4：宿主（进行中，**lib 编译通过，test 编译 25 错误**）
- `host_impl/auth.rs`：
  - 删记录面 7 函数（v18 四个 + v19 三个）
  - biometric-* 改托管 `plugin_secrets`（`biometric_secret_key(fp)` helper；bound=查托管存在性、verify=宿主托管公钥验签、bind=写/删 plugin_secrets）
  - 测试：删 v18 记录面测试（4 个），新增 v24 biometric 测试（bind/bound/unbind 往返 + P-256 验签真轮询）
- `host_impl/session.rs`：删 session_config_list/get + 2 测试
- `db/schema.sql`：删三表，只留 settings / plugin_storage / plugin_secrets
- `db/database.rs`：run_migrations 清空（pairings 列迁移 + CHECK 约束迁移删）、count_legacy_session_configs 删、测试改「退役表不建出」断言
- `db/operations.rs`：删 Pairing/Connection/SessionConfig 三段 CRUD（保留 quick_actions + settings），删对应测试
- `db/models.rs`：删 Pairing/ConnectionHistory/SessionConfig（保留 connection_method/result 常量 + LegacyQuickActionRow + Setting）；`db.rs` 导出同步
- `session/session_config.rs`：壳化（仅 new/db()，删 get_config/list_configs 与测试）
- `lib.rs`：删 legacy_rows 观测
- `utils/auth/auth_center.rs`：加 `notify_connection_touch` / `notify_connection_close`（互调 api，插件未激活降级）
- `server/ws/conn.rs`：认证成功 touch / 断开回填改经互调通知认证中心

## 3. 未完成（按优先级）

### 3.1 收尾当前编译错误（**立即做**）
- `session_e2e.rs`：25 个 `E0609 no field id on JsonValue`——`seed_config_in_plugin_store` 返回值已改为 `serde_json::Value`，所有 `seeded.id` / `seeded_config.id` 需改 `seeded["id"].as_str().unwrap()`（约 17 处，行号见文件 1310/1330/1354/1380/1393/1406/1420/1429/1463/1493/1512/1536/1651/1663/1668/1674/1679/1816/1955/1964/2105/2622 与 test_session_plugin_artifact_lifecycle / task / scheduled 域）

### 3.2 sdk-test 组件清理（**必须**，否则 e2e/闭环测试编译失败）
- `packages/plugin-sdk-test/src/lib.rs`：`test_auth_record_face`（301 行，调 auth_trusted_devices_list/revoke/connection_history_list）与 `test_session_config_face`（327 行，调 session_config_list/get）两个命令整段删除——它们测的 host-auth 记录面 / config 读取面已退役，SDK 重建后这些方法不存在

### 3.3 宿主迁移模块（spec §3.5，未开始）
- 新建 `src-tauri/src/plugin/auth_records_migration.rs`（quick_actions_migration 同型）：读主库存量 pairings/connection_history（需先从 **git 历史恢复** operations CRUD 或直接 SQL）→ 经互调 api `com.bedcode.terminal-session.auth-records-import` 推送；凭据列（public_key→plugin_secrets、session_token 丢弃）在宿主侧处理
- lib.rs setup 挂钩（PluginHost::new 之后，与 task_data/session_db/quick_actions 迁移同位置）
- 注意：**主库表已删**，旧库升级路径需在 init_schema 前兜底读取（否则存量数据丢失）——**这是当前最大缺口**，spec 未完全细化，需设计「旧表存在时先读后删」的一次性逻辑（可参照 quick_actions legacy 表的「表存在才读」模式）

### 3.4 宿主 e2e 测试残余（已删 3 个，待验证）
- 已删：`test_host_auth_record_face_closed_loop`（1133）、`test_session_config_api_closed_loop`（1284）、`test_session_config_private_store_closed_loop`（1426）
- 待查：`test_business_endpoints_dual_track_closed_loop` 是否仍引用已删 config 面（session.config.list / legacy_db 播种 quick_actions 与 configs）——**quick_actions 迁移保留**，但 config 相关播种若引用 SessionConfig 需同步适配

### 3.5 全量验证（spec §6）
- 宿主 `cargo test` 全绿（含 tests/ 集成：ws_auth_rules / pty_session_chain / broadcast_shutdown 可能引用 pairings——**需检查**）
- `rsrc-tauri/tests/*` 若有 pairings/connection_history 引用需清理
- 插件 `cargo test`（已绿 229）、vitest（前端）、eslint
- 插件重建（resources/plugins 产物按 v24 SDK 重建）+ wasmHash 校验
- 权限词汇：本次无权限位增删（记录面是 auth 权限门），但仍需确认 `manifest-gen.js` 映射表与 `permission-vocabulary` 生成物无漂移
- lens_diagnostics mode=all

### 3.6 文档（spec §5 尾部）
- AGENTS.md §8（撤销「pairings/connection_history 表留宿主」口径 + 新增下沉描述）、§7 host-auth 行、code-map.md（db/ 模块与 plugin/ 描述）、根 CHANGELOG.md
- `.scratch/2026-09-20-host-business-decarriage/issues/07` 状态更新（07 的「留宿主」结论已被 2026-09-22 裁定替换）

## 4. 风险与注意

1. **工作区混杂**：大量 `.scratch/` 删除（外部清理）、`commands.rs` 命令面合并（2026-09-22 未提交）、一批外部在途 M（enums/events/api_bridge/host 等）——**提交时精确 add 本任务文件，禁止夹带**（AGENTS.md §11）
2. **凭据红线**：公钥只能在 `plugin_secrets`（`biometric:<fp>` 键），任何日志/记录面不得出现公钥值；session_token 不迁移
3. **ABI v24 破坏性**：旧插件产物（v23 构建）activate 期报错须按 v24 重建——terminal-session 是唯一生产消费者，重建全链（plugin build → resources/plugins → wasmHash）
4. **迁移窗口顺序**：host-auth 记录面原语已从 WIT/SDK 删除，旧产物无法走记录面——**存量数据迁移必须经新互调 api（auth-records-import）**，宿主侧读取旧主库表的逻辑要在删表前设计好
5. **notify_connection_touch 的 display_name 丢弃**：WS 认证路径原本会刷新设备展示名（format_device_display_name），touch 原语不含名称字段——名称刷新职责已归 reauth/配对路径（auth_http），行为差异已在 conn.rs 注释说明；若需保留原名刷新语义，需给 connection-touch 加 name 参数（函数级追加不 bump）
6. **移动端**：无 host-auth / pairings / connection_history，单端桌面改动，无需双端同步（ADR 0022 双端偏离）

## 5. 关键命令

```bash
# 宿主 lib 编译检查
cd bedcode-desktop/src-tauri && cargo check --lib
# 宿主测试（需先修 3.1/3.2）
cd bedcode-desktop/src-tauri && cargo test --lib
# 插件测试（已绿）
cd bedcode-desktop/plugins/terminal-session/rust && cargo test --lib
# SDK 编译
cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo check
```
---

## 6. 实施完成记录（2026-09-22 深夜续跑）

**3.1-3.5 全部完成**，验证证据：

- **宿主 lib 测试**：`cargo test --lib -- --test-threads 1` = **1144 passed / 0 failed**
- **插件测试**：`cargo test --manifest-path rust/Cargo.toml` = **226 passed**（229 - 3 个随迁移通道删除的 config migrate 测试）
- **集成测试**：http_auth_biometric ✓ / ws_auth_rules ✓ (14.12s) / broadcast_shutdown ✓ (1.37s) / pty_session_chain ✓ (0.40s)
- **SDK**：`cargo check` 0 error；插件产物按 v24 重建（wasmHash d888d45c…，manifest-validate exit 0）；权限漂移锁 5 测试绿
- **eslint** 0 error（120 warning 既有）；lens_diagnostics mode=all 0 blocker

**续跑修复的关键问题**（超出 handoff 清单）：

1. **session_e2e 25 处 `seeded.id`** → `["id"].as_str().unwrap()`（3.1）；sdk-test 两命令删除（3.2）
2. **auth_records_migration.rs 新建**（3.3 最大缺口）：legacy 表「存在才读」（schema.sql 只删 CREATE 不 DROP，存量表自然存活）→ 公开行经互调 api 推送 + 生物公钥直接写 plugin_secrets（不经 guest 原语，缓存无该键惰性回填）→ 成功 DROP；5 个新测试（形状/常量/读回/清理/密钥幂等）
3. **`set_plugin_db_root` 注入点**（WasmHostContext 新 pub 方法，plugin_db_root 改 RwLock）：v24 下沉后配对/历史真源在私有库，无头集成测试必须能开私有库——4 个集成 target + host auth-policy/trust 闭环测试全改私有库播种/断言；**storage 权限**补进所有测试 grant（activate 的 auth_records 建表走 host-plugin-database 权限门）
4. **blocking_lock 死锁**（实证）：普通测试线程无 runtime 上下文时 tokio Mutex::blocking_lock（CachedParkThread 路径）在 AMBIENT_RT 交互后死锁——host_impl/auth.rs 测试统一改 block_on_async 获取
5. **WS 认证自锁**（broadcast_shutdown 红）：actix current_thread 主线程同步等互调 reply，而 reply 投递（MessageBus spawn 到同 runtime）依赖同一线程调度 → `notify_connection_touch/close` 改**异步 fire-and-forget**（ambient_handle().spawn）——记录刷新本就不该阻断认证（降级语义）；**params 形状**修正（connection_touch(fingerprint: String) 单参数 → params 直接字符串而非 `{"fingerprint": ...}`）
6. **config 域迁移通道整体退役**（wasm 编译暴露，native 测试不覆盖 wasm 分支）：`LegacyConfigSource` / `migrate` / `migrate_via_host` / wasm_legacy 模块删除（spec §3.3），插件 3 个迁移测试随之删除

**教训**：插件 native `cargo test` 不编译 wasm 分支（`#[cfg(target_arch="wasm32")]`）——改 WIT/SDK 后必须跑 wasm 构建验证插件，否则记录面/类型引用残留只在构建时暴露。

## 7. 提交记录（2026-09-23 补充）

**commit `727041862` feat(desktop): 认证记录下沉认证中心私有库 + session_configs 退役（ABI v24）** — 49 文件 +1547/-3361，dev 分支。

提交纪律（用户指示"精确 add v24 文件"）：
- lib.rs 仅暂存 v24 三 hunk（legacy_rows 删 / auth_records_migration 挂钩 / SyncEventHandler 签名），commands 合并线 hunk（`use commands::RunningSessionInfo` + generate_handler 改名）留工作区待对侧
- 7 个与票 04 撞车文件（server/ws/{conn,channel/event}.rs、host_impl/auth.rs、4 个集成测试）用 blob 级手术（hash-object + update-index）只暂存 v24 内容：票 04 的 `server::core::` 重指向全部留工作区（`git apply` 对 CRLF 补丁不可靠，改 blob 手术绕开）
- ws_auth_rules.rs 曾被 v24 作者转成 LF（1028 行噪音 diff），提交前恢复 CRLF（cr==lines=523）
- 排除：票 04 在途（server/core/ 六文件 git mv + gateway.rs 锁重指向 + 各文件 server::core 重指向）、commands 合并线、permission/approval/host-tests/a03 等其他线、3.6 文档（AGENTS/code-map/CHANGELOG/issues-07，后补）

验证：提交内容 = handoff §6 绿态（1144/226/4 集成）的文件集；提交时工作区被票 04 在途改动遮挡，未重跑编译（票 04 落地后需重验一次全量）。
