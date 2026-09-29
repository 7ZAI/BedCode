# 认证中心持有签发密钥与验签执行（spec）

> 立项：2026-09-29 · 触发：ADR 0031 落地后的架构复核（两个症状的排查过程中暴露）
> 边界单一事实源：`docs/adr/0022-plugin-host-interface-primitive-boundary.md`
> 承接 ADR：`docs/adr/0031-auth-center-registration-and-composable-grant.md`（本专项**修订**其「凭据与密码学不动」口径）
> 本专项 ADR：`docs/adr/0033-auth-center-owns-signing-key-and-verification.md`
> 用户裁定（2026-09-29）：**验签签名与密钥生成等放入认证中心** —— 理由是「认证中心作为内部业务服务是高度可信的」
> 前置：`.scratch/2026-09-29-auth-center-registration/`（注册表 + fail-closed，本专项的起点）

## 1. 背景：两个症状暴露的架构错位

本专项不是从「有个 bug」出发，而是从 ADR 0031 落地后的**架构复核**出发。复核过程中
发现两处错位，都不是 bug，是设计与文档的错位。

### 1.1 错位一：同一张表里躺着两把同名的钥匙

```
plugin_secrets 表
  ('host',                        'jwt.key', …)   ← 宿主 JwtService 签发/验签
  ('com.bedcode.terminal-session','jwt.key', …)   ← 插件自己 getrandom 生成的
```

- 宿主侧：`utils/auth/jwt.rs:19` `JWT_SECRET_KEY_ID = "jwt.key"`，
  密钥经 `utils/auth/host_secrets.rs` 取，属主 `HOST_SECRET_OWNER = "host"`
  （`host_secrets.rs:24`）
- 插件侧：`wasm-apps/terminal-session/rust/src/pairing/keys.rs:31`
  `get_or_create_jwt_key` 在 **guest 内**用 `getrandom::fill` 生成（`keys.rs:42-43`），
  再经 `host-auth secret-set` 写回同表，属主是插件自己

**两把钥匙，同一个 key 名，互不通用。** 隔离是 `WHERE plugin_id = ?1` 做实的
（`host_secrets.rs` 模块头：「插件侧读不到 `host` 行」）。

**并且插件那把是死的。** 生产签发路径
`wasm-apps/terminal-session/rust/src/auth_http/jwt.rs:27` 走的是
`host.auth_device_token_issue(...)`，即绕回宿主；插件自带的 HS256 实现
（`pairing/jwt.rs:78` `sign_hs256` / `:224` `verify_token`）在生产路径上**没有调用点**
（`policy/mod.rs:126,216` 的 `JwtService::with_key([0x42u8; 32])` 是测试夹具）；
`lib.rs:782` 在 activate 里调 `jwt_key_from_host_auth()` **只为打一行日志**
（`log_info("len {}")`），生成后没人拿去签发。

### 1.2 错位二：文档把「不同 key 域」写成了「插件无法验签」

`packages/plugin-sdk-desktop/rust/wit/bedcode.wit` 的 `auth-policy` 接口注释写：

> 入参 = 宿主已验签通过的 JWT token（**验签不在此重复**：插件密钥域与宿主不同，
> 无法也不应验签——spec §3 红线「验签执行点留宿主」）

前半句「插件密钥域与宿主不同」**成立**（§1.1）。后半句「**密钥不出宿主**」与
「无法」**不准确**：插件自己就持有 `jwt.key` 的明文副本并自带完整 HS256 实现。
`utils/auth/jwt.rs:337` 的跨实现锁
`host_jsonwebtoken_matches_plugin_fixed_vector` 恰恰证明两侧能产出**逐字节相同**的
token —— 但它两侧都用**固定注入 key**，只锁了**字节格式**，**没有**锁住两个 key 域
能互通（它们本来就不互通）。

### 1.3 错位三：验签散落在宿主四处

| 位置 | 引用数 | 职责 |
| --- | --- | --- |
| `server/http/middleware/jwt_auth.rs` | 6 | `/api` scope 的 `jwt_gateway` 中间件 |
| `server/http/controllers/plugin_controller.rs` | 3 | 端点级认证档位闸 |
| `server/http/gateway.rs` | 2 | 协议网关 |
| `server/websocket/channel/plugin.rs` | 3 | WS 插件端点首消息 |
| `wasm_core/host_api/auth.rs` | 5 | `device-token-issue` / `device-token-verify` 原语 |

