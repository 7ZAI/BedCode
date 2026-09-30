# 移动端连接桌面端 — 认证机制

> **⚠️ 时效提示（2026-09-10 更新）**：本文主干（设备发现 / WS 连接 / 三种认证方式 / 重连机制）仍然有效；但自 2026-09-10 起，移动端认证请求体新增 `uidHash` 字段（设备唯一 ID 哈希，跨卸载重装稳定），桌面端据其合并同一设备的重复配对记录——本文字段表未覆盖。认证明细以 `bedcode-mobile/src-tauri/src/auth/http.rs`（DeviceAuthContext）与桌面端 `server/http/dtos/` 为准。

本文档描述移动端（Mobile）连接桌面端（Desktop）的完整链路，包括设备发现、WebSocket 连接建立、三种认证方式（配对码 / QR 码 / JWT 重认证）以及断线重连机制。

---

## 1. 整体架构概览

```
┌─────────────┐         mDNS 发现          ┌─────────────────┐
│   Mobile    │ ◄──────────────────────► │    Desktop       │
│  (Client)   │    _bedcode._tcp.local.   │    (Server)      │
│             │                            │                  │
│  mDNS       │                            │  mDNS            │
│  Discovery  │                            │  Advertiser      │
│             │                            │                  │
│  HTTP Probe │ ──── GET /api/health ───► │  Actix HTTP      │
│             │ ◄─── 200 OK ────────────  │                  │
│             │                            │                  │
│  WsClient   │ ──── WS /ws/terminal ───► │  TerminalWs      │
│             │ ◄─── WS Messages ───────  │  (Actor)         │
│             │                            │                  │
│  AuthManager│ ──── Auth Messages ─────► │  auth_service    │
│             │ ◄─── JWT Token ─────────  │  认证中心插件    │
│             │                            │  PairingService  │
│             │                            │  QrTokenManager  │
└─────────────┘                            └─────────────────┘
```

**关键组件：**

| 端 | 组件 | 职责 |
|---|---|---|
| Desktop | `MdnsAdvertiser` | 广播 `_bedcode._tcp.local.` 服务，含端口和设备名 |
| Desktop | `Actix Web Server` | HTTP API + WebSocket 统一端口（默认 8765） |
| Desktop | `TerminalWs` (Actor) | WebSocket 连接管理，消息路由 |
| Desktop | `auth_service` | 处理 Auth 消息（配对码验证 / QR 认证 / JWT 重认证）——**在认证中心插件 `com.bedcode.terminal-session` 内**（`auth_http/`） |
| Desktop | `PairingService` | 配对码生成、验证、消耗 |
| Desktop | `QrTokenManager` | QR Token 生成、验证、一次性消耗 |
| Desktop | `pairing::jwt`（**认证中心插件内**） | JWT 生成与验证（HS256，7 天有效期）+ 入场签发密钥的密钥环。**v33 / ADR 0033 起宿主不再持有任何设备入场密码学**（`utils/auth/jwt.rs` 已整模块退役） |
| Mobile | `MdnsDiscovery` | 扫描局域网 BedCode 服务 |
| Mobile | `ConnectionManager` | WebSocket 连接生命周期管理 |
| Mobile | `AuthManager` | 认证流程编排（配对 / QR / JWT 重认证） |
| Mobile | `useMobileConnection` | 前端连接状态管理与 UI 事件桥接 |

---

## 2. 设备发现（mDNS）

### 2.1 桌面端广播

桌面端启动时通过 `MdnsAdvertiser` 广播 mDNS 服务：

- **服务类型**: `_bedcode._tcp.local.`
- **实例名**: 如 `BedCode-DESKTOP-X1`
- **TXT 记录**: 包含 `platform=desktop`、`device_name=xxx` 等键值对
- **端口**: 从配置读取，默认 `8765`

源码: `bedcode-desktop/src-tauri/src/mdns/advertiser.rs`

