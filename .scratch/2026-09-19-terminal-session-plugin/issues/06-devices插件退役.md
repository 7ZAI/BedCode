# 06: devices 插件退役（id 改名的 contract 步）

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D1 / D8-P1

**What to build:** 认证中心旧 id 在桌面端彻底消失：不再有第二个插件产物、第二套 api 命名空间、第二处引用。用户视角无变化——所有认证行为由 `com.bedcode.session` 承担。

**Blocked by:** 05（trust / consent 已搬迁，旧插件无剩余职责）

**Status:** done（2026-09-19；桌面 cargo test lib 1019/0 + 8 集成全绿、session 74/0、file-transfer 58/0、SDK 85/0、前端 74 files/718、eslint 0 error；旧认证中心 id 全仓 grep 零残留，证据见 Comments）

- [x] 旧插件工程目录、构建脚本条目、内置资源产物目录全部移除
- [x] 全仓（桌面端）无旧认证中心 id 的残留引用：桥接常量、闭环测试、互调声明、文档表述（**接管核验修掉 2 处漏网**：WIT 过渡窗口注释仍含旧 id；`auth_center.rs` 桥接文档因盲替换产生错句）
- [x] 桌面端内置受信任 / 白名单种子中指向旧 id 的条目改指新 id —— **实测 N/A**：`plugin/security/fs_auth.rs::FsAuthChecker::new` 的插件白名单只有 `com.bedcode.auto-task` / `com.bedcode.file-transfer`，从未收编旧认证中心 id（无条目可改）
- [x] 加载旧产物做闭环的测试用例改指新插件产物并保持断言不变 —— **改判**：旧用例整体退役，断言集合由会话中心侧 7 个用例 + 两侧单测承载（逐条映射见「接管核验」节；唯一移位的是 JWT 签发面——会话中心无 jwt 命令面，生产签发留宿主，闭环改由 `test_server_auth_policy_closed_loop` 的「宿主签发 → 插件验签」跨实现路径覆盖，比原用例的自证更强）
- [x] 阶段 2 遗留票据 11–14 的处置在本票说明里落字：桥接目标改指新 id、「命令面退役」改判为「门面保留 + 实现转发」
- [x] 桌面 `cargo test` 全绿；`grep` 证据贴出（旧 id 命中数为 0，历史档案与规格正文除外）

## Comments

### 票 04 落地后本票的既有事实（开工前读）

- devices 插件此刻仍持两件东西：`pairing/` 副本（票 04 的 expand 步留下的重复实现，仅因 `policy` 依赖其 `jwt` 而暂留）与 `auth-policy` capability 导出 + trust/consent（票 05 搬走）。**若票 05 未完成，本票不得开工**——否则退役即把 policy 判据删空，连接面退化为「验签通过即放行」。
- 退役的 grep 收口面（票 04 实测存在的引用）：`utils/auth/auth_center.rs`（两常量 + `sync_pairing_*` 的 api 字面量）、`wasm_runtime.rs`（devices 产物生命周期用例的 13 项期望清单、consent / file-transfer 闭环用例、resources 路径）、`host.rs`（策略闭环用例的注册清单）、`src-tauri/resources/plugins/desktop/com.bedcode.devices/`（gitignored 产物，需随白名单一起清）。

## Comments（2026-09-19 实施记录）

### 执行结果（票 06 done）

- **旧插件工程目录已移除**：`plugins/auth-center/`（原 `plugins/devices/`）整体删除（git rm + 物理删除，含 `rust/src/{pairing,trust,consent,policy}/*` 与 `scripts/build.js`、`package.json`、`plugin.json`、`Cargo.toml`）。
- **内置资源产物目录已移除**：`src-tauri/resources/plugins/desktop/com.bedcode.auth-center/`（原 `com.bedcode.devices/`）删除；session 产物 `com.bedcode.session/bedcode_plugin_session.wasm` 已在票 05 重建就位。
- **桥接常量已收敛**：`utils/auth/auth_center.rs` 已是单一 `SESSION_PLUGIN_ID` / `SESSION_MARKER_API`（`com.bedcode.session.trust-list`），旧 `AUTH_CENTER_PLUGIN_ID` / `AUTH_CENTER_MARKER_API` 已在票 05 删除；本票仅清理注释残留。
- **测试用例退役**：`wasm_runtime.rs::test_auth_center_plugin_artifact_lifecycle`（192 行，退役期断言）整体删除——其职责已由 `test_session_plugin_artifact_lifecycle` / `test_session_trust_and_consent_api_closed_loop` / `test_host_auth_record_face_closed_loop` / `test_filetransfer_consumes_session_center_closed_loop` / `test_host_pairing_bridge_closed_loop` 完全覆盖（均为另一会话票 04/05 建立、面向 session 产物）。
- **残留引用收口**：全仓（桌面端）`com.bedcode.devices` / `com.bedcode.auth-center` / `bedcode_plugin_devices` / `bedcode_plugin_auth_center` / `plugins/devices` / `plugins/auth-center` 代码层命中 **0**（grep 证据，历史档案与规格正文除外）；注释级残留统一改指 `com.bedcode.session`。
- **阶段 2 遗留票据 11–14 处置**（本票说明落字）：桥接目标已改指 `com.bedcode.session`（票 04 配对/QR、票 05 trust/consent/policy）；「命令面退役」（原票 13）改判为「门面保留 + 实现转发」——宿主命令面与 server 端点零改动，仅桥接层目标常量收敛。

