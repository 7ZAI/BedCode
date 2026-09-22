# 07: 认证链 HTTP 面下沉（auth 转发插件 + 插件回调 auth 原语统一认证）

**What to build:** 把 `/api/auth/*` 七端点（pairing / verify / qr-connect / reauth / biometric-challenge / biometric-verify / biometric-bind）的编排从宿主 controller/service 整体搬入 `com.bedcode.session` 插件：网关按「公开路由」策略直接转发（这些端点在 JWT 之前，无验签前置），插件编排后经 host-auth 原语完成统一认证——**密钥托管与信任表留宿主，签发/验签执行在插件**。认证插件默认常开（用户裁定：`auth_center` 一直开启），不再做 HTTP 面的宿主降级轨；宿主 `auth_controller.rs` / `auth_service.rs` 退役。

**决策依据:** 用户裁定（2026-09-21）：「即使是认证链也应该直接转发到对应插件，再由插件调用 auth_center 统一认证，而不是放在宿主侧；开关的问题可以默认 auth_center 一直开启」——推翻 `.scratch/2026-09-20-host-business-decarriage/spec.md` Out of Scope 中「认证链保持现状」的边界；ADR 0022 分层口径随之修订（编排与执行 → 插件；密钥托管 / 信任表 / 记录面 → 宿主）。

**Blocked by:** 01（网关地基）；与 02-04 同范式（别名转发 + 形状逐字节 + contract 退役）

**Status:** ready-for-agent

- [ ] 七端点 URL 与响应形状逐字节一致（含错误码 1001/1005/1006/1007/1008/1009/1010 文案、`kdPublicB64`/`kdFingerprint` 缺席形态、`ConnectionHistory` 记录语义）
- [ ] 网关公开路由策略：Public 条目不要求 JWT 前置即可转发；未激活/未声明 → 明确「插件未激活」错误（无宿主降级）
- [ ] 插件签发的 JWT 与宿主 `JwtService` 逐字节同构（HS256 + 同一 secret-store 密钥 + 同 Claims 形状），宿主中间件 `enforce_connection_policy` 无感
- [ ] 生物认证：挑战状态机在插件（60s TTL 单次消费）；签名验证经 host-auth 原语（公钥不出宿主，凭据红线保持）
- [ ] 信任表写面经 host-auth 原语（upsert/touch/history-record），`pairings` / `connection_history` 表仍留宿主
- [ ] 桌面前端事件（`pairing-code-generated` / `qr-token-consumed` / `device-connected`）经 host-events.emit，事件名与载荷形状逐字节一致
- [ ] 宿主退役：`auth_controller.rs` / `auth_service.rs` 删除（`format_device_display_name` 移 `utils/auth` 供 WS 路径）；命令面（`commands/system.rs` → auth_center 桥接）与 D7 降级不动
- [ ] 插件 native 契约测试（JWT 同构性 / 挑战状态机 / 失败文案）+ 真实 wasm 闭环 + cargo test / vitest / eslint 全绿

## Comments

### ① 能力缺口与 WIT 追加（host-auth 函数级追加 ×6，desktop WIT 保持 v19）

| 追加 | 语义（引擎级） | 对应宿主实现 |
| --- | --- | --- |
| `trusted-device-upsert(record-json) -> id` | 信任记录写入（`add_pairing` 同语义：uid_hash 归并 / connect_count / last_seen；`publicKey` 缺省 = 保留既有值防生物凭证清空） | `db::add_pairing` |
| `trusted-device-touch(fingerprint, address?)` | 连接计数 / last_seen 刷新（`update_pairing_last_seen`） | `db::update_pairing_last_seen` |
| `connection-history-record(record-json)` | 连接历史追加（method/result/address?） | `db::record_connection_event_by_fingerprint` |
| `biometric-credential-bound(fingerprint) -> bool` | 「已配对且绑定公钥」查询（挑战签发闸门） | `get_pairing_by_fingerprint` 判定 |
| `biometric-verify-signature(fingerprint, message, signature) -> bool` | P-256 ECDSA 验签（用**宿主托管**的绑定公钥，明文/密钥不出宿主——凭据红线） | `verify_biometric_signature` |
| `link-identity-parts() -> option<{publicB64, fingerprint}>` | 链路身份 Kd 公钥材料读取（响应字段 `kdPublicB64` / `kdFingerprint`） | `link_crypto::identity_parts` |

不需要新增的：JWT 签发（插件已持同一 secret-store 密钥 + 自实现 HS256，`policy::verify_device_token` 今日已在验同一批 token，签发对称成立）；配对码 / QR 状态机（插件已有）；前端通知（`host-events.emit` 现成）。

### ② 网关公开路由策略

`BusinessRoute` 增 `auth: RouteAuth::{Authenticated, Public}`：Public 条目跳过「已验签」前置与 device 上下文注入（`/api/auth/*` 在 jwt 中间件本就放行），判定纯函数与测试同步；七条 auth 条目 = Public + PluginRequired。错误口径：插件未激活 → 既有 1007 信封。

### ③ 密钥与信任边界（ADR 0022 修订口径）

- **留宿主**：secret-store（`jwt.key` 及全部密钥托管）、`pairings` / `connection_history` 表、生物凭证公钥托管与验签执行（新原语）、link_crypto 身份。
- **移插件**：七端点编排、配对码 / QR / 挑战状态机、JWT 签发与 Claims 构造、连接历史与信任记录的**调用决策**（经原语写）。
- 桌面命令面（`commands/system.rs` → `auth_center` 桥接 → 插件）不动，其 D7 降级轨保留（HTTP 面无降级 = 用户裁定的「常开」语义）。

## Comments（追加，2026-09-22 认证记录下沉）

### ④ 2026-09-22 用户裁定：认证记录下沉，逆转本票「信任表留宿主」结论

本票 §③ 的「`pairings` / `connection_history` 表留宿主」结论被替换（`.scratch/2026-09-22-auth-records-downsize/spec.md`，ready-for-agent 已实施完成）：

- 配对设备 / 连接历史真源 → 认证中心私有库 `auth_records` 域（`auth_pairings` / `auth_connection_history`）；宿主主库两表 + `session_configs` 退役（schema.sql 删 CREATE，存量由宿主 handoff `plugin/auth_records_migration.rs` 一次性迁入，成功即 DROP）
- host-auth **记录面原语**（trusted-devices-list / revoke、connection-history-list，本票 ① 表的上四行）+ host-session 配置读取面随表退役（WIT/SDK/ABI v24）
- **保持宿主**（不变）：secret-store（`jwt.key`）、生物凭证公钥托管与验签执行（迁移后键位 `plugin_secrets` key=`biometric:<fp>`）、link_crypto 身份、`auth-policy` capability（策略取用，传输失败回退放行）
- WS 认证/断连的配对记录刷新改经互调 api 通知认证中心（`connection-touch` / `connection-close`，异步 fire-and-forget 防 actix current_thread 自锁）
- 本票验收单中「信任表写面经 host-auth 原语（upsert/touch/history-record），`pairings` / `connection_history` 表仍留宿主」一项随 v24 退役；七端点编排下沉与 JWT 同构、网关公开路由等其余验收不变