### 2.2 移动端发现

移动端通过 `MdnsDiscovery` 扫描局域网：

1. 调用 `invoke('mdns_start_discovery')` 启动扫描
2. 监听 Tauri 事件：
   - `mdns_service_found` — 发现服务（未解析）
   - `mdns_service_resolved` — 解析完成，含 IP/端口/设备名
   - `mdns_service_removed` — 服务消失
3. 解析结果存入 `DiscoveredService`：
   ```typescript
   interface DiscoveredService {
     instance_name: string   // "BedCode-DESKTOP-X1"
     host_name: string       // "DESKTOP-X1.local."
     address: string         // "192.168.1.100"
     port: number            // 8765
     platform: string        // "desktop"
     device_name: string     // 用户可读设备名
   }
   ```

源码: `bedcode-mobile/src/composables/useMdnsDiscovery.ts`、`bedcode-mobile/src-tauri/src/mdns/discovery.rs`

---

## 3. 连接建立

### 3.1 HTTP 探测

WebSocket 连接前，移动端先通过 HTTP 探测桌面端可达性：

```
GET http://{address}:{port}/api/health
```

- 超时: 3 秒
- 成功返回: `{ status, port, uptime_secs }`
- 失败则立即报错，不等待 10 秒 WS 超时

源码: `bedcode-mobile/src/composables/useHttpApi.ts` → `httpProbe()`

### 3.2 WebSocket 连接（WS 面硬切后，插件端点双通道）

> 2026-09-26 起移动端 WS 面整体对齐桌面插件端点。旧 `/ws/event` 与 `/ws/terminal/session/{id}`
> 路径在桌面已 404；WS 只承载两条**插件端点**连接，帧永不加解密（桌面
> `TrafficChannel::WsPlugin => false`——**WS 帧级链路加密已退役**，仅 HTTP 信封加密保留）。

探测通过并完成 HTTP 认证后（§4），Rust 侧经两条 WS 端点与桌面交换数据：

1. **事件通道（常驻，信号面）**——`/ws/plugin/com.bedcode.terminal-session/session-control`：
   - 首帧极简认证 `{"type":"auth","token":"<jwt>"}`（**不是** `Message::Auth` 信封：无加密提案、
     不等待回执；认证失败由宿主 close 4001 显性表达）
   - 入站只有事件帧 `{"type":"event","event":"<name>","payload":{...}}` → `MobileEvent` →
     前端 `ws_sync_*`（session:created / stopped / removed、task:status-changed / queue-changed /
     scheduled-changed、session:mode-changed）
   - 事件**不重放**：连接建立/自愈后发射 `ws_event_channel_ready`，前端触发 HTTP 对账
     （`loadActiveSessions` + 活动会话任务队列按需拉取）补齐重连期间缺口
   - 意外断开按退避经 HTTP reauth 自愈后重建（`connection/event_ws.rs` 常驻监督）
2. **终端流（终端页，按需）**——`/ws/plugin/com.bedcode.terminal-session/terminal`：新协议
   （订阅回放 + 裸字节 + 本地计数 + `ring_resync` 重锚 + `session_stopped`），详情见
   `bedcode-mobile/docs/code-map.md`「终端链路」

认证本身走 HTTP `/api/auth/*`（§4），WS 不再承载认证业务；会话控制 / 会话与配置加载 / 终端输入
也全部走 HTTP（`/api/sessions/*`、`/api/configs`），调用口径见 `bedcode-mobile/docs/code-map.md`
与根 `AGENTS.md` §9 协议节。

源码: `bedcode-mobile/src-tauri/src/connection/{manager,event_ws}.rs`（事件通道）、
`bedcode-mobile/src-tauri/src/terminal_link.rs`（终端流）

---

## 4. 认证机制

移动端支持三种认证方式，按优先级自动选择：

