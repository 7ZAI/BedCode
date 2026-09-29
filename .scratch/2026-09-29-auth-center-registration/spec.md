# 认证中心显式注册 + 组合式认证原语（spec）

> 立项：2026-09-29 · 触发事故：移动端连接日志风暴（4001 × 616 次 / 98 秒）
> 边界单一事实源：`docs/adr/0022-plugin-host-interface-primitive-boundary.md`
> 本专项 ADR：`docs/adr/0031-auth-center-registration-and-composable-grant.md`
> 用户裁定（2026-09-29）：**D1 无中心 = 拒绝（fail-closed）** · **D2 单中心** · **D3 凭据与密码学不动** · **D4 架构调整**

## 1. 背景：今天这个 bug 的根因不是「桥接」，是「没有注册语义」

事故现象（`.dev-logs/android-dev.2026-09-29.log` + `dev-run.2026-09-29.log`）：

```
移动端：[WsClient] Server closed: auth-policy not provided by this plugin (4001)
桌面端：WARN auth_center: 多个插件导出 auth-policy capability，取第一个作为认证中心
        candidates: [agent-hub, ai-chatbox, file-transfer, terminal-session]
        center: com.bedcode.agent-hub          ← 选错了插件
桌面端：WARN jwt_auth: HTTP /api request denied by auth center policy
移动端：616 次自愈重连 / 98 秒（≈6.3 Hz，无退避）→ toast 洪水
```

根因链（**不是**「宿主桥接认证中心」这个形态本身有问题）：

1. SDK 让 `wasm_entry!` **无条件导出** `auth-policy`（默认拒绝实现，
   `packages/plugin-sdk-desktop/rust/src/wasm_auth_policy.rs` 模块头 + `wasm.rs:152`），
   于是**任何**用 SDK 构建的应用都被探测为候选；
2. 宿主取「按 id 升序的第一个候选」当中心（`utils/auth/auth_center.rs:150`）——
   **无锚点、无唯一性仲裁**，`com.bedcode.agent-hub` 排序第一被选中；
3. 只有 `com.bedcode.terminal-session` 真正实现了策略（`wasm-apps/terminal-session/rust/src/lib.rs:889`），
   agent-hub 走 SDK 默认实现 → 每个持 JWT 的请求/连接都被拒；
4. 移动端丢弃 close code（`connection/ws_client.rs:265-273` 只取 reason 字符串），
   4001（认证被拒）被当成网络断连送进自愈重连，且每轮重置 `is_reconnecting` 回到 `1/3`，
   永远升不到 `max_retry` → 无限循环 + 6 Hz 日志/toast 风暴。

**更深的问题**：同一个「认证中心」概念存在**两条互不一致的发现路径**——
桥接路径（配对码/QR/trust/consent）走 api_registry 锚点
（`auth_center.rs:54` `SESSION_MARKER_API`），裁决路径走能力探测 + 排序取首个。
两套发现机制并存，本身就是病灶。

**测试为什么没抓到**：`wasm_core/manager/host/tests/system_component_test.rs:189`
的闭环测试只激活 terminal-session **一个**候选，「取第一个」恰好正确 → 多候选场景零覆盖。

## 2. 概念对齐：认证中心 ≡ 微服务架构的 auth center

用户裁定（2026-09-29）：**认证中心与微服务中的认证中心功能概念一致，
只不过这里没有也不需要服务发现。**

| 微服务 | 本仓对应 | 差异 |
| --- | --- | --- |
| 服务发现（etcd / consul） | **无** | 注册表就是宿主进程内的 `AuthCenterRegistry`；同进程同 ABI，无网络拓扑问题 |
| auth server（Keycloak / auth-service） | 认证中心 wasm-app（当前 `com.bedcode.terminal-session`，未来可换） | 形态是 wasm 组件，生命周期随插件激活/停用 |
| API Gateway（所有请求先过网关鉴权） | 宿主 server 网关：HTTP `/api/*` + WS 首消息 | 网关是宿主 server，不是独立进程 |
| 网关委托 auth server 裁决 | 网关调**注册表**里的中心 | 从「猜一个」改成「查注册表」 |
| auth server 暴露多种 grant（password / otp / oauth） | 中心注册 `methods`（`pairing_code` / `qr` / `biometric` / `jwt`），其他 app 经原语调用 | 同一形状、零复制 |
| auth server 不可用 | **fail-closed 拒绝**（D1） | 微服务里 auth server 挂掉网关也是 5xx/拒绝 |
| 单 auth server 部署 | **单中心**（D2）：第二个注册者被拒 | — |