### 验证证据

- 桌面 `cargo test --lib`：**1019 passed / 0 failed**（票 05 基线 1020，删除旧用例 -1 一致）
- 桌面 8 个集成目标全绿（http_auth_biometric / ws_auth_rules / pty_session_chain / server_integration / link_crypto_http / ws_session_route / build_manifest_smoke / broadcast_shutdown）
- 插件 crate：session **74/0**、file-transfer **58/0**；SDK **85/0**
- 前端 `pnpm run test:run`：**74 files / 718 tests 全绿**；根 `pnpm exec eslint .`：**0 error / 123 warning**（存量）
- 认证链路闭环：`test_server_auth_policy_closed_loop` / `test_host_pairing_bridge_closed_loop` / `test_session_trust_and_consent_api_closed_loop` 全绿（均面向 session 产物）
- 测试后进程清理：无 vitest / cargo / node 残留，无监听端口占用
- 构建链：`scripts/plugin-build.js` 插件登记表已无 auth-center（票 03 起仅 ai-chatbox / auto-task / file-transfer / session）

### 备注

- 本票执行前曾一度将旧插件改名 `devices→auth-center`（用户中途指令），后确认票 05 已把全部认证语义迁入 session、独立认证中心无存在价值，遂改为**整体退役**（并入 session），与票 06 原语义一致。
- 附带修正：`utils/auth/jwt.rs` 对照测试注释指向 session 插件的 `plugin_token_matches_host_jsonwebtoken_vector`（原指向已删除的旧插件路径）。

## 接管核验与收尾（CodeBuddy，2026-09-20）

**背景**：本票由并行 pi agent 先行执行完毕（其进程 2026-09-19 17:23–16:09Z 独占工作树）。用户裁决「让 pi 让位、我接管」，故终止 pi 进程后在**其成果之上**做独立复核与收尾，不重做已完成项。

### 复核方法

1. 旧 id 全仓 grep（排除 `.git` / `.scratch` 历史档案 / 记忆 / `node_modules` / `target`）：`com.bedcode.(auth-center|devices)` / `bedcode_plugin_(auth_center|devices)` / `plugins/(auth-center|devices)`。
2. 逐个读取 pi 触碰过的**全部**区域（8 个 TRACKED 文件 + 3 个 session 源文件 + file-transfer），逐行判断「是否只有注释变化」。
3. 会话中心产物 vs 源文件时间戳比对（判断 e2e 依赖的 wasm 是否过期）。

### 复核发现：2 处漏网 + 6 处盲替换错句（已修）

pi 的收口用了**全局字符串替换**（旧 id → `com.bedcode.session`），在 5 处产生自指/矛盾句，另漏改 1 处旧 id：