```
┌─────────────────────────────────────────────────────┐
│  已有 JWT Token？                                    │
│  ├─ 是 → JWT 重认证（Reauthenticate）               │
│  │       ├─ 成功 → Paired ✓                        │
│  │       └─ 失败 → 清除凭据，进入配对流程           │
│  └─ 否 → 用户选择：                                 │
│          ├─ 扫描 QR 码 → QR 认证（QrConnect）       │
│          └─ 输入配对码 → 配对码认证（VerifyCode）    │
└─────────────────────────────────────────────────────┘
```

### 4.0 桌面端裁决面：认证中心 fail-closed（ADR 0031，2026-09-29）

移动端发什么不影响**能不能连上**——能不能连上由桌面端的裁决面单方面决定，且现在是
**fail-closed**（这是本节存在的理由：排障时「移动端一切正常但连不上」的根因往往在桌面端）：

```text
请求（HTTP /api/* 或 WS 插件端点首消息）
  → 查认证中心注册表（单中心，O(1)）
      ├─ 无中心在册      → 拒绝  deny_kind=no_center     「no auth center registered」
      ├─ 中心调用失败    → 拒绝  deny_kind=unavailable   「auth center unavailable: …」
      └─ 中心内部：先验签（HS256，密钥环）→ 再逐条做策略
            ├─ 签名无效 / 过期 / 结构非法 / 已撤销 → 拒绝 deny_kind=policy
            └─ 全部通过 → 放行，并交回连接身份（deviceId / deviceName / fingerprint）
```

- **验签与裁决收为同一次调用**（v33 / ADR 0033）：迁移前是「宿主先验签 → 再问中心策略」
  两步，如今中心在 `policy::evaluate` 内部先做密码学验签（密钥来自中心自持的密钥环）
  再逐条做策略，宿主只拿一份裁决结果 + 连接身份。**对称密码学下验签方必须持密钥**，
  所以「验签执行点留宿主」与「密钥归中心」二者只能留其一（ADR 0033 §信任模型论证：
  中心本来就有为任意设备签发凭证的权力，交出密钥**不增加**实际授权面）。
- **token wire 格式逐字节未变**（HS256 + 三段 base64url + 既有 claims 形状），
  `kid` 是**可选** claim 且声明在末尾 ⇒ 移动端把 token 当不透明串，**零改动**。

- **「谁是认证中心」是注册事实，不是猜测**：中心插件激活时调
  `host-auth.auth-center-register` 登记（第二注册者被拒并点名在册属主），停用时注销 /
  宿主回收。旧的「能力探测 + 按 id 排序取首个」已退役——它在候选 > 1 时会选中未实现
  策略的插件（2026-09-29 事故：600+ 次 4001 / 98 秒的拒绝洪水，根因即此）。
- **没有 fail-open 降级**：「查不到中心就放行」「调用失败就放行」两条已删除。
  代价是认证中心未激活时本机全部需认证面不可用——**这是有意的**（认证面失效时放行
  等于无认证裸奔）。真因此状时桌面端日志会出现点名
  「L2 插件激活完成但未注册为认证中心」的 `error` 行。
- **对移动端的可观测后果**：以上三类拒绝对 WS 一律是 `close 4001`，对 HTTP 是 401。
  移动端把 **4001 / 4003 判为致命**（见「重连机制」节），**不自愈**——重连不可能成功，
  反复重试只会刷日志。界面提示「需重新配对」。

源码: `bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs`（裁决面）、
`bedcode-desktop/src-tauri/src/wasm_core/host_api/auth_center.rs`（注册表）

### 4.1 AuthStage 枚举

所有认证消息通过 `AuthStage` 区分阶段：

```rust
enum AuthStage {
    RequestPairing,    // 移动端 → 桌面端：请求配对
    VerifyCode,        // 移动端 → 桌面端：提交配对码
    QrConnect,         // 移动端 → 桌面端：QR Token 认证
    Reauthenticate,    // 移动端 → 桌面端：JWT 重认证
    Authenticated,     // 桌面端 → 移动端：认证成功
    Failed,            // 桌面端 → 移动端：认证失败
    QrFailed,          // 桌面端 → 移动端：QR 认证失败
}
```

