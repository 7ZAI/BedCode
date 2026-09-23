# legacy 迁移链整体退役（2026-09-23 用户裁定）

## 裁定

用户：「不用考虑兼容旧版本用户」——宿主侧一次性 legacy 迁移链整体退役，不再为存量旧库/旧插件 id 路径做数据搬运。

## 删除清单

- **迁移模块（4 + 入口）**：`src/wasm_core/legacy.rs` + `auth_records_migration.rs` / `quick_actions_migration.rs` / `session_db_migration.rs` / `task_data_migration.rs`（git rm）
- **lib.rs**：setup 中 4 个 `run()` 触发点及注释（task_data / session_db / quick_actions / auth_records）
- **db/models.rs**：`LegacyQuickActionRow` / `LegacyPairingRow` / `LegacyConnectionRow` / `LegacyAuthRows` + 死代码常量 `connection_method` / `connection_result`（v24 下沉后宿主侧零消费，清理遗漏）
- **db/operations.rs**：`list_legacy_quick_action_rows` / `seed_legacy_quick_action_row` / `list_legacy_auth_rows` / `seed_legacy_auth_rows` / `drop_legacy_auth_tables`
- **db.rs / wasm_core.rs**：re-export 与模块声明同步
- **session_e2e.rs**（M 状态外部在途文件，仅精确删 legacy 三块）：quick_actions 播种 / handoff 迁移断言 / 幂等重推断言 + quick-actions DTO 双轨对照段（依赖播种数据）

## 保留（明确不删）

- `Setting`（通用 KV，配置/安全域引擎原语）
- 各文件里「legacy」字样但属独立概念：`legacy_http_alias`（HTTP 别名）、`auto_approve_legacy_user_plugin`（老插件审批）、`find_legacy_paths`（结构锁）、老格式解析测试等
- 插件侧 `auth-records-import` / `quick-actions-import` api 与 marker 幂等逻辑（宿主不推后无害保留，插件工程未动）
- `SessionConfigManager` 壳（v24 遗留清理项，另一任务范围）

## 语义变化

旧库升级后：`pairings` / `connection_history` / `session_configs` / `quick_actions` 滞留表**不读不迁不清理**（schema.sql 不再建、宿主不再读）；插件私有库旧 id 路径数据不再搬。

## 验证

- `cargo check --lib` 0 error（42 既有 warning 未增）
- `cargo test --lib` 1136 passed / 1 failed（`host_api::pty::tests::spawn_applies_declared_env_without_business_identity` —— 外部在途 pty 任务文件，非本次改动）
- session_e2e 10/10；残留引用 rg 归零；测试后无残留进程
- lens_diagnostics delta 无 blocker

## 文档同步

- AGENTS.md §7 v24 段 / §8 认证记录下沉段
- code-map.md 三处（db/ 树注释、handoff 模块段、host-auth 段）
- CHANGELOG Unreleased → Improvements → Desktop 新条目（历史条目不改）
- schema.sql / database.rs 注释

## 注意

工作区有外部在途（peer_net 重构、P1 读取面、permission 改名、pty 任务），提交前精确 `git add` 本清单文件，禁止整目录 add。