## 3. 决定

| # | 决定 | 依据 |
| --- | --- | --- |
| K1 | **认证中心角色经 `host-auth.auth-center-register` 显式注册进宿主注册表**，宿主只做唯一性仲裁 + 句柄登记 + 停用回收 | 用户裁定「注册到宿主中」；消灭两套发现路径 |
| K2 | **所有需认证面（HTTP `/api/*` + WS 插件端点首消息）统一裁决**：`验签(引擎) → 问注册中心` | 用户裁定「所有需要认证的请求都由认证中心判断」 |
| K3 | **无中心 = 拒绝**；中心调用失败（实例缺失/trap/超时）= 拒绝。**删除现有两条 fail-open 降级** | D1；§8 安全红线「fail-safe 默认如无应答/超时即拒」 |
| K4 | **单中心**：已有中心在册时第二个注册者被显式拒绝（含属主 id） | D2；避免 method 冲突裁决变成宿主业务逻辑（B2） |
| K5 | **凭据与密码学不动**：`secret-store` / `biometric-verify` / `device-token-issue` / `link-identity-parts` 全部保留，宿主仍执行 HS256 验签与 P-256 验签 | D3；WIT v24 已定「凭据与密码学不动」，§8 凭据红线 |
| K6 | **组合式认证原语**：`auth-methods-list` / `auth-method-invoke` 两个新函数，让其他 wasm-app 复用中心已实现的认证方式，**不再各复制一份配对码/QR/生物** | 用户裁定「wasm-app 通过认证中心提供的多种认证方式进行组合式认证」 |
| K7 | **桥接门一并切注册表**：`session_active()` 改查认证中心注册表，退役 `SESSION_MARKER_API` 锚点 | 留锚点=留第二套发现路径=留同型病灶 |
| K8 | 注册权限位复用**已有 `auth`**（`PERMISSION_AUTH`，host-auth 域已用），不新增词汇位 | 权限词汇单一真源（`permission.rs`）；ADR 0020 审批流使第三方插件注册需用户批准 `auth` |
| K9 | **认证中心先于其他 wasm-app 加载**：认证中心声明 `PluginKind::System`（复用现成机制，`system` 角色生产零使用者） | 用户裁定；fail-closed 下中心必须先就位，否则其他应用首个请求就被误拒 |

## 4. 注册面契约

### 4.1 WIT（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`，`host-auth` 追加 4 函数）

```wit
/// 注册本插件为**认证中心**（单中心角色）。入参 = 本中心提供的认证方式
/// 标识列表（声明式，宿主不解释其语义）。成功返回中心句柄 `authc-<uuid>`。
/// 已有中心在册 → err（点名在册属主）；无 `auth` 权限位 → err。
auth-center-register: func(methods: list<string>) -> result<string, string>;
/// 注销本插件的认证中心角色（仅属主本人可调）。
auth-center-unregister: func() -> result<_, string>;
/// 列取当前认证中心提供的认证方式（无中心 → err）。其他插件据此发现可用方式。
auth-methods-list: func() -> result<list<string>, string>;
/// 经认证中心执行一次认证方式调用（零解析窄转发：params 原样透传给中心，
/// 返回值原样透回）。method 必须是中心注册表内的方式，否则 err。
auth-method-invoke: func(method: string, params: string) -> result<string, string>;
```

- **纯增量**（旧产物不 import 仍可实例化），但**行为破坏**：不带注册原语的中心插件
  激活后不会注册中心 → 按 K3 全部认证面拒绝。故须走 fail-visible 形态 ②（见 §7）。
- 权限门：4 个函数全部 `check_permission(perm, plugin_id, PERMISSION_AUTH, ...)`。

### 4.2 宿主注册表（新增 `wasm_core/host_api/auth_center.rs`）

```rust
struct AuthCenterEntry { center_id: String, owner: String, methods: Vec<String> }
static CENTER: OnceLock<Mutex<Option<AuthCenterEntry>>>   // 单中心（D2）
```

| 函数 | 语义 |
| --- | --- |
| `register(owner, methods) -> Result<String>` | 唯一性仲裁 → 铸 `authc-<uuid>` → 登记 → `tracing::info!` |
| `unregister(owner) -> Result<()>` | 属主校验（非属主 → `err("not owner of auth center")`） |
| `center() -> Option<AuthCenterEntry>` | 裁决面只读 |
| `is_registered() -> bool` | 桥接门（替代 `session_active()` 的锚点判据） |
| `purge_for_plugin(plugin_id)` | 停用回收（只碰本人） |