源码: `bedcode-mobile/src-tauri/src/enums/auth.rs`（桌面端副本已随认证编排下沉
session 插件删除，2026-09-25——宿主 WS 面不再解析 AuthStage/AuthPayload，只认
`{"type":"auth","token":"<jwt>"}` 极简帧，见 `server/websocket/channel/plugin.rs`）

### 4.2 AuthPayload 结构

```rust
struct AuthPayload {
    stage: AuthStage,
    device_id: Option<String>,           // 设备 ID
    device_name: Option<String>,         // 设备名称
    device_fingerprint: Option<String>,  // 设备指纹
    pairing_code: Option<String>,        // 配对码（VerifyCode 阶段）
    session_token: Option<String>,       // JWT Token（Reauthenticate 阶段）
    qr_token: Option<String>,            // QR Token（QrConnect 阶段）
    error: Option<String>,               // 错误信息（Failed 阶段）
}
```

---

## 5. 配对码认证

### 5.1 流程时序

```
Mobile                              Desktop
  │                                    │
  │  1. Auth { RequestPairing }        │
  │ ──────────────────────────────────► │
  │                                    │  2. 生成 6 位配对码
  │                                    │     发射 Tauri 事件
  │                                    │     "pairing-code-generated"
  │  3. Auth { VerifyCode }            │
  │ ◄────────────────────────────────── │
  │                                    │
  │  4. 用户在移动端输入配对码          │
  │                                    │
  │  5. Auth { VerifyCode, code }      │
  │ ──────────────────────────────────► │
  │                                    │  6. 验证配对码
  │                                    │     生成 JWT Token
  │                                    │     记录配对到 DB
  │                                    │     发射 "device-connected"
  │  7. Auth { Authenticated,          │
  │          session_token }           │
  │ ◄────────────────────────────────── │
  │                                    │
  │  8. 保存凭据，状态 → Paired ✓     │
```

### 5.2 配对码规则

- **格式**: 6 位随机数字（0-9）
- **有效期**: 60 秒（`PAIRING_CODE_TTL_SECS`）
- **一次性**: 验证成功后立即消耗，不可复用
- **每次请求生成新码**: 不复用现有配对码，确保用户有足够时间输入

源码: `bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/`（配对码生成 / 验证编排已随认证记录下沉认证中心插件；宿主 `utils/auth/pairing.rs` 已删）

### 5.3 桌面端处理

1. 收到 `RequestPairing` → 调用 `PairingService::generate_code()` 生成新配对码
2. 发射 `pairing-code-generated` 事件到桌面前端，显示配对码
3. 回复 `VerifyCode` 阶段消息，通知移动端等待输入
4. 收到 `VerifyCode` + 配对码 → 调用 `PairingService::verify_and_consume_code()`
5. 验证通过 → 生成 JWT Token，记录配对到数据库，回复 `Authenticated`

源码: `bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/`（`RequestPairing` → 配对码编排在认证中心插件；宿主侧 WS 首消息认证窗口在 `bedcode-desktop/src-tauri/src/server/websocket/conn.rs`，`PairingService` 已退役）

### 5.4 移动端处理

1. 调用 `invoke('ws_request_pairing')` → `AuthManager::request_pairing()`
2. 发送 `RequestPairing` 消息，等待响应（30 秒超时）
3. 收到 `VerifyCode` 响应 → 状态变为 `WaitingPairingCode`
4. 用户输入配对码 → 调用 `invoke('ws_verify_pairing_code', { code })`
5. 收到 `Authenticated` → 提取 `session_token`，保存凭据，状态变为 `Authenticated`

