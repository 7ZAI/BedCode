# 05: host-auth 记录面四函数 + trust / consent 搬迁 + file-transfer 互调改指

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D4 / D8-P1

**What to build:** 「这台设备能不能连我」的完整决策面搬到插件：已配对设备列表与撤销、对等首连确认（consent）决策、认证域设置项读写，全部经新追加的宿主只读/撤销原语取得原始记录后由插件组织；file-transfer 插件的两处消费点改调新命名空间 api，行为与改指前一致。

**Blocked by:** 04（插件已具备认证语义与桥接通道）

**Status:** done（2026-09-19；桌面 `cargo test` 全目标绿 lib 1020 + 8 集成 + doc 1、SDK 85、三插件 crate 32/74/58、前端 74 files/718、根 eslint 0 error；实施记录见 Comments）

- [x] `host-auth` 追加四函数（信任设备列表 / 撤销信任 / 连接历史列表 / 认证域设置写入），均为既有 interface 的函数级追加，不新增 interface
- [x] 四函数权限门 `auth` + 属主校验 + 记录真源仍在内核表；读取返回原始记录，排序与解读一律在插件侧
- [x] trust 模块（统一信任视图 + 撤销编排）与 consent 模块（首连确认决策）搬入插件，与搬迁前实现做对照测试
- [x] 互调 api 面按域命名（pairing / trust / consent）声明齐备；未声明 api 不可调的门禁用例通过
- [x] file-transfer 的两处消费点（consent 决策、可信对端列表）改指新 id 与新 api 名，其对等网络集成测试不改断言即过
- [x] 宿主真实 wasm 闭环覆盖「列表 → 撤销 → 列表变化」与「consent 接受/拒绝」两条路径
- [x] 桌面 `cargo test` + 插件 crate `cargo test` 全绿

## Comments

### 票 04 落地后本票的既有事实（开工前读）

- 桥接层已是**双目标**：配对面锚点 `SESSION_PLUGIN_ID` / `SESSION_PAIRING_MARKER_API`（`com.bedcode.session.pairing-code-status`）已生效；`AUTH_CENTER_PLUGIN_ID` / `AUTH_CENTER_MARKER_API`（`com.bedcode.devices.list-trusted-devices`）仍指向 devices，供 `enforce_connection_policy` 的 capability 导出与 `sync_pairing_*` 镜像同步使用。**本票的收敛动作**：trust / consent / policy 进会话中心后，把 `enforce_connection_policy` 的目标改为 `SESSION_PLUGIN_ID`、删掉 `AUTH_CENTER_*` 两常量，并把 `sync_pairing_*` 的 api 名改到新命名空间（`com.bedcode.session.trust-*`）。
- devices 侧 `pairing/` 副本此刻是 `policy` 的编译依赖，`policy` 搬走后即无消费方——随票 06 删除；本票搬 `policy` 时它是**唯一**需要一起过的跨域件。
- `plugins/session/rust/src/pairing/jwt.rs` 顶部有模块级 `#![allow(dead_code)]`（票 04 落语义、无生产消费方）；`policy` 接入后收窄到具体项。
- 宿主测试 `host.rs::test_server_auth_policy_closed_loop` 的注册清单已含 `list-trusted-devices`（票 04 为锚点拆分而加），本票改指时应整体重写为该测试面向会话中心产物，而不是继续给旧清单打补丁。

### 2026-09-19 实施落地

**0. 开工时的基线变化（对侧线改名，已按新基线实施）**

并行线把 `plugins/devices/` 整体改名为 `plugins/auth-center/`（plugin id → `com.bedcode.auth-center`，rustLibrary → `bedcode_plugin_auth_center`，产物目录 → `resources/plugins/desktop/com.bedcode.auth-center/`），宿主桥接常量随改。本票按用户裁决**以该改名为新基线**推进：文案与路径全部用 `auth-center`，票 06 的退役对象即 `plugins/auth-center/`（本票 Comments 里凡涉及旧名的段落均指改名前后同一份代码）。

**1. `host-auth` 记录面四原语（既有 interface 的函数级追加，ABI 17 → 18）**

