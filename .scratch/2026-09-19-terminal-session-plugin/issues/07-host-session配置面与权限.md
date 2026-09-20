# 07: host-session 配置面 + `session:config` 权限 + 补成功闭环单测缺口

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D4 / D2 / D8-P2

**What to build:** 插件第一次拿到「读改会话配置」的能力：新增配置读取、写入（含 upsert 语义）、删除三个原语，权限位独立于会话读写。宿主会话页仍走原命令面，行为不变；这条票交付的是能力面 + 把今天完全缺失的「成功路径闭环测试」补上。

**Blocked by:** 04（有插件消费方才能验证原语可用）

**Status:** done（2026-09-20；门禁：桌面 `cargo test` lib **1024/0** + 8 集成目标 + doc 1、SDK crate **85/0**、插件 crate session 74 / file-transfer 58 / auto-task 24、前端 **74 files / 718**、根 eslint **0 error / 123 warning**（存量）；ABI desktop **19** / mobile 11（双端偏离）；实施记录见 Comments）

- [x] `host-session` 追加配置三函数（既有 interface 函数级追加，不新增 interface）——`config-upsert` / `config-get` / `config-delete`
- [x] 新权限位 `session:config` 落五同步点：SDK 权限常量与 API 映射、打包 CLI、前端合法集合、宿主能力清单、host_impl 权限门（漂移锁用例 `permission_sync_points_all_know_session_config`）
- [x] 权限门前置于一切参数处理：缺权限拒绝、越权拒绝、参数非法可见报错（禁止裸透传）
- [x] **补口**：会话原语现有单测全是权限/参数门，新增成功路径闭环（写 → 读回 → 改 → 删 → 确认消失），走临时库不污染真实数据（`session_config_crud_success_closed_loop`，内存库）
- [x] 宿主真实 wasm 闭环新增 `test_session_config_*` 矩阵（含 `session:config` 授予/未授予两态）——`wasm_runtime::test_session_config_api_closed_loop`
- [x] 桌面 `cargo test` 全绿；ABI 计数在契约文件里与实现同步（**实读 `abi.rs` 确认开工时 desktop = 18**（v18 host-auth 记录面，票 05 占用），本批次顺延取 **v19**——规格正文「并发线已占 v17/v18」的表述已过时，编号以实读为准）

## Comments（2026-09-20 实施记录）

### 1. 交付物（按仓内文件）