- 锁：`std::sync::Mutex`（瞬时操作、无跨 await），同 `host_api/mdns.rs` 口径。
- 登记时机与 mdns/ws/http 一致：激活期登记、停用期 `purge_for_plugin`
  （接线点 `manager/host/activation.rs:693-702` 那一组）。

### 4.3 中心侧（`com.bedcode.terminal-session`）

- 激活时 `host_auth::auth_center_register(&["pairing_code","qr","biometric","jwt"])`；
- 停用时 `auth_center_unregister()`（`purge_for_plugin` 兜底，防 guest 未跑 deactivate）；
- 组合式调用的实现面：中心新增互调 api `auth-grant`（契约见 §4.3.1）。
  **既有 `/api/auth/*` 端点与真源零改动**（`auth-method-invoke` 是新增出口，不是搬家）。

#### 4.3.1 `auth-grant` 互调 api 契约

沿用仓内既有互调 wire 约定（`packages/plugin-sdk-desktop/rust/src/api_call.rs:1-16`：
请求 topic `bedcode.api.<plugin-id>.<method>`，payload `{jsonrpc, id, method, params}`，
`params` 为 **camelCase 具名对象**，与 guest 形参名一一对应）：

```rust
/// 认证中心按 method 分派一次认证方式调用。
/// params.method ∈ 本插件注册时声明的 methods；params.params 原样透传给该方式实现。
/// 成功 → 该方式的返回形状（各 method 自定，见 §4.4 表）；
/// 失败 → ADR 0030 错误信封 `{ code, request_id, params? }`（code 为插件自己的业务码）。
#[api("auth-grant")]
fn auth_grant(method: String, params: serde_json::Value)
    -> Result<serde_json::Value, String>;
```

宿主侧 `auth-method-invoke(method, params)` → 查注册表 → 找到中心 → 经
`call_api(host_ctx, "com.bedcode.terminal-session.auth-grant", {"method":…, "params":…})`
转发 → `result` 原样透回调用方。**宿主全程零解析**（不拆 `params`、不解释 `method`
的业务含义，只校验「method 在注册表内」——那是安全闸门不是解释，B1 不命中）。

| 边界情况 | 宿主行为 |
| --- | --- |
| 无中心在册 | `err`：`no auth center registered`（fail-closed，与裁决面同措辞） |
| `method` 不在注册表 methods 内 | `err`：点名 `method` 与在册 methods 列表（不猜、不回退到"试试别的"） |
| 中心调用失败（未激活/超时/trap） | `err`：`auth center unavailable: <原因>` |
| 中心返回错误信封 | 原样透传给调用方（业务拒绝，**不**吞成宿主错误） |

#### 4.3.2 组合式认证端到端示例

**场景 A：非中心插件只需要"续期自己的 JWT"**（组合 = 单方式）

```rust
// agent-hub guest
let token = host_auth::device_token_issue(sub, name, fp)?;        // K5 引擎原语
let ok = host_auth::auth_method_invoke("jwt", json!({"action":"verify","token":token}))?;
```

**场景 B：非中心插件要给"本机用户"做生物认证"**（组合 = 多方式 + 回退）

```rust
// 某插件要确认本机用户身份，二选一：系统有生物凭证 → 生物；否则 → 配对码
let methods = host_auth::auth_methods_list()?;                    // ["biometric","pairing_code","qr","jwt"]
let challenge = host_auth::auth_method_invoke("biometric", json!({"phase":"challenge"}))?;
let verified = host_auth::auth_method_invoke("biometric",
    json!({"phase":"verify","challenge":challenge,"signature":sig}))?;
if !verified["ok"].as_bool().unwrap_or(false) {
    // 回退：配对码（组合发生在调用方，中心不做策略选择）
    let code = host_auth::auth_method_invoke("pairing_code", json!({"phase":"issue"}))?;
}
```

**关键设计点**：**组合逻辑在调用方，中心只提供原子方式**——中心不做"哪个方式优先/何时回退"
的业务判断（那是 B2，宿主和中心都不该有）。这与微服务 auth server 提供多个 grant、
由客户端/网关决定用哪个 grant 的分工一致。