源码: `bedcode-mobile/src-tauri/src/auth/manager.rs`、`bedcode-mobile/src-tauri/src/commands/auth.rs`

---

## 6. QR 码认证

### 6.1 流程时序

```
Mobile                              Desktop
  │                                    │
  │                                    │  1. 桌面前端生成 QR Token
  │                                    │     调用 QrTokenManager::generate()
  │                                    │     显示二维码（含 token + 地址）
  │                                    │
  │  2. 扫描 QR 码，提取 token         │
  │                                    │
  │  3. Auth { QrConnect, qr_token }   │
  │ ──────────────────────────────────► │
  │                                    │  4. 验证 QR Token
  │                                    │     消耗 Token（一次性）
  │                                    │     生成 JWT Token
  │                                    │     记录配对到 DB
  │                                    │     发射 "qr-token-consumed"
  │                                    │     发射 "device-connected"
  │  5. Auth { Authenticated,          │
  │          session_token }           │
  │ ◄────────────────────────────────── │
  │                                    │
  │  6. 保存凭据，状态 → Paired ✓     │
```

### 6.2 QR Token 规则

- **格式**: 128-bit 随机 hex 字符串（32 字符）
- **有效期**: 可配置 TTL（由桌面前端生成时指定）
- **一次性**: 验证成功后立即消耗并清除，桌面前端需重新生成二维码
- **验证失败场景**:
  - `QR token expired` — 二维码已过期
  - `QR token already used` — 二维码已绑定其他设备
  - `No active QR token` — 桌面端未生成二维码
  - `Invalid QR token` — Token 不匹配

源码: `bedcode-desktop/src-tauri/src/utils/auth/qr_token.rs`

### 6.3 移动端处理

1. 扫描 QR 码获取 token
2. 调用 `invoke('ws_authenticate_with_qr', { token })` → `AuthManager::authenticate_with_qr()`
3. 发送 `QrConnect` 消息，等待响应
4. 收到 `Authenticated` → 保存凭据，状态变为 `Authenticated`
5. 收到 `QrFailed` → 显示错误信息

源码: `bedcode-mobile/src-tauri/src/auth/manager.rs`

---

## 7. JWT 重认证（断线重连）

### 7.1 流程时序

```
Mobile                              Desktop
  │                                    │
  │  1. WebSocket 重连成功             │
  │     状态: Connected（未认证）      │
  │                                    │
  │  2. Auth { Reauthenticate,         │
  │          session_token }           │
  │ ──────────────────────────────────► │
  │                                    │  3. 验证 JWT Token
  │                                    │     检查签名 + 过期时间
  │                                    │     更新 DB last_seen
  │                                    │     发射 "device-connected"
  │  4. Auth { Authenticated }         │
  │ ◄────────────────────────────────── │
  │                                    │
  │  5. 状态 → Paired ✓               │
```

### 7.2 JWT Token 规则

- **算法**: HS256
- **密钥**: 硬编码 `BedCode_Secure_JWT_Key_2024_Change_In_Production`
- **有效期**: 7 天（`DEFAULT_TOKEN_EXPIRY_SECS = 604800`）
- **Claims 结构**:

```rust
struct JwtClaims {
    sub: String,                // 设备 ID
    iss: String,                // 签发者 "BedCode"
    iat: u64,                   // 签发时间（Unix 时间戳）
    exp: u64,                   // 过期时间（Unix 时间戳）
    device_name: Option<String>,  // 设备名称
    fingerprint: Option<String>,  // 设备指纹
}
```

源码: `bedcode-desktop/src-tauri/src/utils/auth/jwt.rs`

### 7.3 凭据持久化

移动端认证成功后，凭据同时保存在两个位置：

1. **Rust 端**: `AuthManager::credentials`（内存，`AuthCredentials` 结构）
2. **前端**: `localStorage`（持久化，跨重启保留）

```typescript
interface AuthCredentials {
  pairing_id: string       // 配对 ID（= device_id）
  fingerprint: string      // 设备指纹
  session_token: string    // JWT Token
}
```