每处各自 `JwtService::new()` 各自验一遍。

## 2. 决定

| 号 | 决定 | 状态 |
| --- | --- | --- |
| **D1** | **签发密钥的生成、签发、验签全部归认证中心**（`com.bedcode.terminal-session`）；宿主不再持有任何设备 JWT 密码学 | ✅ 用户裁定 2026-09-29 |
| **D2** | 中心是**内部业务服务**，作为本机信任锚点，其可信度等同于宿主。这是 D1 的前提 | ✅ 用户裁定 2026-09-29 |
| **D3** | 迁移策略：**接受存量已配对设备全量重新配对**，不做过渡期双密钥验签 | ⚠️ **待确认**（见 §10.1，本 spec 给推荐） |
| **D4** | 新增**密钥轮换**原语。入场密钥进 guest 内存后泄露 = 可伪造任意设备 + 7 天窗口 + **无恢复手段**（`utils/auth/` 下今天**没有任何轮换机制**） | ⚠️ **待确认**（见 §9，本 spec 强烈推荐） |
| **D5** | ABI 破坏性 bump **32 → 33**：`host-auth` 退役 `device-token-issue` / `device-token-verify` | ✅ 由 D1 直接推出 |

## 3. 信任模型论证：为什么「中心可信」成立

D1 表面上违反「验签方应是信任锚」的直觉，需要论证「中心就是信任锚」。

**关键事实：`HS256` 是对称算法**（`utils/auth/jwt.rs:25`
`Algorithm::HS256`）。对称意味着**验签方与签发方持有同一把秘密**。所以：

1. **中心今天已经能伪造。** 它调 `device-token-issue` 就能给任意设备签发入场
   token，而 `enforce_connection_policy` 决定放行。**「中心能签发」与「中心持有
   密钥」的权限差距，只差一个可审计的 API 调用 vs. 一块内存里的裸密钥。**
2. 既然中心已被授予「为任何设备签发入场凭证」的权力，把签发密钥交给它**不增加
   任何实际权限**。
3. 反过来，**验签方必须是签发方**——这是硬约束：中心若要验签，就得能读宿主那把
   密钥，也就必须跨 `plugin_secrets` 属主隔离。这正是 D1 要消除的东西。

**结论**：把密钥与验签收敛到中心，是让实现**匹配既有的授权事实**，而不是扩大授权面。

**这个论证的边界（必须写明）**：它成立的前提是「中心可信」。若将来中心可被第三方
替换（ADR 0031 明确「单中心注册表天然允许未来换人」），则信任锚可被换掉 —— 这在
今天的 fail-closed 模型下也是既有事实（中心停用 = 认证面全断），**本专项不改变
这一点，只是让它更诚实**。

## 4. 性能实测（决策依据，不是估算）