各 method 的入参/出参形状**复用中心既有实现**（`pairing/` `auth_http/biometric.rs`
`auth_http/jwt.rs` 的内部形状），本票不新增形状定义，只固定「哪个 method 对应哪段既有实现」
的映射表（票 04 落地时逐条列进代码注释）。

## 5. 裁决面（K2 + K3）

```
HTTP /api/*            ┐
WS 插件端点首消息       ├→ 验签(宿主 JwtService) ──失败→ 拒绝(401/4001)
                       │        └ 成功 ↓
                       │   AuthCenterRegistry::authorize(token)
                       │        ├ 无中心                → Deny("no auth center registered")
                       │        ├ 调用失败(trap/缺失)   → Deny("auth center unavailable: …")
                       │        ├ 中心 Err(reason)      → Deny(reason)   ← 拒绝原因透出
                       │        └ 中心 Ok(claims)       → Allow
                       └→ Allow 则挂 claims 建连/放行；Deny 则 401 / close 4001
```

**删除清单**（fail-visible 形态 ①）：

| 现状 | 处置 |
| --- | --- |
| `auth_center.rs:150` `candidates.first()` | **删**，改查注册表 |
| `auth_center.rs:151-160` 「无候选 → `Ok(())` 放行」 | **删**，改 Deny |
| `auth_center.rs:166-172` 「传输失败 → `log_fallback` + 放行」 | **删**，改 Deny |
| `host.rs:709 auth_center_candidates()` | **删**（含 `auth_center_candidates` 全仓引用） |
| `auth_center.rs:54 SESSION_MARKER_API` + `api_registered()` | **删**，`session_active()` 改查注册表 |

### 5.1 中心状态 × 裁决结果 → 认证面行为矩阵（实现无歧义表）

| 中心状态 | `call_auth_policy` 返回 | 认证面行为 | 原因文案（WS close reason / HTTP 提示） |
| --- | --- | --- | --- |
| 未注册 | 不调用 | **拒绝** | `no auth center registered` |
| 已注册 · 停用中（实例已回收） | 外层 `Err` | **拒绝** | `auth center unavailable: plugin '<id>' not loaded` |
| 已注册 · 实例在 · 调用 trap | 外层 `Err` | **拒绝** | `auth center unavailable: <trap 摘要>` |
| 已注册 · 超时 | 外层 `Err` | **拒绝** | `auth center unavailable: timeout after 5000ms` |
| 已注册 · 中心放行 | 内层 `Ok(claims)` | **放行** | — |
| 已注册 · 中心拒绝（撤销/策略） | 内层 `Err(reason)` | **拒绝** | 中心原样给的 `reason` |

三类拒绝必须**可区分**（日志字段 `deny_kind = no_center | unavailable | policy`，
禁止只拼进消息字符串——AGENTS §8 结构化字段红线）：排障时「中心没起来」和
「用户撤销了设备」是两类完全不同的问题。

### 5.2 性能：中心查询不得每请求遍历全部插件

现状 `auth_center_candidates()` **每个认证请求**都遍历全部激活插件 + 排序（6.3 Hz 循环下
被放大 616 次）。改为注册表后是 O(1) 查表，但**裁决的 IO 部分**（`call_guest`）仍在关键路径上：

- 中心裁决是**每次请求一次 guest 调用**（同实例串行红线，ADR 0029）——这是有意的：
  策略撤销必须即时生效，不允许缓存裁决结果（缓存 = 撤销后仍放行的安全洞）；
- 但**中心 id 的解析**（`center()` 查表）零成本，不缓存也会很便宜；
- 因此**不做中心 id 缓存**（避免引入"注册表变了但缓存没变"的不一致面），
  票 03 里那条"缓存化"改为：**只缓存 `candidates` 的废弃删除**，不缓存中心 id。

## 6. 组合式认证面（K6）

- 其他 wasm-app 要认证能力时：`auth-methods-list()` 发现有什么 → `auth-method-invoke(method, params)` 调用。
- 宿主实现是**零解析窄转发**（同 `utils/session_gateway.rs` 口径）：查注册表 → 经
  `api_registry` 找到中心的 `auth-grant` → JSON-RPC 转发 → 原样透回。
- 宿主**不解释** `params`（B1 不命中）、**不持有**任何认证记录（B3 不命中：真源在中心私有库 `auth_records`）。
- 与既有 `host-api-call`（ADR 0017 插件直连中心）的区别：本面是**经宿主网关**的窄转发，
  宿主做路由与属主校验，不做解释；插件直连互调仍保留给非认证类调用。