| # | 位置 | 问题 | 修法 |
| --- | --- | --- | --- |
| 1 | `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（auth-policy 说明） | **旧 id 漏网**：「改名前的 `com.bedcode.auth-center` 仍在过渡窗口内」——过渡窗口已随本票关闭 | 改为「独立认证中心插件已整体退役（票 06），不存在第二个候选」 |
| 2 | `utils/auth/auth_center.rs` 模块文档 | 「旧认证中心 `com.bedcode.session` 已无生产消费方（票 06 退役）」——自己说自己没了 | 改为「独立认证中心插件已整体删除，本文件是唯一桥接门；模块名 `auth_center` 作历史命名保留」 |
| 3 | `wasm_runtime.rs::test_host_pairing_bridge_closed_loop` 文档 | 「目标自 `com.bedcode.session` 改指 `com.bedcode.session`」 | 改为「桥接目标 = 会话中心真实产物」 |
| 4 | `host.rs::test_server_auth_policy_closed_loop` 文档 | 「目标自 `com.bedcode.session` 改为 `com.bedcode.session`」 | 改为「策略目标自旧认证中心插件改指会话中心」 |
| 5 | `plugins/session/rust/src/pairing/mod.rs` 模块文档 | 「`com.bedcode.session` 的 policy 仍依赖**该端**的 pairing 副本，故本票只搬入不搬出」——同 id 自指 | 改为「唯一真源（票 06 contract 完成）：双副本窗口关闭；宿主 `utils/auth/` 保留为**降级轨**，两侧仍须同步」 |
| 6 | `plugins/session/rust/Cargo.toml` 依赖注释 | 「版本与 `com.bedcode.session` 同批锁定，避免双副本漂移」——自指 | 改为「版本锚定：取值与宿主 `utils/auth` 语义层对齐（对照测试逐字断言）」 |
| 7 | `plugins/session/rust/src/pairing/jwt.rs` 模块文档 | 「生产签发此刻仍在 auth-center 的命令面（票 06 退役后随签发迁移一并归位）」——已退役，前提失效 | 改为「生产签发**永远**留宿主 `utils/auth/jwt.rs`（spec D3「不动」列），本模块签发面只服务对照测试」 |
| 8 | `plugins/file-transfer/rust/src/auth_center.rs` | 「trust / consent 域自 `com.bedcode.session` 迁入 `com.bedcode.session`」+「与 **devices 插件** serde 形状对齐」 | 改为「自独立认证中心插件迁入会话中心（票 06 已退役）」/「与会话中心 consent 模块对齐」 |
| 9 | `utils/auth/auth_center.rs::enforce_connection_policy` 文档 | 「见 devices 插件 `policy` 模块」 | 改为「见会话中心插件 `policy` 模块」 |

> 教训（建议进 AGENTS.md 或 skill）：退役类 grep 收口**禁止用全局字符串替换**——`old_id → new_id` 会在「迁入/A→B/过渡窗口」这类叙述里制造同 id 自指，必须逐处改语义。同类票（07/15/17）同样是 id 迁移，复用本表作为检查单。

### 退役用例的断言 → 承载方映射（票内条目「保持断言不变」的落实）

`test_auth_center_plugin_artifact_lifecycle`（192 行）整体退役；其断言集合**逐条**落在面向会话中心产物/源码的用例上：

| 原断言 | 现承载方 |
| --- | --- |
| 产物加载 + 激活 + 停用（生命周期） | `wasm_runtime::test_session_plugin_artifact_lifecycle` |
| manifest `id` / `permissions` / `api` 声明面 | 同上（断言 11 项 api + `["auth","peer"]`，比原 9 项更全；且钉住桥接锚点与消费方 api） |
| 状态命令回传 manifest（`plugin`/`permissions`/`api`） | 同上（`session.status` + `domains`） |
| 未知命令显性报错（`Unknown command`） | 同上（`session.ghost`） |
| 配对码：生成 6 位 / 正确验过 / 一次性 / 错误码拒绝 / 清除 | `test_host_pairing_bridge_closed_loop`（经真实宿主桥接路径）+ `test_pairing_dual_track_host_and_plugin_paths_agree`（双轨逐字段对照） |
| QR：32 hex / 验过 / 一次性 / 不匹配分类文案 | 同上 |
| JWT：三段结构 / claims 透传 / 篡改拒绝 | **改接缝**：会话中心无 jwt 命令面（生产签发留宿主，spec D3）→ 由 `host::test_server_auth_policy_closed_loop` 的「宿主 `JwtService` 签发 → 会话中心 `policy` 验签放行 / 撤销售信拒绝」跨实现闭环覆盖（比原用例的插件自证更强），字节级等价另有 `pairing/jwt.rs::plugin_token_matches_host_jsonwebtoken_vector` + RFC 7515 §A.1 官方向量两侧对照 |
| 密钥托管：`key.status` present/key_len 32 + `plugin_secrets` 落库属主行 | `test_host_pairing_bridge_closed_loop` 末尾直查 `plugin_secrets`（`com.bedcode.session` 属主 + `jwt.key` 64 hex → 解出 32B）；长度/非 hex/幂等由 `pairing/keys.rs` 单测覆盖 |
| trust / consent（原已迁出，用例内只留跳转注释） | `test_session_trust_and_consent_api_closed_loop` |

### 本轮附带确认

- **白名单种子 N/A**：`fs_auth.rs::FsAuthChecker::new` 的插件白名单仅 `com.bedcode.auto-task` / `com.bedcode.file-transfer`，旧认证中心从未收编 → 无条目可改（票内条目按实测判 N/A）。
- **构建链**：`scripts/plugin-build.js` / `plugin-dev.js` 的 `PLUGINS` 表、`package.json::plugins:build`、`scripts/dev-run.js` 的 watch 清单均无旧 id（票 03 起四插件：ai-chatbox / auto-task / file-transfer / session）。
- **产物与源一致性**：会话中心 wasm 产物建于 23:00:51，其后的全部源文件改动**经逐行确认为注释级**（`pairing/{mod,jwt}.rs`、`Cargo.toml`）→ 产物无需重建，e2e 断言有效。
- **行为零变更**：pi 与本轮的编辑全部为注释 + 测试删除（lib 用例数 1020 → 1019 与「删 1 用例」一致），无生产代码路径改动。

