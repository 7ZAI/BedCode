# 01: 授权记录与策略真源（两表 + 幂等迁移 + 读模型 + 设置页总览）

**What to build:** 设置页里出现「应用授权」二级入口，列出所有已安装的 wasm-app 及其授权概览（策略徽标 + 文件/网络记录数）。授权策略与授权记录第一次有持久化真源。本阶段所有应用显示「默认 / 0 条」。

**Blocked by:** None (can start immediately)

**Status:** done（2026-09-27）

- [x] 两张表进 `db/schema.sql`（DDL 见 spec §5.1，含 `idx_auth_records_lookup` 索引）
- [x] `run_migrations()` 幂等：同一库连跑两次结果一致；补迁移幂等测试（**建表与建索引两条语句都要被覆盖**）
- [x] 读模型命令按 spec §9.3 返回策略（缺省 `default`）+ 全部记录 + 第一方内置免询问项；不带 plugin_id 时返回所有已安装 wasm-app
- [x] 设置页新增二级入口「应用授权」，单页列全部已安装 wasm-app，**按风险排序**（`always_allow` 置顶）
- [x] 本阶段 UI 只呈现「默认 / 0 条」，**不出**策略控件（留给 03）
- [x] i18n 键落 `settings` 分组，zh-CN / en 同步
- [x] 改 UI 前先加载 `frontend-styles` skill（AGENTS §4 强制）
- [x] 单元测试：空库上读模型返回空集而非报错；策略行缺失时回落 `default`

## 关键实现事实（grilling 已核实）

- 本票**不新增权限位**，不动 SDK 权限词汇漂移锁（该锁要求「每条词汇都有门禁落点」）
- 移动端不跟演（ADR 0018 双端偏离）；本票不 bump ABI，故 §5.1.4 的移动端影响评估不触发
- 红线：真源与 UI 留宿主（spec §11），默认值 = fail-safe，**manifest 不得声明策略档位**

## 实现记录（2026-09-27）

**落点**

| 内容 | 位置 |
|---|---|
| 两表 + 索引 DDL | `bedcode-desktop/src-tauri/src/db/schema.sql`（与 spec §5.1 逐字一致） |
| 真源读面 + 读模型装配 | `src-tauri/src/wasm_core/security/auth_policy.rs`（新） |
| 第一方清单只读投影 | `src-tauri/src/wasm_core/security/fs_auth.rs::first_party_trusted_dirs()`（清单与判定语义仍在原处，投影只翻译形状） |
| 读模型命令 | `src-tauri/src/wasm_core/manager/host/api_bridge.rs::plugin_auth_overview`（宿主面凭证绑定；`pluginId` 空 = 全部；只列 `pluginType=rust-ts`） |
| 命令注册 | `src-tauri/src/lib.rs` invoke_handler 插件块 |
| 展示助手（风险排序 + 缺项兜底） | `src/utils/authPolicy.ts`（新，纯函数） |
| 授权管理页 | `src/views/AuthorizationView.vue`（新，路由 `/settings/authorization` → `settings-authorization`） |
| 设置页二级入口 | `src/components/settings/SettingsAuthorizationSection.vue` + `useSettingsSections.ts`（`BUILTIN_SECTION_ORDERS.authorization = 700`，落在「日志」与「关于」之间） |
| 前端命令封装 | `src/plugin/commands.ts::pluginAuthOverview` |

**决策与偏离**

1. **迁移形态**：`plugin_auth_policies` / `plugin_auth_records` 无列级迁移（`schema.sql` 的 `IF NOT EXISTS` 即迁移，AGENTS §9 单一事实源），`run_migrations()` 保持空实现并注明；幂等由两条锁覆盖——`db::database::tests::auth_tables_migration_is_idempotent`（建表 + 建索引两次跑一致，且不清既有数据）与 `auth_tables_are_created_on_legacy_db`（旧库只跑一次生产初始化即建表成功，旧 `fs_granted_paths` 原样保留）。
2. **默认档兜底两处同向**：宿主 `AuthStrategy::parse` 未知值回落 `default`；前端 `normalizeStrategy` 同样回落 `default`——不认识的档位绝不等于更宽松（免询问 / 跳过记录）。
3. **风险排序放前端**（`utils/authPolicy.ts`）：spec §9.1 只规定「始终允许置顶」，排序属展示决策；权重 = 两类资源取风险最高者（任一为 `always_allow` 即置顶），同档按名称、再按 pluginId，顺序确定。宿主读模型按 `(resource, created_at, id)` 稳定输出记录。
4. **读模型不含图标字段**：总览行用 `PluginIcon` 的字母头像回退（`:name` + `:pluginId`）。若后续要在总览行显示真实图标，07 票在详情页已有完整 `PluginInfo`，届时再评估是否加 `icon` / `extensionPath`——本票不为展示多开字段。
5. **不预写写入面**：策略落库与记录落账方法留到 02–06 接入时再加（本票零写入路径，写方法此刻没有消费者）。
6. **`ops` 脏 JSON 显性报错**：不降级成空数组（空 ops 在判定里等于「该子树不含任何操作」= 一条有效拒绝，静默降级会把脏数据伪装成用户决定）。

**验证**

- Rust 定向：`cargo test --lib auth_policy`（8 passed，含 7 条新用例）、`--lib auth_tables`（2 passed）、`--lib first_party_projection`（1 passed）、`--lib retired_`（8 passed，结构锁未受影响）
- 前端定向：`pnpm exec vitest run` 四个文件（`utils/authPolicy` 8 + `AuthorizationView` 5 + `SettingsView` 3 + `useSettingsSections` 13 = 29 passed）
- 前端全量：107 files / 1298 tests 全绿（`AuthorizationView` / `SettingsView` / `useSettingsSections` 的既有断言已随新增内置分组同步更新）
- eslint：本次改动文件 0 error
- 桌面 Rust 全量：958 passed / **1 failed**，失败项 `session_e2e::test_session_input_via_gateway_closed_loop` 与本票无关——宿主测试仍期待旧文案「会话不存在」，而插件产物已改出 ADR 0030 信封码（插件侧 2026-09-27 commit `bd3fed0e4` 改、宿主测试文件停在 2026-09-25），属既有红，已上报