## 7. fail-visible 三形态（§8）

| 形态 | 落点 |
| --- | --- |
| ① 旧读路径显性失败 | 无中心 → 401/4001 **带点名原因**（`no auth center registered`），不静默放行、不返回空 |
| ② 旧产物实例化期点名 | 新中心插件若不带注册原语 → 激活后无中心 → 全部认证面拒绝。为免「装上就连不上」，`stale_artifact_rebuild_hint` 增加条件：manifest 声明 `auth` 且是已知中心角色的旧产物 → 实例化期点名「按 v32 SDK 重建以注册认证中心」 |
| ③ 退役词汇加载即抛 | `auth_center_candidates` / `SESSION_MARKER_API` / 两条 fail-open 分支**从代码删除**（删即抛），并加防回接锁断言这些字眼不在宿主出现 |

## 8. 双端偏离（ADR 0022「双端偏离」节）

- `host-auth` **已是桌面独有接口**（v15 密钥托管 / v18 记录面已退役 / v24 `auth-setting-set` 保留），
  本次 4 个新函数**移动端不跟演**：mobile WIT 副本不动、mobile ABI 保持 **11**。
- desktop ABI **31 → 32**（`host-auth` 函数级追加）。
- 移动端是客户端：受影响的只有「桌面端拒绝时移动端的表现」→ 移动端**必须同步修**：
  4001 不得进自愈重连（K9，另见移动端票）。

## 9. 移动端配套（独立于本专项的边界变更，但同批交付）

| 票 | 内容 |
| --- | --- |
| M1 | `connection/ws_client.rs` 保留 close **code**（`ServerClosed { code, reason }`），`ConnMonitor` 对认证类 code（4001/4003）判**致命** → 不自愈、只发一次 toast、提示需重新配对/重连 |
| M2 | 自愈重连加**最小退避**（下限 ≥1s）+ 熔断（连续 N 次同因失败 → 停），杜绝任何 6 Hz 风暴（纵深防御：即使桌面再出别的快速拒绝，也不会打爆日志） |

## 10. 测试矩阵

**票 01 注册表（inline 单测）**：注册成功 / 重复注册被拒且点名属主 / 属主注销校验 /
非属主注销拒绝 / 停用回收只碰本人 / 无中心 `authorize` 拒绝（fail-closed）/
中心 `Ok` 放行 / 中心 `Err` 拒绝且原因透出 / 调用失败拒绝（fail-closed，**反例锁**）。

**票 05 闭环 + 回归锁**（本事故的专属锁）：

1. **多候选锁**（真源回归）：同时激活两个导出 `auth-policy` 的插件、只注册其中一个 →
   裁决必须落在**注册者**身上，未注册者即使排序在前也不被选中（复刻本次事故）。
2. fail-closed 锁：未注册中心 → `enforce_connection_policy` 必须 `Err`（不是 `Ok`）。
3. 防回接锁：`auth_center_candidates` / `SESSION_MARKER_API` / `log_fallback` 放行分支
   不得在宿主出现（`retired_auth_center_discovery_is_not_reintroduced`）。
4. 中心停用 → 认证面立即拒绝（fail-closed，不静默放行）。
5. **三类拒绝可区分**：`no_center` / `unavailable` / `policy` 各自的 `deny_kind` 断言（spec §5.1）。
6. 错误文本不含凭据片段（AGENTS §8 凭据红线）。

**移动端单测**：4001 不触发 `reconnect()`；退避下限生效；同因熔断生效。

## 11. 票分解

| 票 | 内容 | 面 |
| --- | --- | --- |
| 01 | 宿主认证中心注册表 `host_api/auth_center.rs` + inline 单测 | 桌面 |
| 02 | WIT `host-auth` 追加 4 函数 + SDK 绑定 + ABI 32 + 移动端偏离登记 | 双端（移动端仅记录） |
| 03 | 裁决面切换 + fail-closed + 删 5 处退役面 + 桥接门切注册表 | 桌面 |
| 04 | 中心侧：terminal-session 注册 / 注销 + `auth-grant` 互调 api 分派 | 桌面 |
| 05 | 闭环测试 + 5 条回归锁 + `stale_artifact_rebuild_hint` 条件 | 桌面 |
| 06 | 文档：ADR 0031 / 本 spec / `mobile-desktop-auth.md` / `plugin-development-checklist.md` / `AGENTS.md` §8 / 两端 code-map / CHANGELOG 双语 | 仓库 |
| M1 | 移动端：close code 保留 + 4001 致命不自杀 | 移动 |
| M2 | 移动端：自愈最小退避 + 同因熔断 | 移动 |