localStorage 键:
- `auth_session_token` — JWT Token
- `auth_pairing_id` — 配对 ID
- `auth_fingerprint` — 设备指纹

### 7.4 重认证失败处理

| 场景 | 行为 |
|------|------|
| JWT 过期/无效 | 清除凭据，需重新配对 |
| 网络错误/超时 | **不删除 token**，下次重连仍可复用 |
| 服务端明确拒绝 | 清除凭据，降级到配对流程 |

源码: `bedcode-mobile/src/composables/useMobileConnection.ts` → `authenticate()`

---

## 8. 设备身份

移动端设备身份持久化在 `device_identity.json`（App 数据目录），确保重启后身份一致：

```rust
struct DeviceIdentity {
    device_id: String,      // UUID v4，首次生成后持久化
    fingerprint: String,    // UUID v4，首次生成后持久化
}
```

- `device_id`: 用于 JWT `sub` 字段，标识设备
- `fingerprint`: 用于识别同一设备，配对记录关联

源码: `bedcode-mobile/src-tauri/src/auth/manager.rs` → `init_identity()`

---

## 9. 消息 Token 注入

所有业务消息发送时自动注入全局 Token：

```rust
// ConnectionManager::send()
let token = get_global_token();
let message = if !token.is_empty() {
    message.clone().with_token(&token)
} else {
    message.clone()
};
```

桌面端收到消息后通过 `token` 字段验证请求合法性。

源码: `bedcode-mobile/src-tauri/src/connection/manager.rs`

---

## 10. 连接状态机

```
Disconnected ──connect()──► Connecting ──WS握手──► Connected
                                                      │
                                              ┌───────┴───────┐
                                              │               │
                                         认证成功          认证失败
                                              │               │
                                              ▼               ▼
                                           Paired         Disconnected
                                              │          (清除凭据)
                                              │
                                     ┌────────┴────────┐
                                     │                 │
                                 正常断开          意外断开
                                     │                 │
                                     ▼                 ▼
                               Disconnected      自动重连
                               (手动断开)        (最多 3 次)
                                                     │
                                              重连成功 → Connected
                                              → JWT 重认证 → Paired
                                              重连失败 → Disconnected
```

**状态说明：**

| 状态 | 含义 |
|------|------|
| `Disconnected` | 未连接或已断开 |
| `Connecting` | WebSocket 握手中 |
| `Connected` | WebSocket 已建立，**未认证** |
| `Paired` | WebSocket 已建立且认证通过，可发送业务消息 |

---

## 重连机制

- **最大重试次数**: 3 次
- **退避策略**: 指数退避，**下限 1000ms**（`MIN_RECONNECT_DELAY_MS`，钳制在
  `calculate_delay` 里而不是只在配置里——纵深防御：任何给出 0 / 负退避的调用方都不会
  打出 6 Hz 风暴）
- **同因熔断**: 连续 5 次**相同原因**的重连失败即放弃（`CIRCUIT_BREAKER_SAME_CAUSE_LIMIT`
  + `same_cause_streak`）。原因变化即重置计数（网络抖动不会把临时故障打成永久放弃）；
  手动 `reset` 同时清掉熔断态
- **认证类关闭码不自愈**（M1，ADR 0031 配套）: `4001`（认证失败）与 `4003`
  （链路完整性失败）属**致命**——`is_auth_fatal_close_code` 判真后
  ① 跳过 supervisor 自愈、② 事件带 `fatal: true`、③ 前端只发**一次** toast
  「需重新配对 / 重新连接」，不走 `handleUnexpectedDisconnect`
  （桌面端此刻已 fail-closed 拒绝，重连不可能成功，见 §4.0）