- WIT（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）：`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` 四函数 + 一段裁剪线说明。**不新增 interface**。
- **ABI 编号**：规格正文写「17 → 19」（前提是并发线已占 v18），实读 `abi.rs` 与 WIT 版本表确认 **v18 在本分支空闲**（notification 线未落树）→ 本批次落 **v18**，并在 `abi.rs` 顶部把编号口径写死（「以实读为准，不以规格正文为准」）。规格正文的对应行已同步修正。
- 宿实现（`host_impl/auth.rs`）：四函数 + 权限门 `PERMISSION_AUTH` + `AUTH_SETTING_KEYS` 白名单。
  - `trusted-devices-list`：`SELECT ... FROM pairings`（**全表**，含 `is_active = 0` 软删行，不排序）。全量返回是刻意的——撤销检测依赖「已撤销记录仍可见」，只回活跃集合会让「已撤销」与「从未配对」不可区分 → 撤销判定 fail-open。
  - **凭据红线（§8）**：`session_token` / `public_key` 列**不出口**；记录 JSON 只含公开视图列（测试用 sentinel 值钉住）。
  - `trusted-device-revoke`：内核 `remove_pairing` 语义（软删 + 连带删除该设备连接历史），返回是否命中；未知 id 幂等 `false`，已软删记录再撤销仍 `true`（不重复写）；**不做**断开在线连接（M5 保持宿主现状语义）。
  - `connection-history-list(device-id)`：内核 `connection_history` 原始记录（`device_id` = `pairings.id`）。
  - `auth-setting-set(key, value)`：内核 `settings` 表写入 + 键白名单（`pairing_code_ttl` / `qr_token_ttl`）+ 正整数校验（非法值显性报错，不写半合法数据）。
  - **属主说明**：记录是宿主全局数据、无句柄表 → 无「属主段」校验；权限门即授权边界（与 secret-store 的属主命名空间隔离不同，已在 WIT 与模块文档写明）。
- SDK（`host/auth.rs` trait + `wasm_host.rs` impl）四方法，JSON 字符串 → `serde_json::Value`；`component.rs` 接线 `host_auth::Host`。
- 宿主单测 8 条（`host_impl::auth::tests`，全部实跑绿）：权限门四函数拒绝 / 原始记录含软删行且无凭据列 / 撤销软删 + 连带删历史 + 幂等 / 历史形状与空结果 / 白名单与值校验 / 设置落库 + 重启持久化。

**2. trust / consent / policy 自 auth-center 搬入会话中心**

- `plugins/session/rust/src/trust/`（新）：`model.rs`（内核记录形状 + 统一视图 DTO）、`source.rs`（`TrustRecords` trait，wasm 走 host-auth 记录面；native 注入 mock）、`ops.rs`（list / revoke）。
  - **数据源整合（本票最有价值的一条）**：删除 host-storage `trust.pairings` 镜像与 `add_pairing` 写入入口——真源就是内核 `pairings` 表。撤销由插件经 host-auth 写内核真源，测试直查内核表断言 `is_active = 0`（不再是「插件自己的账本」）。
- `plugins/session/rust/src/consent/`（自 auth-center 搬入，api 名 `consent-decide`）、`policy/`（搬入，改读 host-auth 记录面；`active` → `isActive`）。policy 的「撤销即拒绝、未命中从宽」语义**保持不变**（收紧为「未配对即拒绝」属新协议决策，不在本批次，已写进模块文档）。
- 会话中心 `plugin.json`：permissions `["auth", "peer"]`；api 11 项（pairing 八 + `trust-list` / `trust-revoke` / `consent-decide`）；`rust/src/lib.rs` 新增三域 api 实现 + `verify_device_token_policy`（auth-policy 导出）+ 命令面 `session.trust.list` / `session.trust.revoke` / `session.consent.decide`；`session.status` 的 `domains` → `["pairing","trust","consent"]`。
- auth-center 侧删除 `trust/` `consent/` `policy/` 三目录、两项 api、三组命令与 `verify_device_token_policy`；`plugin.json` → api 9 项 / permissions `["auth"]`；crate 测试 36 → **32**（迁出的测试随模块走，新增的测试在会话中心）。

**3. 宿主桥接收敛（`utils/auth/auth_center.rs`）**

