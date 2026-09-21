# 02: 主库 SQL 隔离闭环（P0-1，全插件密钥泄露）

**What to build:** 一条真实的纵深：插件对**主库**的 SQL 访问只能命中自己的 `plugin_<id>_` 前缀表，任何其他表（`plugin_secrets` / `pairings` / `connection_history` / `settings` / 他插件表）在 SQLite 引擎层就不可见；主库访问不再是人人自动持有的 `storage` 权限。先红测、后修、再锁。

**Blocked by:** 01（权限位拆分依赖词汇单源与锁）

**Status:** ready-for-agent

## 现状与攻击链（已复核）

1. `packages/plugin-sdk-desktop/rust/src/permission.rs:233-236` 对每个插件无条件 `granted.insert(PERMISSION_STORAGE)` → `host_impl/database.rs:86,105` 的权限门恒过；
2. 主库唯一隔离是 `validate_sql_table_prefix`（`database.rs:564-580`）+ `extract_table_names`（`:585-613`），八个正则模式**都不识别逗号多表**；
3. `SELECT b.value FROM plugin_com_bedcode_demo_x a, plugin_secrets b` 通过校验并执行 → `db/schema.sql:76` 的 `plugin_secrets`（明文，宿主托管密钥同表不同域：`utils/auth/host_secrets.rs:4`）与全部配对记录泄露；
4. 连接上无 `sqlite3_set_authorizer`（全仓无）；现有 `database.rs:687` 的「多表」用例只覆盖 `JOIN` 形态。

## 验收

- [ ] **先落红测**（当前实现必须失败）：逗号多表读 `plugin_secrets`、逗号多表写他插件表、`ATTACH`/`PRAGMA` 形态、以及正则可预见的其它逃逸写法——每种一条，断言被拒
- [ ] 修复选型落地并在 Comments 记录取舍：(A) `rusqlite` 的 `set_authorizer` 回调按表名白名单仲裁（引擎层，推荐）；(B) 真 SQL 解析器提取全部表引用；(C) 主库访问改走受限视图/影子库。**禁止**只在正则表上打补丁
- [ ] `storage` 不再自动授予；主库访问用独立权限位（`database:main`，与 `host-plugin-database` 的私有库权限分离），并同步进 01 的词汇真源与漂移锁
- [ ] 现有 `host-database`（主库）消费者清单：全仓 grep 确认改动不破坏既有插件（若第三方无处可用，明确「主库面改判为仅内置插件可申请」并写进 AGENTS §7 能力清单）
- [ ] 凭据红线复核：`plugin_secrets` 的 `value` 在错误/日志路径只记长度（AGENTS §8）
- [ ] 回归：`cd bedcode-desktop/src-tauri && cargo test` 全绿；`host_impl/tests/*` + `wasm_runtime/tests/*` 相关闭环用例全绿；跑完清理测试残留进程

## Comments

- 2026-09-21 立项：来源 spec §4-P0-1。这是本轮唯一「无需任何权限即可跨插件取密」的缺陷，优先级最高。