## 12. 认证中心优先加载与 wasm 分类现状（用户裁定：认证中心先于其他 wasm-app）

用户裁定：**认证中心应该优先于其他 wasm-app 应用加载。** 本节先摆清桌面端 wasm 核心
**有没有分类、分哪几类**，再定优先级机制。

### 12.1 分类现状：两套正交枚举，四个应用全部落在同一格

| 分类轴 | 枚举（SDK 真源） | manifest 字段 | 取值 | 语义 |
| --- | --- | --- | --- | --- |
| **装配角色**<br>`PluginKind`<br>（`types.rs:167`） | `Application`（缺省）\| `System` | `type` | `application` | 消费能力，经 `dependencies` 声明依赖 |
| | | | `system` | 内置、**先于应用插件激活**、启停不持久化、向能力注册表提供 host-* 同形接口能力 |
| **产物形态**<br>`PluginType`<br>（`types.rs:145`） | `Rust` \| `RustTs` \| `TsOnly` | `pluginType` | `rust` / `rust-ts` / `ts-only` | 纯后端 / 前后端 / 纯前端（与角色**正交**） |

四个 wasm 应用的现况（`wasm-apps/*/plugin.json`）：

| 插件 | `type` | `pluginType` |
| --- | --- | --- |
| `com.bedcode.terminal-session` | （缺省）`application` | `rust-ts` |
| `com.bedcode.agent-hub` | （缺省）`application` | `rust-ts` |
| `com.bedcode.ai-chatbox` | （缺省）`application` | `rust-ts` |
| `com.bedcode.file-transfer` | （缺省）`application` | `rust-ts` |

**结论：`system` 角色在生产环境当前零使用者**（唯一使用点是测试
`host/tests/system_component_test.rs:90`）。即：优先加载的**机制早已存在且在启动序列里
就位**，只是没有任何插件声明这个角色。

### 12.2 `system` 角色实际生效的三条语义（逐条核过代码，区分「已实现」与「仅文档」）

| 语义 | 实现 | 位置 | 状态 |
| --- | --- | --- | --- |
| **先于应用插件激活** | `activate_system_components()` 在 `auto_activate_from_persisted_state()` **之前**执行 | `manager/host/boot.rs:93`、`host.rs:515` vs `:519` | ✅ 已实现；按 id 排序保证确定性；**单个失败不阻断其余**（落 Error 态） |
| 启停不持久化 | `System` 插件跳过持久化状态写入 | `manager/host/activation.rs:861` | ✅ 已实现（停用仅当前会话生效，下次启动自启） |
| 向能力注册表提供 host-* 同形能力 | `register_system_capabilities()` | `manager/host/activation.rs:571`、`host/wasm.rs:197` | ✅ 已实现，但**当前只有 `host-storage` 可路由**（`capability.rs:96 ROUTABLE_CAPABILITIES` 仅一项）；`auth-policy` 是**仅探测不路由**（`capability.rs:108` 注释：注册为路由提供者会让任意插件接管认证策略，语义错误） |
| **只停不删** | — | `install.rs:153 uninstall_plugin` | ⚠️ **名实不符**：卸载的唯一规则是「仅未启用插件可卸载」，**没有任何按 `kind` 拒绝的分支**。仅存在于 `PluginKind::System` 的文档注释里 |

### 12.3 决定 K9：认证中心 = L2 角色，暂留 terminal-session（已裁定 4b）

**加载顺序（用户 2026-09-29 裁定）**：`L1 基础服务 → L2 内部统一业务 → L3 wasm-app
（业务插件 wasm + 业务 worker 同批）`。与现有启动序列的对应（`host.rs:505-520`）：

| 启动序列步骤 | 现状 | 改造后 |
| --- | --- | --- |
| 步骤 4 | `activate_system_components()`（`boot.rs:93`）—— 单一 System 批 | **拆两批**：先 L1 基础服务，再 L2 内部统一业务（批内按 id 排序，确定性） |
| 步骤 5 | `auto_activate_from_persisted_state()` | **不变**——它就是 L3 批 |