探针：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/auth_center_perf.rs`
（新增，本专项票据 06 一并交付）。真实 terminal-session 产物 + 真实 WASM 导出调用，
`cargo test` dev profile，N=1000，3 轮：

| 段 | 耗时 | 占热路径 |
| --- | --- | --- |
| **A** 宿主原生验签（HS256，进程内） | 5.96 – 7.09 µs/op | ~6% |
| **B** 认证中心往返（guest 导出调用） | 95.7 – 113.5 µs/op | **~94%** |
| 每请求合计（`jwt_gateway` 热路径） | 101.7 – 120.6 µs/op | |
| 中心往返 / 原生验签 | 15 – 17× | |
| 单实例串行吞吐上限 | 8,300 – 9,800 req/s | |

**三点结论**：

1. **平均成本不是问题。** 移动端远程终端的请求量是几十/秒量级，离 9k/s 差三个
   数量级。删掉整个 `enforce_connection_policy` 用户也感觉不到。
2. **crypto 放哪几乎不影响性能。** A 只占 6%。把验签搬进 guest（WASM 里 HS256
   约 10–20 µs）相对 100 µs 的往返仍是小头。→ **D1 与性能解耦**，可以纯按信任
   模型决策。
3. **真正的杠杆在消掉每请求往返**（B 占 94%），且与 crypto 位置无关（本地信任缓存 /
   撤销推送 / WS 建连时裁决 + HTTP 短 TTL 缓存）。**本专项不做**（见 §14）。

### 4.1 已知的既有风险（本专项不修，但要记账）

- `CallModel::Mutex` 是默认（`wasm_core/config.rs:121`）——**每实例一把锁全程持有**，
  同实例的策略调用**串行**，并发不提升吞吐。
- `block_on_async` 落在 actix worker（current_thread 分支，
  `wasm_core/runtime_util.rs:113-133`）→ **每个在飞请求占住一个 actix worker 线程**
  直到完成。`spawn_blocking` 只把 guest 执行搬到阻塞池，worker 本身仍同步等待。
- 超时 `AUTH_CENTER_TIMEOUT_MS = 5_000`。
- 合起来：**N 个慢/ trap 的中心调用 = 整个 HTTP server 停摆最长 5 秒**。触发条件是
  「插件慢」（同实例其它插件做重活时排队），不是外部攻击，但是**可自伤**。

## 5. 目标形态

```
今天（两个 key 域）                      目标（一个 key 域）
─────────────────────────────────────    ─────────────────────────────────────
插件 keys.rs 生成 key（死代码）    ──▶    插件 keys.rs 生成 key（活，唯一真源）
插件 auth_http → 宿主 device-token-issue ─▶  插件 auth_http → 插件本地 sign
宿主 JwtService sign/verify         ──▶    宿主无 JWT 密码学
plugin_secrets ('host','jwt.key')  ──▶    该行成为死数据（票据 02 删除）
四处宿主验签                        ──▶    一处：中心 verify-device-token
```

**收敛点**：中心的 `policy::evaluate`（`wasm-apps/terminal-session/rust/src/policy/mod.rs`）
**本来就已经**在做结构复检 + claims 解析 + 时效 + 撤销检查，只差密码学那一步。
所以这是「补上一行」，不是「新建一条链路」。

## 6. WIT 契约变更（ABI 32 → 33，破坏性）

`packages/plugin-sdk-desktop/rust/wit/bedcode.wit` · `host-auth` 退役 2 函数：

| 函数 | 处置 | 理由 |
| --- | --- | --- |
| `device-token-issue` | **退役** | 中心自持密钥后不再需要宿主代签 |
| `device-token-verify` | **退役** | 同上 |

`host-auth` 保留面（不变）：`secret-*` 四函数 · `auth-setting-set` ·
`biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind` ·
`link-identity-parts` · `auth-center-register` / `auth-center-unregister` /
`auth-methods-list` / `auth-method-invoke`。

**双端偏离登记**（ADR 0022「双端偏离」节追加）：本组 2 函数是**桌面独有**，移动端
WIT/ABI/SDK **不跟演不投影**（移动端是客户端，不承载服务端网关与认证中心角色），
mobile ABI 保持 11。

**fail-visible ②**：旧产物（v32 SDK 构建）在**实例化期**拿到点名缺失 interface
+「按 v33 SDK 重建」的错误（`LoadedWasmPlugin::stale_artifact_rebuild_hint`），
不是 trap、不是静默降级。

## 7. 宿主改造

### 7.1 四处验签调用点改调中心

| 文件 | 现状 | 改后 |
| --- | --- | --- |
| `server/http/middleware/jwt_auth.rs` | `verify_token_with_expiry` → `enforce_connection_policy` | 只调 `enforce_connection_policy`（中心内部先验签再裁决） |
| `server/http/controllers/plugin_controller.rs` | 端点级 `JwtService` | 复用中间件已注入的 claims，或调中心 |
| `server/http/gateway.rs` | `JwtService::new()` | 同上 |
| `server/websocket/channel/plugin.rs` | `verify_endpoint_jwt` | 只调中心 |

**顺序变化**：今天「先宿主验签 → 再问中心策略」两步；改后「问中心一次，中心内部
先验签再裁决」一步。语义等价（中心 `evaluate` 本就独立复检结构/时效），
**失败面收窄为一次调用**。

### 7.2 退役

- `utils/auth/jwt.rs` —— 整个退役（宿主不再有任何 JWT 密码学）
- `utils/auth/host_secrets.rs` 的 `JWT_SECRET_KEY_ID` 用途
- `wasm_core/host_api/auth.rs` 的 `auth_device_token_issue` / `auth_device_token_verify`
- `utils/auth/auth_center.rs::enforce_connection_policy` 保留（改为只调中心，
  `deny_kind` 三态不变）

## 8. 插件改造（`com.bedcode.terminal-session`）

| 文件 | 改造 |
| --- | --- |
| `pairing/keys.rs` | `get_or_create_jwt_key` 从「探活兼生成」变成**唯一密钥源**；`lib.rs:782` 的 activate 探活改为调用它并让失败阻断激活（凭据不可用时显性失败，不静默） |
| `pairing/jwt.rs` | 从死代码变活代码。`sign` / `verify_token` 走生产路径 |
| `auth_http/jwt.rs` | `issue_device_token` / `verify_device_token` 从调 `host-auth` 改为调本地实现 |
| `auth_http/mod.rs:311` | `handle_reauth` 的验签改走本地实现 |
| `policy/mod.rs` | `evaluate` 前置密码学验签（现只有结构/claims/时效/信任检查） |

**wire 兼容性**：token 格式（HS256、三段 base64url、claims 形状 `sub`/`iss`/`iat`/
`exp`/`device_name?`/`fingerprint?`）**逐字节不变** —— 跨实现锁已经证明两侧产出一致。
移动端把 token 当**不透明串**（只存 localStorage、只往上传，从不自行验签或解析），
**移动端零改动**。

## 9. 密钥轮换（D4，待确认）

**为什么必须**：今天入场签发密钥从**不进入 guest**。D1 之后它会明文躺在 guest 线性
内存里。`utils/auth/` 下**今天没有任何轮换机制**（已 grep 确认无 rotate/rotation）。
于是泄露的后果是：可伪造任意设备凭证 + 7 天有效期 + **无法通过任何手段恢复**
（既不能吊销也不能换钥），只能全量重新配对。

**这把「理论风险」换成了「不可恢复风险」。** 故列为 D4。

**建议形态**（最小可用）：

- 中心持有 `key_id`（当前激活的密钥标识，嵌进 claims，如 `kid`）
- 保留上一代密钥用于**验签**（宽限期 = 最长 token TTL = 7 天）
- 轮换触发：手动命令面 + 撤销设备时可级联
- 验签：先试 `kid` 指向的密钥，miss 则试上一代

**若 D4 被否**：必须在 ADR 里显式登记「入场密钥泄露 = 全量重配」为**已接受风险**，
不得沉默。

## 10. 迁移路径

### 10.1 存量设备（影响面已查清）

`wasm-apps/terminal-session/rust/src/auth_http/mod.rs:305-324` 的 `handle_reauth`：
设备用**旧 token 本身**当证明来换新 token（`jwt::verify_device_token(host, &session_token)`）。
**密钥一换，旧 token 验不过 → 换新失败 → 只能走配对码 / QR / 生物重配。**

| 方案 | 做法 | 代价 |
| --- | --- | --- |
| **A（推荐，本 spec 默认）** | 接受全量重配，文档 + UI 显式说明 | 存量设备一次性重配（每个约 30 秒） |
| B | 过渡期双密钥验签：中心一次性接管宿主旧 `jwt.key`，宽限期 7 天内两把都验 | 多一条分支 + **一次长期密钥跨域移动**（正是 D1 要消除的东西） |

**本仓先例**：v24 退役 `pairings` / `connection_history` / `session_configs` 三表
即「不兼容旧版本存量用户」。方案 A 与既有口径一致。

**移动端不需要改**（token 不透明 + `handle_reauth` 是既有端点），但**用户可见**：
重配后 `authenticate` 会拿到 `invalid` 而非网络错误 —— 需在文案上区分
（`common.notification.*` 或中心业务码，ADR 0030）。

### 10.2 发布顺序

本专项是**破坏性变更**（ABI 33 + 存量 token 作废）。参照 ADR 0031 §15 的三步法：

| 阶段 | 宿主 | 中心插件 | 结果 |
| --- | --- | --- | --- |
| 迁移前 | ABI 32，宿主持钥验签 | v32 产物（`device-token-issue` 委托宿主） | 现状 |
| 步骤 1（票 01+02） | ABI 33 代码就位，**宿主仍持钥验签**（旧路径保留） | v32 产物仍可用 | 行为不变，为票 03 留回归窗口与回滚点 |
| 步骤 2（票 03+04+05） | 宿主验签退役，四处改调中心 | v33 产物（自持密钥） | 中间态：宿主已切、中心未升级 → **全部认证拒绝** |
| 步骤 3 | 同上 | v33 产物发布 | 恢复正常（存量设备需重配） |
| 兜底 | `stale_artifact_rebuild_hint` 点名；UI 给「认证中心未就绪 / 需重新配对」显式信号 | — | 排障不靠猜 |

**发布纪律**：步骤 2 与步骤 3 的两个产物（宿主 + 中心插件）**必须同批发布**。
本仓插件产物全部由源码构建（无外部第三方产物），同批成本低，但发布脚本必须把
两个产物绑成原子单元（`scripts/package-plugins.mjs` 口径）。

**降级预案**：步骤 2/3 之间出问题，回滚宿主到步骤 1 版本即可（旧路径仍在），
不需回滚插件产物 —— 这是「步骤 1 保留旧路径」的设计目的。

## 11. fail-visible 三形态（AGENTS §8）

| 形态 | 落点 |
| --- | --- |
| ① 旧读路径显性失败 | `device-token-issue` / `device-token-verify` 退役后，插件若仍调 → **实例化期**缺 interface 即报错，不静默 |
| ② 旧产物点名重建 | `stale_artifact_rebuild_hint`：「按 v33 SDK 重建」 |
| ③ 退役字眼加载即抛 | `packages/plugin-sdk-desktop/bin/manifest-gen.js` 权限词汇自检；宿主侧 `host_auth_device_token_issue` 等退役字眼不得出现（复用 `l2_gating_test.rs` 的 `BRIDGE_PUBLIC_SURFACE` 白名单机制） |

## 12. 失败模式与可观测性

| # | 失败 | 现象 | 处置 |
| --- | --- | --- | --- |
| F1 | 中心未注册 | 全部认证拒绝（`no_center`） | ADR 0031 已承认的可用性耦合；**UI 显式信号本专项补上**（见 §13） |
| F2 | 中心 trap / 超时 | `deny_kind=unavailable`，全部拒绝 | 既有；见 §4.1 记账 |
| F3 | 存量 token 验不过 | `handle_reauth` 返回 `invalid` | §10.1；文案区分「凭证失效」与「网络错误」 |
| F4 | 中心密钥生成失败 | activate 显性失败 | 禁止降级为「用进程随机密钥」后仍对外服务（重启即全灭，比拒绝更糟） |
| F5 | 旧产物 | 实例化期点名 | fail-visible ② |

**日志字段**（AGENTS §8 结构化字段红线，`key = %value` 形式，禁止拼进消息）：
`deny_kind`、`center_id`、`center_owner`、`plugin_id`、`device_id`、`request_id`。
**禁止**把 JWT / claims / 密钥任何片段写进日志或错误文本（凭据红线）—— 包括
`key_id` 之外的一切密钥材料。

## 13. 测试矩阵

| 层 | 覆盖 |
| --- | --- |
| 插件单测 | 密钥生成幂等 · 签发/验签往返 · 错误 key 拒绝 · 过期拒绝 · 结构畸形拒绝 · 轮换后新旧 key 验签（若 D4 通过） |
| 插件策略 | `evaluate` 补密码学前置后，五类裁决（结构/claims/时效/撤销/放行）不变 |
| 宿主单测 | `enforce_connection_policy` 三态 `deny_kind` 不变 · 四处调用点改调后行为等价 |
| 跨实现 | **删或反转** `host_jsonwebtoken_matches_plugin_fixed_vector`（宿主不再产出 token，该不变量失去意义）；替换为「中心实现 vs RFC 7515 官方向量」 |
| 闭环 | 真实产物加载 → 配对 → 签发 → HTTP `/api/*` 准入 → WS 端点准入 → 撤销后拒绝 |
| 迁移 | 旧 token → `handle_reauth` 拒绝且文案可读 |
| 性能 | `auth_center_perf` 探针复测，确认 A 段消失后合计不劣化 |
| 双端 | 移动端全量回归（**不跟演，但须确认不受影响**） |
| 静态锁 | `retired_*` 系列新增「宿主无 JWT 密码学」文本锁（参照 `l2_gating_test.rs` 手法） |

**测试纪律**（AGENTS §3 两段式）：开发中只跑针对性单测；集成测试与全量回归留收尾
（§15）。`unit-test-discipline` skill 强制。

## 14. 票分解

| 票 | 内容 | 面 | 依赖 |
| --- | --- | --- | --- |
| 01 | ADR 0033：落决策、信任模型论证、迁移与轮换要求 | 仓库 | — |
| 02 | 插件侧自持密钥：生成 / 签发 / 验签 + 删宿主 `jwt.key` 行 | 桌面 | 01 |
| 03 | WIT + SDK：退役 2 函数、ABI 33、双端副本同步、移动端偏离登记、stale hint | 双端 | 01 |
| 04 | 宿主四处验签改调中心 + `utils/auth/jwt.rs` 退役 | 桌面 | 02, 03 |
| 05 | 密钥轮换原语（D4，待确认；否决则改为 ADR 登记已接受风险） | 桌面 | 02 |
| 06 | 测试重做 + 回归锁 + 性能探针复测 | 桌面 | 04, 05 |
| 07 | 文档反转：WIT 注释、ADR 0031 修订、`mobile-desktop-auth.md`、code-map、CHANGELOG 双语 | 仓库 | 04 |
| 08 | 中心未就绪 / 需重配的显式 UI 信号（补 ADR 0031 欠账 + 本次迁移用户可见面） | 双端 | 04 |

**顺序理由**：01 先落决策防返工；02/03 可并行（插件侧与契约侧互不阻塞）；
04 是主体；05 与 04 无耦合，可并行；06/07/08 收尾。

**发布原子性**：票 02 + 03 + 04 必须同批（见 §10.2），票 05 若通过也须同批
（否则带着无轮换的密钥发布）。

## 15. Out of Scope

- **不改 fail-closed 语义**（ADR 0031 D1）：无中心仍是一律拒绝。本专项不重开这个讨论。
- **不改移动端 WIT / ABI / 代码**（mobile ABI 保持 11）。移动端只在新 i18n 文案上配合（票 08）。
- **不消每请求往返**（§4 结论 3）：信任缓存 / 撤销推送 / 裁决降频是独立专项。
  本专项只保证 A 段（6%）消失后合计不劣化。
- **不改 token wire 格式**：HS256 + 三段 base64url + 现有 claims 形状逐字节不变。
- **不引入非对称签名**（EdDSA/RS256）：那会让中心持私钥、宿主持公钥，是另一套
  信任模型与另一次存量作废，本专项不做。
- **不动生物凭证验签**（`biometric-verify-signature`）：P-256 公钥由宿主托管，
  与设备 JWT 是两条线。
- 不做认证中心热切换 / 多中心按 method 分派（ADR 0031 D2 排除）。

## 16. 附带清理（同批处理，均已核实）

| 项 | 事实 | 票 |
| --- | --- | --- |
| `utils/auth/auth_center.rs::format_device_display_name` | **死代码**：全仓（含 `tests/`）除自身外零引用，唯一「引用」是 `l2_gating_test.rs:76` 的锁表字符串。注释称「WS 重认证路径仍在宿主使用」已过期 | 04 |
| ADR 0031 Consequences 承诺的「中心停用 UI 提示」 | **未落地**：`bedcode-desktop/src/`、`wasm-apps/terminal-session/src/`、`bedcode-mobile/src/` 三处搜 `no auth center registered` / `no_center` / `auth center` **零命中** | 08 |
| `call_api` 归属错位 | 被 `utils/session_gateway.rs:44` 当**通用互调 JSON-RPC 客户端**用（会话 API，不只认证中心），却住在 `auth_center.rs` 且注释写「调用认证中心互调 api」 | 04（顺手上提到中性位置，或明确拆名） |
| `utils/auth/auth_center.rs::session_active(host_ctx)` | 参数 `host_ctx` 已无用（`let _ = host_ctx;`），注册表是全局 | 04（清理或注释说明为何保留签名） |

## 17. 未决事项（开工前需用户确认）

| # | 事项 | 本 spec 推荐 |
| --- | --- | --- |
| **Q1** | 迁移策略 D3：接受全量重配（A）还是过渡期双密钥（B） | **A**，与 v24 退役三表的既有口径一致，且 B 要额外搬一次长期密钥 |
| **Q2** | 密钥轮换 D4：本批做还是登记为已接受风险 | **做**。不做等于把「理论风险」换成「不可恢复风险」 |