| 层 | 文件 | 变更 |
| --- | --- | --- |
| 契约 | `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | `host-session` 追加三函数 + 版本表 v19 条目 |
| 契约 | `packages/plugin-sdk-desktop/rust/src/abi.rs` | `ABI_VERSION 18 → 19` + 版本表条目 + `test_abi_version_is_v19` |
| SDK | `rust/src/permission.rs` | `PERMISSION_SESSION_CONFIG` 常量 + 合法集合 + API 映射（`session.configUpsert/Get/Delete` 审计名） |
| SDK | `rust/src/host/session.rs` / `rust/src/wasm_host.rs` | `HostSession` 三方法 + `WasmHost` 实现（JSON 值 ↔ wire 字符串） |
| 宿主 | `wasm_runtime/component.rs` | `host_session::Host` 三方法接线 |
| 宿主 | `wasm_runtime/host_impl/session.rs` | 三实现（权限门 → 参数仲裁 → `SessionConfigManager`）+ 4 条新单测 + 漂移锁 |
| 宿主 | `wasm_runtime.rs` | `test_session_config_api_closed_loop` 闭环 + 测试基建修正（见 §5 踩坑一） |
| 前端 | `src/plugin/permission.ts` | 合法集合 + API 映射（与 SDK 侧同名） |
| 打包 | `packages/plugin-sdk-desktop/bin/cli.js` | `VALID_PERMISSIONS` 增 `session:config` |
| 打包 | `packages/plugin-sdk-desktop/bin/manifest-gen.js` | Rust 推导规则：`session_config_(upsert\|get\|delete)` → `session:config` |
| fixture | `packages/plugin-sdk-test/src/lib.rs` | 探针命令 `test_session_config_face`（新建→读回→覆盖→删除→读回） |

### 2. 语义决策（实现期定，写进 WIT 文档）

- **`config-upsert` 的 id 语义**：`id` 缺省/空 → 新建（宿主生成 UUID）；`id` 命中 → 覆盖；**`id` 非空但未命中 → 显性报错**（`session error: config not found: <id>`）。取舍理由：真 upsert（未知 id 静默新建）会让「以为改的是既有配置」变成静默多出一份重复配置——拼错 id 是最常见误用；显性报错把这类 bug 立刻暴露，且调用方要新建时本就不该带 id。
- **覆盖不做「缺字段补空串」**：仅覆盖 JSON 中出现的字段（`update_config` 的 `None` 传参 = 保持原值）。若按「缺字段置空」实现，`{"id":"x","name":"y"}` 会把 environment / workingDir / command 清空 → 直接被 schema `NOT NULL`/`CHECK` 打回，成为难以理解的错误。
- **`config-delete` 返回 `bool`**：命中 `true` / 未知 id 幂等 `false`（与 `trusted-device-revoke` 同口径——「删不存在的记录」不是错误，但要能区分是否真删掉一条）。先读后删，删除复用 `delete_config`（连带 `ConfigRemoved` 同步事件）。
- **`autoStart` 缺省 `false`**：与宿主命令面 `create_session_config` 一致（自启由插件域驱动，宿主命令面同样硬编码 false）。
- **校验沿用内核现状**：`SessionConfigManager::validate_config`（name / environment 非空）。环境取值白名单、WSL 分支合法性、空命令兜底属**业务规则**，随票 08 下沉插件——本原语只做输入仲裁（spec D2/D3 的裁剪线）。
- **权限错误文案带权限名**：`permission denied: session:config`（沿用 pty 域 v16 起的可诊断口径；本模块 v6/v7 旧函数的裸 `permission denied` 未改动——避免无收益地翻既有断言）。

### 3. 五同步点落点（漂移锁逐点断言）

| # | 同步点 | 落点 | 断言方式 |
| --- | --- | --- | --- |
| ① | SDK 常量 + 合法集合 + API 映射 | `permission.rs` | 行为：`grant_permissions` 保留该位 + `check` 为真 |
| ② | 打包 CLI | `bin/cli.js` `VALID_PERMISSIONS` | 字面量 `'session:config'` |
| ③ | 前端合法集合 | `src/plugin/permission.ts` | 字面量 `'session:config'` |
| ④ | 宿主能力清单 | `capability.rs`（本批次**无新 interface**，权限挂在既有 `host-session`） | `CapabilityRegistry::is_available("host-session")` |
| ⑤ | host_impl 权限门 | `host_impl/session.rs` 三函数 | 三态用例（缺权限 / 越权 / 授权）+ 拒绝零副作用 |

### 4. 测试矩阵

| 用例 | 层 | 覆盖 |
| --- | --- | --- |
| `session_config_crud_success_closed_loop` | host_impl 单测（内存库） | **成功闭环**：新建（UUID）→ 读回逐字段相等 → 覆盖只改 name（workingDir/command 保持原值、createdAt 不变）→ 删除命中 → 读回 None → 再删幂等 false |
| `session_config_requires_config_permission` | host_impl 单测 | 越权：只有 `session:read` + `session:write` 三函数全拒；拒绝零副作用（`config-list` 反查为空） |
| `session_config_param_gates_are_explicit` | host_impl 单测 | 非法 JSON / 非对象 / 空 name / 空 environment / 未知 id / 空 config_id 全部显性报错；未知 id 删除幂等 false |
| `permission_sync_points_all_know_session_config` | host_impl 单测 | 五同步点漂移锁 |
| `test_abi_version_is_v19` | SDK crate | ABI 编号锁（v19） |
| `test_session_config_api_closed_loop` | 宿主真实 wasm 闭环 | 真实 sdk-test 产物经 WIT → SDK → host_impl：未授权三原语拒绝 + 内核配置表零写入；授权后新建/读回/覆盖/删除/读回消失，并**经内核配置管理器反查**确认落库 |

### 5. 踩坑与处置

1. **`setup_wasm_runtime` 的配置库没有 schema**（`SessionConfigManager::new(Database::new(":memory:"))` 未 `init_schema`）→ 配置 CRUD 闭环首次运行报 `no such table: session_configs`。处置：按 `host_impl::tests::build_host_ctx` 同构补 `init_schema()`（测试基建修正，非生产路径）。这也是「此前 host-session 侧没有成功闭环」的直接原因之一——配置表在 wasm 测试上下文里根本不存在。
2. **改 WIT 会让全部 fixture 的 mtime 检查失效** → 测试内 `cargo build --target wasm32-wasip3` 在 ambient stable（无该 target）下失败，一次性红 39 项（票 06 已登记同一现象）。处置：以 `RUSTUP_TOOLCHAIN=nightly-2026-09-16 cargo build --target wasm32-wasip3 --release --manifest-path packages/<fixture>/Cargo.toml` 预热 6 个 wasip3 fixture + 1 个 wasip2 fixture（`plugin-wasi-test`）后再跑宿主测试。
   - **建议**（非本票范围）：AGENTS §3 的黄金命令应补注「fixture 需重建时须 `RUSTUP_TOOLCHAIN=nightly-2026-09-16`」，或让 fixture helper 自动注入该 env——否则任何人改 SDK WIT 后跑 `cargo test` 都会看到这 39 项假红。

### 6. 遗留 / 交给后续票

- **票 08 的设计张力需显式裁决**：本票把配置 CRUD 加在 `host-session`（真源 = 内核 `session_configs` 表），而 spec D5 / 票 08 要求「配置真源迁插件私有库、主库旧表只读退役」。两者不可能同时成立：08 落地后 `host-session.config-*` 在插件侧将无消费者。08 开工前需在「保留 host-session 配置面（作为内核代理）」与「08 起由插件直连 host-plugin-database、本票三函数留空转待退役」之间二选一（本票已按票面语义实现能力面，未预判结论）。
- 宿主会话页与命令面**零改动**（票面要求「行为不变」）：`commands/session_config.rs` 四个命令未触碰。
- 文档计数：`AGENTS.md` §7 仍写「当前 desktop **v17**」（票 05 后已应为 v18、本票后 v19）；`bedcode-desktop/docs/code-map.md` 未列 `host-session` 配置面。按 spec P5，二者归**票 18**统一同步（本票不顺手改，避免与 18 冲突）。
- 测试残留：本票三条门禁命令（`cargo test` ×3 / `vitest run` / `eslint`）均为一次性进程，跑完已退出（`vitest run` 非 watch）；端口检查命令因工具安全规则被拦（未执行），如后续发现残留以 `ss -ltnp` 复核。
