# 认证记录下沉认证中心：`pairings` / `connection_history` 出宿主主库 + `session_configs` 删除

**日期:** 2026-09-22
**状态:** ready-for-agent（设计已获用户确认方向）
**涉及端:** desktop 单侧（host-auth 为桌面独有接口，双端偏离——ADR 0022「双端偏离」节；移动端不跟演）

## 1. 用户裁定（最新指令，优先于一切既有文档）

1. **`session_configs` 表：删除**（宿主主库不再持有；既有 v21 起退役计划直接落地，不等 legacy_rows 观测归零）
2. **`pairings`（配对设备）与 `connection_history`（连接历史）：不应留在宿主侧** → 下沉「认证中心」（= `com.bedcode.terminal-session` 插件，认证语义权威实现方）；**其他插件经查询认证中心获取对应记录**

此裁定逆转 `.scratch/2026-09-20-host-business-decarriage/issues/07-auth-chain-forwarding.md`（2026-09-21 用户裁定「密钥托管 / 信任表 / 记录面 → 宿主」）与 AGENTS.md §8「`pairings` / `connection_history` 表留宿主」明文。按 §0 规则优先级 1（用户当前明确指令）执行；**凭据与验签红线（§8）仍生效**，见 §4 边界。

## 2. 现状取证（2026-09-22 审计）

### 2.1 宿主主库（`bedcode-desktop/src-tauri/src/db/schema.sql`）6 张表

| 表 | 性质 | 本次动作 |
| --- | --- | --- |
| `session_configs` | 业务数据（会话配置；v21 起只读 legacy 迁移通道） | **删除** |
| `pairings` | 认证业务数据（设备配对 / 信任记录） | **下沉认证中心** |
| `connection_history` | 认证业务数据（连接历史） | **下沉认证中心** |
| `settings` | 内核配置（`trafficEncryption`、`pairing_code_ttl`、`qr_token_ttl`） | 留宿主（配置域） |
| `plugin_storage` | 内核原语（插件 KV） | 留宿主 |
| `plugin_secrets` | 内核原语（v15 secret-store 密钥托管） | 留宿主 |

### 2.2 `pairings` / `connection_history` 宿主侧消费点（下沉必须覆盖的全部路径）

**宿主 Rust 直接消费：**
- `server/ws/conn.rs`（WS 认证中间件）：
  - 认证成功 → `db.update_pairing_last_seen(&fp, display_name)`（conn.rs:541）
  - 断开回填 → `db.close_open_connection_event_by_fingerprint(fp)`（conn.rs:705，DEVICE_DISCONNECTED 事件伴随）
- `host_impl/auth.rs` v18/v19 记录面（WIT host-auth 接口，插件经原语访问宿主主库）：
  - `trusted-devices-list` / `trusted-device-revoke`（v18）
  - `connection-history-list` / `connection-history-clear`（v18/v19）
  - `trusted-device-upsert` / `trusted-device-touch` / `connection-history-record`（v19）
  - `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind`（v19，读 `pairings.public_key`）
- `db/operations.rs`：`add_pairing` / `get_pairings` / `get_pairing_by_fingerprint` / `update_pairing_last_seen` / `update_pairing_token` / `remove_pairing` / `update_pairing_public_key` / `record_connection_event_by_fingerprint` / `close_open_connection_event_by_fingerprint` / `get_connection_history`
- `db/models.rs`：`Pairing` / `ConnectionHistory`

**插件 `com.bedcode.terminal-session` 消费（经 host-auth 原语）：**
- `trust/source.rs`：`trusted-devices-list` / `trusted-device-revoke`
- `auth_http/mod.rs`：`trusted-device-upsert` / `connection-history-record` / `trusted-device-touch` / `biometric-credential-bind` / `link-identity-parts`
- `auth_http/biometric.rs`：`biometric-credential-bound` / `trusted-devices-list` / `biometric-verify-signature`
- `auth_http/jwt.rs`：`device-token-issue` / `device-token-verify`
- `pairing/keys.rs`：`secret-get` / `secret-set`
- `device_face.rs` / `devices.rs`：`trusted-devices-list` / `connection-history-list` / `connection-history-clear` / `auth-setting-set`
- `policy/mod.rs`：读 pairings 原始记录做撤销检查（`isActive = false` 拒绝）