**认证中心归属（已裁定 4b）**：暂留 `com.bedcode.terminal-session`，由它**兼任 L2**。
拆分为独立 `com.bedcode.auth-center` 留作后续专项——不与本批绑定，避免把
注册 + fail-closed 的核心绑架在六域（配对/QR/生物/JWT/策略/信任/记录）拆分风险上。

**已作废的中间方案**（曾在本节被推荐，现明确作废）：

| 方案 | 作废理由 |
| --- | --- |
| 给 `terminal-session` 加 `"type": "system"` | `system`/L1 定义是「**提供 http/mdns/pty 同形能力**」；terminal-session 是**消费者**（`dependencies: ["host-pty"]`，不提供任何 host-* 同形能力），打该标签等于**把消费者标成提供者**，语义错误。且 `register_system_capabilities` 对它是 no-op，标签只会带来「只停不删」等无关语义 |

**L2 身份须「静态声明 + 动态就绪」两段，不合并**：

| 段 | 载体 | 回答什么 | 时机 |
| --- | --- | --- | --- |
| 静态声明 | `terminal-session` 的 manifest 声明 L2 身份 | 「谁该先加载」→ 步骤 4 第二批 | 加载期可判定 |
| 动态就绪 | `auth-center-register`（本专项 §4.1 K1） | 「我已就绪 + 唯一性仲裁」 | activate 内 |

合并成「只有静态声明」则没有唯一性仲裁（第二个声明者无人拒绝）；
合并成「只有动态注册」则加载顺序退化成「L2 与其他应用同批、靠时序巧合抢先」。

**K9 的直接推论（必须同时做，否则 K9 只做了一半）**：

1. `boot.rs` 现有语义是「**单个组件失败不阻断其余**」——认证中心激活失败时，
   其他应用照常激活但**认证面全拒**（K3 fail-closed）。这在安全上正确，但**必须有
   可见信号**（core-monitor 计数 + 前端一次性提示 + 日志 `deny_kind=no_center`），
   否则用户看到的是「所有东西都连不上」而无从下手（§14 F2）。
2. L2 是宿主**唯一反向依赖**的类别（宿主主动调它），三条红线
   （白名单式登记 / 只做安全闸门 / 只转发不解释）见 ADR 0032，**须落成代码与防回接锁**。
3. `System` 角色的「只停不删」**名实不符**（§12.2 末行）。**不得让文档承诺悬空**
   （§0 文档字面 ≠ 事实）——要么补 `uninstall_plugin` 的 kind 拒绝分支，要么修正文档口径。

## 13. 为什么这里没有也不需要服务发现（用户裁定，技术论证）

用户裁定：**「认证中心 和微服务中的认证中心功能 概念一致，只不过这里没有也不需要服务发现。」**

微服务需要 etcd/consul 的服务发现，是因为中心与调用方**跨进程、跨主机、地址会变**
（容器 IP 漂移、扩缩容、滚动发布）。BedCode 的认证中心与宿主网关的关系不具备其中任何一条：

| 服务发现要解决的问题 | 本仓是否成立 | 依据 |
| --- | --- | --- |
| 中心地址会变（容器 IP 漂移） | **否** | 中心是同进程 wasm 组件，宿主经插件 id + 实例表触达，**地址是进程内指针不是网络地址** |
| 中心有多个副本要选主/负载均衡 | **否** | D2 单中心，且插件实例表天然单例（同 plugin_id 一个实例） |
| 中心会扩缩容 | **否** | 随插件激活/停用，生命周期是**插件闸门**（`approval_gate` + 停用回收），不是副本数 |
| 中心可能健康但地址不可达 | **否** | 同进程，无网络分区；「不可达」等价于实例缺失/trap，已由 `call_guest` 的外层 `Err` 表达 |
| 调用方需要主动拉取端点列表 | **否** | 宿主**就是**网关本身，它在进程内直接查注册表 |
| 需要跨主机互认（TLS/mTLS/租约） | **否** | 同进程同地址空间 |

**结论：注册表本身就是服务发现**——只不过发现协议是「插件激活时调用
`auth-center-register` + 宿主唯一性仲裁」这一个函数，而不是一套网络协议。
中心换人（从 terminal-session 换到独立 auth-center app）= 改一个插件 id，
不需要任何发现基础设施。

**反面代价（明确登记）**：没有服务发现 = 没有「中心暂时不可用时换一个健康副本」这种
高可用手段。K3 的 fail-closed 正是这条代价的显式定价：中心不可用就拒绝，
而不是"降级到没有认证"。