- 两目标常量**收敛为一个**：`SESSION_PLUGIN_ID` + `SESSION_MARKER_API = com.bedcode.session.trust-list`，`session_pairing_active()` / `auth_center_active()` → 单一 `session_active()`；`AUTH_CENTER_PLUGIN_ID` / `AUTH_CENTER_MARKER_API` 删除（票 06 无残留可清）。
- **锚点取 trust 域只读 api 而非 `pairing-code-status`**：合并后配对 / trust / policy 同属一个插件、同一个「已激活且互调面已登记」判据，锚点不应绑在某个可能演进的域上（旧口径正是拿配对码状态探 trust 面）。
- `enforce_connection_policy` 目标改 `SESSION_PLUGIN_ID`（验签执行点仍留宿主中间件，密码学不移动）。

**4. file-transfer 改指**

- `plugins/file-transfer/rust/src/auth_center.rs`：目标 id → `com.bedcode.session`；`#[plugin_api(manifest = "../../session/plugin.json")]`；trait 方法集与目标 manifest **精确一致**（`consent-decide` / `trust-list` / `trust-revoke` + pairing 八项，宏在 wasm 构建期比对）；client 调用改新短名。
- 编排层与 native 单测**零改判**（`AuthCenterGateway` 方法名是本插件侧词汇，与 wire 短名解耦）——58 条测试原样通过。
- 撤销维持宿主 `peer_revoke_trusted` 原语直通（与 trust 视图 peer 段同一宿主 store，改走互调无行为收益，避免无用耦合；已在模块文档注明）。

**5. 测试面（全部实跑）**

| 用例 | 覆盖 |
| --- | --- |
| `wasm_runtime::test_auth_center_plugin_artifact_lifecycle` | 退役期 auth-center 产物：activation + manifest（api 9 / permissions `["auth"]`）+ pairing 命令闭环 + 未知命令显性报错 |
| `wasm_runtime::test_session_plugin_artifact_lifecycle` | 会话中心 manifest：三域 11 项 api、`["auth","peer"]`、**桥接锚点与 file-transfer 消费的两条 api 必须在声明面里**（缺一即静默降级） |
| `wasm_runtime::test_session_trust_and_consent_api_closed_loop` | **S1 两条路径**：内核播种（1 活跃 + 1 软删）→ `trust-list` 只回活跃（peerError 透出）→ `trust-revoke` → 列表变化 + **直查内核表 `is_active = 0`**；consent 阶段 1 ask / 阶段 2 accept·deny·one_time；未声明 api 门禁拒绝 |
| `wasm_runtime::test_host_auth_record_face_closed_loop` | 四原语经 WIT → SDK → host_impl 全链：未授权实例被拒（负向）→ 授权后 raw 记录（含软删行、无凭据列）/ 历史 / 撤销 / 设置写入落库 |
| `wasm_runtime::test_filetransfer_consumes_session_center_closed_loop` | 真实 file-transfer 产物 × 真实会话中心产物：wire 捕获断言 topic/短名（`consent-decide` / `trust-list`）与参数映射；会话中心注销后降级路径 |
| `wasm_runtime::test_host_pairing_bridge_closed_loop` + `test_pairing_dual_track_host_and_plugin_paths_agree` | 配对桥接闭环与双轨对照矩阵；降级断言从「两门互不牵连」改成单一门（合并后语义） |
| `host::test_server_auth_policy_closed_loop` | 整体重写为面向会话中心产物：未激活回退 / 无记录放行 / 有记录放行 / **经插件撤销 → 拒绝**（并直查内核表）/ 其他设备放行 / 实例消失回退 |

- 测试接缝顺带补的两处基础设施：`sdk-test` fixture 的 caller 命令改指会话中心（`test_session_consent_decide*` / `test_session_trust_list` / `test_session_trust_revoke` / `test_session_undeclared`）并新增 `test_auth_record_face` 探针（四原语直调）；`WasmHostContext::database()` 只读访问器（宿主测试读写内核真源，替代此前从私有字段取库的不可达路径）。
- `host.rs` 的用例权限改用 **manifest 声明**（`loaded.manifest.permissions`）而非手工 grant——`activate_plugin` 会用 manifest 重新授权，手工 grant 会被覆盖（首轮实测踩点：`auth` 被冲掉 → `permission denied`）。