**宿主命令面（已注销，无残留）：** lib.rs 注释确认配对 / QR / 连接历史命令面已随票 05/06 注销，产品面全归插件 `session.pairing.*` / `session.devices.*` / `session.history.*`。

### 2.3 `session_configs` 现存影响面

- 宿主：`db/operations.rs` CRUD、`session/session_config.rs`（只读面 `SessionConfigManager`）、`host_impl/session.rs::session_config_get/list`（WIT host-session config-get/config-list 读取面）、`database.rs::migrate_session_configs_check_constraint` / `count_legacy_session_configs`（票 02 观测）、lib.rs 启动 `legacy_rows` 日志
- 插件：`config/ops.rs::migrate`（`LegacyConfigSource` 经 `session_config_list` / `session_config_get` 一次性迁入插件私有库，marker 幂等）
- 宿主侧无其他生产消费者（全仓 grep 确认：`set_db_setting` / `get_all_db_settings` 命令亦零消费者，顺带清理候选）

### 2.4 边界事实

- `pairings.session_token` 列**零生产消费**（`update_pairing_token` / `verify_session_token` 无调用方）——死列，下沉时直接丢弃（凭据不复制）
- `pairings.public_key`（生物凭证公钥）：**验签执行在宿主**（`auth_biometric_verify_signature`），§8 凭据红线「密钥与公钥不出宿主」——下沉时公钥不复制入插件库？**见 §4 边界决策**
- 移动端：无 host-auth 接口、无 pairings/connection_history 表、无消费 → 单端桌面改动
- `quick_actions_migration.rs` / `task_data_migration.rs` / `session_db_migration.rs`：与本次无关，保持不动

## 3. 目标设计

### 3.1 数据归属（终态）

| 数据 | 归属 | 说明 |
| --- | --- | --- |
| 会话配置 | 插件私有库（已实现，`session_configs` 主库表退役） | 迁移通道删除 |
| 配对设备 / 信任记录 | **认证中心插件私有库**（新增表） | 真源 = 插件 |
| 连接历史 | **认证中心插件私有库**（新增表） | 真源 = 插件 |
| 生物凭证公钥 | **宿主 `plugin_secrets`**（认证中心插件属主键下） | §8 凭据红线：公钥不出宿主，验签执行点在宿主 |
| JWT 密钥 / secret-store | 宿主 `plugin_secrets`（不变） | v15 原语保持 |
| 认证 TTL 配置 | 宿主 `settings`（不变） | 配置域 |
| 在线设备列表 | WS 注册表（宿主，不变） | 会话在线语义 |

### 3.2 WIT host-auth 接口调整（desktop v23 → v24，**ABI bump**）

| 原语 | 动作 | 理由 |
| --- | --- | --- |
| `trusted-devices-list` / `trusted-device-revoke` | **删除** | 数据在认证中心私有库，插件直接查自己的库 |
| `connection-history-list` / `connection-history-clear` | **删除** | 同上 |
| `trusted-device-upsert` / `trusted-device-touch` / `connection-history-record` | **删除** | 写面归插件自有库 |
| `biometric-credential-bound` | **改实现（不删签名）** | 判定「已配对且绑定公钥」改为查宿主 `plugin_secrets`（公钥托管位） |
| `biometric-verify-signature` / `biometric-credential-bind` | **保留**（实现指向 plugin_secrets） | 验签执行在宿主，公钥不出宿主 |
| `auth-setting-set` | 保留 | settings 表留宿主 |
| `secret-*` / `device-token-*` / `link-identity-parts` | 保留 | 宿主密码学 / 密钥托管面 |
| `config-get` / `config-list`（host-session） | **删除** | session_configs 退役，legacy 迁移通道关闭 |