## 14. 失败模式与可观测性

| # | 失败模式 | 现状 | 本专项之后 | 观测手段 |
| --- | --- | --- | --- | --- |
| F1 | 中心选错插件 | 全局 401/4001，无提示指向选错 | 不可能发生（唯一性仲裁） | `tracing::warn!(center=…, owner=…)` 注册成功即留痕 |
| F2 | 中心未激活 | **静默放行**（fail-open） | 显性拒绝 + 点名原因 | `deny_kind=no_center` + HTTP 401 / close 4001 |
| F3 | 中心 trap / 超时 | **静默放行** | 显性拒绝 | `deny_kind=unavailable` + 中心 id 与 trap 摘要 |
| F4 | 用户撤销设备 | 拒绝，但原因埋在日志 | 拒绝 + 中心原样 reason | `deny_kind=policy` + reason（可透出到前端） |
| F5 | 旧产物未注册中心 | 表现为 F2，但排障要猜 | 实例化期点名「按 v32 SDK 重建」 | `stale_artifact_rebuild_hint` |
| F6 | 移动端把认证拒绝当网络断连 | 6 Hz 日志 + toast 洪水 | 4001 致命不自杀 + 最小退避 + 同因熔断 | M1/M2 票 |
| F7 | 裁决成为热点 | 每请求一次 guest 调用（同实例串行） | **有意保留**（撤销必须即时生效） | core-monitor 计数：裁决耗时 / 拒绝率 |

**日志字段（AGENTS §8 结构化字段红线，key = %value 形式，禁止拼进消息）**：
`deny_kind`、`center_id`、`center_owner`、`plugin_id`、`device_id`、`request_id`。
**禁止**把 JWT / claims / 凭据任何片段写进日志或错误文本（凭据红线）。

**前端文案**：F2/F3/F5 属宿主/基础设施域 → 走 `frontend.*` 错误码；
F4 属中心业务域 → 中心给 `com.bedcode.terminal-session.*` 业务码（ADR 0030）。
两端都要有对应 i18n key（zh-CN + en 同步）。

## 15. 迁移路径（不让用户被锁在外面）

本专项是**破坏性行为变更**（fail-closed），必须给出明确的升级序列：

| 阶段 | 桌面端 | 中心插件 | 结果 |
| --- | --- | --- | --- |
| 迁移前（今天） | 能力探测 + 取首个 + 两条 fail-open | v31 SDK 产物 | **本事故状态**：中心选错即全局拒绝 |
| 步骤 1（票 02+05） | ABI 32 + 注册表代码就位，但**裁决仍走旧路径** | v31 SDK 产物（不注册） | 行为不变（旧路径还在），为票 03 留回归窗口 |
| 步骤 2（票 03+04） | 裁决切注册表 + fail-closed；旧发现路径删除 | v31 产物**不注册** | 中心未注册 → 全拒绝 → **必须同时**发布 v32 中心产物 |
| 步骤 3 | 同上 | v32 产物（激活即注册） | 恢复正常 |
| 兜底 | `stale_artifact_rebuild_hint` 点名；设置页/日志给「认证中心未注册」显式信号 | — | 排障不靠猜 |

**发布纪律**：步骤 2 与步骤 3 的两个产物（宿主 + 中心插件）**必须同批发布**——
中间态（宿主已切 fail-closed、中心还没注册）会让远程终端完全不可用。
本仓所有插件产物由源码构建（无外部第三方产物），同批发布成本低，
但**发布脚本必须把两个产物绑成一个原子单元**（`scripts/package-plugins.mjs` 口径）。

**降级预案**：若步骤 2/3 之间出问题，回滚宿主到步骤 1 版本即可（裁决走旧路径），
不需要回滚插件产物——这是「步骤 1 保留旧路径」的设计目的。

## 16. Out of Scope

- **不动凭据与密码学**（D3）：secret-store / HS256 签发验签 / P-256 验签 / 链路身份。
- **不动 `/api/auth/*` 端点契约与真源**：配对码 / QR / biometric 的存储与编排仍在
  `com.bedcode.terminal-session` 私有库（`auth_records`），`auth-method-invoke` 是**新增出口**。
- **不引入服务发现协议**（用户裁定）：不做网络拓扑/健康检查/多副本选主。
- **不改移动端 WIT/ABI**。
- 不做认证中心的热切换/多中心按 method 分派（D2 排除）。