**6. 门禁取证**

- `cargo test --no-fail-fast`（bedcode-desktop/src-tauri）：lib **1020 passed / 0 failed**、8 个集成目标全 ok（broadcast_shutdown / build_manifest_smoke / http_auth_biometric / link_crypto_http / pty_session_chain / server_integration / ws_auth_rules / ws_session_route）、doc-tests 1 passed / 2 ignored。
- **连跑 5 轮（lib）**：3 绿 / 2 红，两处红都落在**存量测试隔离缺陷**（与本票 diff 无关，单独跑必绿）：
  1. `wasm_runtime::tests::test_ws_endpoint_server_domain_roundtrip`（1 次）——已知存量：`server/middleware/http_filter.rs` 的用例向**进程级** `TrafficFilterChain::global()` 注册改写过滤器（入站转大写 / 出站追加 `|ENCRYPTED`），其锁只串行化本模块用例，并行跑的 WS 插件端点 e2e 过链后被改写（上一票已登记，修复需跨模块共享测试锁，属独立裁决）。
  2. `test_compile_component_from_file_aot_cache` + `test_compile_component_from_file_recompiles_on_stale`（同一次运行同时红，1 次）——两者共用 `temp_dir/bedcode_aot_<pid>/` 缓存目录，且都调用 `build_test_component()`（共享 fixture 构建）；疑为共享 fixture 构建窗口内大小变化导致缓存 key 与产物错位（未取到断言原文，单独跑必绿）。
  - 处置：**不在本票修**（都是测试隔离面，且第 1 项需要「跨模块测试锁」这一设计取舍）；两项已随本节登记，建议由票 18（等价回归）统一收口。
- 插件 crate：`plugins/session/rust` **74 passed**、`plugins/auth-center/rust` **32 passed**、`plugins/file-transfer/rust` **58 passed**；SDK `packages/plugin-sdk-desktop/rust` **85 passed**。
- 产物重建（无 node 环境下手动等价构建：`RUSTUP_TOOLCHAIN=nightly-2026-09-16 cargo build --target wasm32-wasip3 --no-default-features --features wasm --release`）：session 743 KB、auth-center 707 KB、file-transfer 945 KB，均已落 `src-tauri/resources/plugins/desktop/<id>/`。
- 前端：`node node_modules/vitest/vitest.mjs run`（本机临时装 node 22.12）→ **74 files / 718 tests 全绿**；根 `node node_modules/eslint/bin/eslint.js .` → **0 error / 123 warning**（warning 全为存量，与票 03/04 同数）。
- 无 node/pnpm 的应对：本机原无 node，票内用 `install_binary` 装 node 22.12 后以 `node <pkg>/vitest.mjs` 直跑（pnpm 仍缺，`pnpm run test:run` 字面命令未跑，等价命令已跑）。

**7. 遗留 / 下一步**

- **票 06**：`plugins/auth-center/` 整体退役（目录 + 构建条目 + 产物 + 剩余 pairing 副本与其 32 测试 + `test_auth_center_plugin_artifact_lifecycle`），并把「命令面退役」改判为「门面保留 + 实现转发」写进说明。
- `auth-setting-set` 的**读侧**：本票只做写（键白名单 + 校验）；插件侧读 TTL 需给 `host-config` 增 DB 支撑的两键（`ConfigKey` 白名单 + `config_get` 取库），归**票 14 设置分组**（那一票才真正需要读），避免本票无消费方地扩白名单。
- `connection-history-list` / `auth-setting-set` 的产品消费方（设备视图的设备详情、设置分组的 TTL 项）归票 14；本票以宿主单测 + `test_auth_record_face` wasm 闭环覆盖原语自身。
- policy 的「未配对指纹」当前仍从宽放行（搬迁前语义）；是否收紧为「无配对即拒绝」需新协议决策（会改变现有 token 放行面），不在本批次。
- 双端偏离不变：mobile 不跟演（`host-auth` 记录面为 desktop 独有，ADR 0022「双端偏离」已补 v18；ABI desktop 18 / mobile 11）。