> 删除接口即 ABI bump：v23 → v24。旧产物（按 v23 SDK 构建）在 activate 期拿到点明新形态的错误（复用 host-bus / host-session 退役先例的错误形态），须按 v24 SDK 重建。SDK 侧同步：`permission.rs` 真源增删位（`session:config` 已退役；本次无权限位增删——记录面是 `auth` 权限门，位本身不变）、`rust/types.rs`、生成物 `permission-vocabulary.json` / `permission.vocabulary.ts`、`manifest-gen.js` 映射表。

### 3.3 认证中心插件（terminal-session）侧

- **新增表**（私有库 schema，命名沿用 `plugin_id_` 无关——私有库无前缀约束）：
  - `pairings`（迁移宿主旧表公开列：`id / device_name / device_fingerprint / address / uid_hash / paired_at / last_seen / connect_count / is_active`；**不迁移** `public_key`、`session_token` 两凭据列）
  - `connection_history`（`id / device_id / auth_method / result / address / connected_at / disconnected_at`）
  - 迁移账本键（`plugin_meta`）：`auth_records.migrated_from=host-main-db`
- **迁移**：宿主启动时经互调 api（复用 `quick_actions_migration` 通道形态）推存量数据 → 插件 marker 幂等落库（`INSERT OR IGNORE`）；host-auth 记录面原语退役前留一次性窗口
- **查询面供其他插件使用**（manifest `api` 声明 + `#[plugin_api]` 宏，ADR 0017）：
  - 现有 `trust-list` / `devices-connect-list` 已覆盖部分；按需补 `devices-list` / `history-list`（对齐现有 JSON-RPC 2.0 形状）
- **内部改自读**：`trust/source.rs`、`auth_http/*`、`device_face.rs`、`devices.rs`、`policy/mod.rs` 的 pairings/history 访问从 host-auth 原语改为私有库读写
- **`config/ops.rs::migrate` + `LegacyConfigSource`**：删除（session_configs 迁移通道关闭）

### 3.4 宿主 WS 认证中间件（`server/ws/conn.rs`）改造

- 认证成功 last_seen 更新：改经互调 api 通知认证中心插件（`auth_center::call_api` 现成通道；插件未激活 → 静默跳过，记录缺失不阻断认证——沿用现有 D7 降级语义）
- 断开回填连接历史：同理经互调 api（DEVICE_DISCONNECTED 伴随）
- 验签执行（`JwtService` / `enforce_connection_policy`）**保持不动**（密码学引擎不移动，spec §3「不动」表）

### 3.5 宿主主库清理

- `schema.sql`：删 `session_configs` / `pairings` / `connection_history` 三表定义 + 相关索引
- `database.rs`：删 `migrate_session_configs_check_constraint`、`count_legacy_session_configs`、相关迁移与测试
- `db/operations.rs` / `db/models.rs`：删对应 CRUD 与模型
- `session/session_config.rs`：删 `SessionConfigManager`
- `host_impl/auth.rs` / `host_impl/session.rs`：删退役原语；biometric 原语改查 plugin_secrets
- `lib.rs`：删 legacy_rows 观测、删 `get_all_db_settings` / `set_db_setting` 命令注册（死命令）
- 迁移报告 struct（QuickActionsMigrationReport 同型的 AuthRecordsMigrationReport）

### 3.6 宿主命令面

- 配对 / QR / 连接历史 / 设备列表命令面：已注销（票 05/06），无宿主残留可删——确认 `commands.rs` 无 list 设备 / history 命令
- `get_all_db_settings` / `set_db_setting`：零消费者，**删除**（顺带清理）

## 4. 凭据与验签红线边界（§8 不可违反）