- **手动断开检测**: `manual_disconnect` 标记，用户主动断开时不触发重连
- **重连流程**:
  1. 断开旧客户端
  2. 创建新 `WsClient`
  3. 连接成功 → 尝试 JWT 重认证
  4. JWT 认证失败 → 清除凭据，需用户重新配对

> 关闭码曾被 `ws_client` 丢弃（只按连接是否 clean 处理），4001 与「网络掉线」不可区分——
> 这正是 2026-09-29 事故里移动端无退避自愈、616 次 / 98 秒刷屏的放大器。
> `ServerClosed { code, reason }` 起保留关闭码（未携带时按 1006 处理）。

源码: `bedcode-mobile/src-tauri/src/connection/manager.rs` → `reconnect()`、
`bedcode-mobile/src-tauri/src/connection/reconnect.rs`（退避 / 熔断）、
`bedcode-mobile/src-tauri/src/system/constants/{connection,reconnect}.rs`（常量与判据）

---

## HTTP API 认证端点

除 WebSocket 认证外，桌面端还提供 HTTP API 认证端点（用于无 WebSocket 场景）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/auth/verify` | POST | 验证配对码，成功返回 JWT Token |
| `/api/auth/qr-connect` | POST | QR Token 认证，成功返回 JWT Token |
| `/api/auth/reauth` | POST | JWT 重认证 |
| `/api/health` | GET | 健康检查（连接探测用） |

源码: `bedcode-desktop/wasm-apps/terminal-session/rust/src/auth_http/`（`/api/auth/*` 端点编排已下沉认证中心插件，宿主不再注册认证业务路由——JWT 之前的入口经网关免验签转发）

> **迁移提醒（2026-09-29，ADR 0033）**：入场签发密钥改由认证中心自持，**存量已配对设备需
> 全量重新配对**（与 v24 退役 `pairings` / `connection_history` / `session_configs`
> 三表的既有口径一致）。移动端侧的表现是 `handle_reauth` 返回「凭证失效」类业务码
> 而非网络错误——文案区分见 ADR 0030 错误码口径。移动端把 token 当**不透明串**
> （只存本地、只往上传，从不自行验签或解析），故 wire 格式变化对移动端零影响。

---

## 关键源码索引

### 桌面端（Desktop）

| 文件 | 职责 |
|------|------|
| `src-tauri/src/utils/auth/auth_center.rs` | 认证中心宿主桥接：裁决面（`enforce_connection_policy`，**只问中心一次**）+ 注册表查询 + 组合式认证窄转发。**宿主无任何设备 JWT 密码学**（`utils/auth/jwt.rs` / `host_secrets.rs` 已随 ADR 0033 整模块删除） |
| `wasm-apps/terminal-session/rust/src/pairing/code.rs` | 配对码生成/验证（6 位数字，60 秒有效期；编排已下沉认证中心插件） |
| `wasm-apps/terminal-session/rust/src/pairing/qr.rs` | QR 配对 Token（一次性；编排已下沉认证中心插件） |
| `src-tauri/src/server/websocket/conn.rs` | WS 连接骨架：首消息认证窗口（JWT 重连 / 配对流程） |
| `wasm-apps/terminal-session/rust/src/pairing/` | 配对码业务逻辑（认证中心插件，宿主 `PairingService` 已退役） |
| `wasm-apps/terminal-session/rust/src/auth_http/` | HTTP 认证 API（认证中心插件，宿主经网关免验签转发） |
| `src-tauri/src/server/websocket/terminal_ws/` + `websocket/conn.rs` | WS 终端输出端子面（control_frame / forward / subscriber）+ 连接骨架 |
| `src-tauri/src/server/websocket/websocket_manager.rs` | WS 连接管理器（单例） |
| `src-tauri/src/mdns/advertiser.rs` | mDNS 服务广播 |
| ~~`src-tauri/src/enums/auth.rs`~~（已删 2026-09-25） | AuthStage / AuthPayload 定义曾在此；wire 真源现仅移动端 |

### 移动端（Mobile）

| 文件 | 职责 |
|------|------|
| `src-tauri/src/auth/manager.rs` | 认证管理器（配对/QR/JWT 重认证编排） |
| `src-tauri/src/auth/pairing.rs` | 配对码数据结构（与桌面端共享） |
| `src-tauri/src/connection/manager.rs` | 连接管理器（WS 连接/断开/重连） |
| `src-tauri/src/connection/request.rs` | 认证消息构建（AuthRequest 工厂方法） |
| `src-tauri/src/connection/ws_connection.rs` | WS 底层连接（握手/超时/状态） |
| `src-tauri/src/commands/auth.rs` | Tauri 认证命令（前端 invoke 入口） |
| `src-tauri/src/mdns/discovery.rs` | mDNS 服务发现 |
| `src-tauri/src/mdns/types.rs` | mDNS 共享类型 |
| `src/composables/useMobileConnection.ts` | 前端连接/认证状态管理 |
| `src/composables/useMdnsDiscovery.ts` | 前端 mDNS 发现 composable |
| `src/composables/useHttpApi.ts` | HTTP API 客户端 + 连接探测 |

---

## 跨端真实互连测试（2026-09-30 起）

本文件描述的每一条链路（配对 / QR / 重认证 / WS 端点认证 / 终端流）都**有跨端
测试守着**——不是「两端各自的 mock 自洽」，而是同一进程内**桌面端真实服务器 +
真实 wasm 认证中心产物**与**移动端真实客户端代码**互连。

```bash
cd cross-end-tests && cargo test
```

工程：`cross-end-tests/`（仓库根，第三个 Rust 包；依赖两端 lib：
`bedcode-desktop-lib` / `bedcode-mobile-lib`）。前置：桌面随包 wasm 产物须先构建
（`cd bedcode-desktop && pnpm run plugins:build`）——认证中心是**真实产物**，
缺失时测试**显性失败**而非跳过。

| 场景文件 | 覆盖链路 |
|---------|---------|
| `harness_selfcheck.rs` | 台子自检：两端接线 + 端点登记 + 停机后不再应答 |
| `pairing_auth_flow.rs` | §5 配对码 / §6 QR / §7 重认证（正例 + 错码 1005 / 篡改 token / QR 一次性 / 未绑定生物 1008） |
| `jwt_rotate_reconnect.rs` | §7 + ADR 0033 密钥环轮换宽限期（**旧 token 轮换后仍可用**，含 WS 面） |
| `session_http_flow.rs` | §HTTP API 会话域：`/api/sessions*` 契约 + 1002 错误信封 + remove 幂等 vs stop/input 严格 |
| `terminal_ws_flow.rs` | §终端流：订阅 → **真实 bash PTY 输出字节到达移动端页面通道** → 终态 `session_stopped` |
| `fail_closed_flow.rs` | ADR 0031 fail-closed：无中心在册 / 伪造凭证一律拒绝（HTTP + WS 两面） |
| `lifecycle_flow.rs` | 桌面停用 / 激活插件对移动端连接的联动影响 |

**覆盖不到的部分（诚实边界）**：

- `deny_kind` 三态（`no_center` / `unavailable` / `policy`）是宿主**日志结构化
  字段**，不是 wire 字段——客户端一律看到 401，这是有意的（不泄露部署信息）。
  三态分类的覆盖在宿主 `utils/auth/auth_center` 单测。
- 生物认证正向路径：移动端私钥在 Android Keystore，无头进程构造不出真设备密钥，
  只覆盖了「未绑定 → 1008」的反例。
- QR 的「桌面扫码确认」UI 步骤：无头装配直接走插件 `qr-code-generate` 互调
  （即桌面 UI 的同一入口）生成 token，跳过扫码动作本身。

每个场景 = 独立测试二进制（进程隔离）：桌面端 `AppContext` 是进程级 `OnceLock`
单例，场景之间无法重装。