1. **公钥不复制入插件私有库**：`pairings.public_key` 迁移时写入宿主 `plugin_secrets`（属主键 `com.bedcode.terminal-session`，key 如 `biometric:<fingerprint>`）；`biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind` 实现改查/写 plugin_secrets —— **验签执行点在宿主不变，公钥不出宿主**
2. **session_token 丢弃**：死列，不迁移（凭据零复制原则）
3. **认证链路不走旁路**：JWT 签发/验签（device-token-*）、生物验签、secret-store 全部保持既有 auth 模块路径；本次只搬「记录」不搬「密码学」
4. **插件未激活降级语义保留**：WS 中间件 last_seen/历史记录更新失败 → warn + 跳过（与现状 update_pairing_last_seen 失败 warn 同语义）；认证放行不受记录写入影响

## 5. 影响文件清单（预计）

**宿主（bedcode-desktop/src-tauri/src）：**
- `db/schema.sql`、`db/database.rs`、`db/operations.rs`、`db/models.rs`
- `session/session_config.rs`（删）
- `server/ws/conn.rs`（last_seen / 断开回填改互调）
- `plugin/manager/wasm_runtime/host_impl/auth.rs`（删记录面原语，biometric 改 plugin_secrets）
- `plugin/manager/wasm_runtime/host_impl/session.rs`（删 config-get/list）
- `utils/auth/auth_center.rs`（新增记录互调通道 / 迁移推送）
- `lib.rs`（删命令注册 + legacy_rows 观测）
- `plugin/auth_records_migration.rs`（新增，quick_actions_migration 同型）
- 新增：迁移测试 / WS 中间件互调测试

**WIT / SDK（packages/plugin-sdk-desktop）：**
- `rust/wit/bedcode.wit`（host-auth v24：删 7 记录面原语 + host-session config 面；bump）
- `rust/src/permission.rs`、`rust/src/types.rs`、生成物、`bin/manifest-gen.js` 映射表
- `pnpm run gen:permissions` 重出词汇表

**插件（plugins/terminal-session）：**
- `rust/src/schema.rs`（新增 pairings / connection_history 表）
- `rust/src/lib.rs`（api 清单、迁移导入 api、查询面）
- `rust/src/trust/*`、`auth_http/*`、`device_face.rs`、`devices.rs`、`policy/mod.rs`、`config/ops.rs`
- 契约测试更新（native mock 换私有库）

**文档：**
- AGENTS.md §8（撤销「pairings/connection_history 表留宿主」口径）、§7 检查清单 host-auth 行
- code-map.md（db/ 模块描述）
- 根 CHANGELOG.md

## 6. 验证

- 宿主 `cargo test` 全绿（新迁移测试：旧库重跑幂等 / 凭据列不迁移 / 零丢失；WS 互调降级测试）
- 插件 `cargo test` 全绿（私有库读写契约、迁移 marker 幂等）
- 插件构建 + `plugins:build` 全链（v24 SDK 重建产物，wasmHash 校验）
- 宿主 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error
- 手动：真实升级路径（旧库 → 迁移 → 设备列表/历史页数据完整、撤销生效、生物登录可用）
- lens_diagnostics mode=all 无 blocker

## 7. 实施顺序（依赖关系）

1. **插件侧先行**：私有库表 + 内部改自读 + 迁移导入 api + 查询面（新 schema 就绪前宿主不动）
2. **WIT/SDK v24**：删原语 + bump + SDK 重建 + 权限词汇重出
3. **宿主迁移**：auth_records_migration + WS 中间件互调 + biometric 改 plugin_secrets
4. **宿主清理**：schema.sql / operations / models / session_config.rs / lib.rs 死代码
5. **插件重建 + 双端测试 + 文档**

> 依赖注意：host-auth 原语删除前，旧插件产物（v23 SDK 构建的 wasm）将无法通过记录面读写——**同批完成**插件重建与宿主退役，避免中间态；迁移窗口期的「插件旧产物 + 宿主新主库」组合由迁移 api 推送兜底（存量数据在宿主主库，插件经互调拉取，不依赖 host-auth 记录面）。
