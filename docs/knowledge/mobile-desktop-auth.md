# 移动端连接桌面端 — 认证机制

> **⚠️ 时效提示（2026-09-10 更新）**：本文主干（设备发现 / WS 连接 / 三种认证方式 / 重连机制）仍然有效；但自 2026-09-10 起，移动端认证请求体新增 `uidHash` 字段（设备唯一 ID 哈希，跨卸载重装稳定），桌面端据其合并同一设备的重复配对记录——本文字段表未覆盖。认证明细以 `bedcode-mobile/src-tauri/src/auth/http.rs`（DeviceAuthContext）与桌面端 `server/` DTO 为准。

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
│             │ ◄─── JWT Token ─────────  │  JwtService      │
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
| Desktop | `auth_service` | 处理 Auth 消息（配对码验证 / QR 认证 / JWT 重认证） |
| Desktop | `PairingService` | 配对码生成、验证、消耗 |
| Desktop | `QrTokenManager` | QR Token 生成、验证、一次性消耗 |
| Desktop | `JwtService` | JWT Token 生成与验证（HS256，7 天有效期） |
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

### 3.2 WebSocket 连接

探测通过后，建立 WebSocket 连接：

1. 前端调用 `invoke('ws_connect', { address, port, name })`
2. Rust 端 `ConnectionManager::connect()` 创建 `WsClient`
3. `WsClient` 配置:
   - 路径: `/ws/terminal`
   - 连接超时: 10 秒
   - 心跳间隔: 30 秒
4. 连接成功后状态: `Connected`（WebSocket 已建立，但**未认证**）

**重要**: `Connected` 仅表示 WebSocket 握手完成，此时还不能发送业务消息。必须完成认证后状态才变为 `Paired`。

源码: `bedcode-mobile/src-tauri/src/connection/manager.rs`、`bedcode-mobile/src-tauri/src/connection/ws_connection.rs`

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

源码: `bedcode-desktop/src-tauri/src/enums/auth.rs`、`bedcode-mobile/src-tauri/src/enums/auth.rs`

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

源码: `bedcode-desktop/src-tauri/src/utils/auth/pairing.rs`、`bedcode-desktop/src-tauri/src/server/services/pairing_service.rs`

### 5.3 桌面端处理

1. 收到 `RequestPairing` → 调用 `PairingService::generate_code()` 生成新配对码
2. 发射 `pairing-code-generated` 事件到桌面前端，显示配对码
3. 回复 `VerifyCode` 阶段消息，通知移动端等待输入
4. 收到 `VerifyCode` + 配对码 → 调用 `PairingService::verify_and_consume_code()`
5. 验证通过 → 生成 JWT Token，记录配对到数据库，回复 `Authenticated`

源码: `bedcode-desktop/src-tauri/src/server/services/auth_service.rs`

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
- **退避策略**: 指数退避
- **手动断开检测**: `manual_disconnect` 标记，用户主动断开时不触发重连
- **重连流程**:
  1. 断开旧客户端
  2. 创建新 `WsClient`
  3. 连接成功 → 尝试 JWT 重认证
  4. JWT 认证失败 → 清除凭据，需用户重新配对

源码: `bedcode-mobile/src-tauri/src/connection/manager.rs` → `reconnect()`

---

## HTTP API 认证端点

除 WebSocket 认证外，桌面端还提供 HTTP API 认证端点（用于无 WebSocket 场景）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/auth/verify` | POST | 验证配对码，成功返回 JWT Token |
| `/api/auth/qr-connect` | POST | QR Token 认证，成功返回 JWT Token |
| `/api/auth/reauth` | POST | JWT 重认证 |
| `/api/health` | GET | 健康检查（连接探测用） |

源码: `bedcode-desktop/src-tauri/src/server/controllers/auth_controller.rs`

---

## 关键源码索引

### 桌面端（Desktop）

| 文件 | 职责 |
|------|------|
| `src-tauri/src/utils/auth/jwt.rs` | JWT 生成/验证（HS256，7 天有效期） |
| `src-tauri/src/utils/auth/pairing.rs` | 配对码数据结构（6 位数字，60 秒有效期） |
| `src-tauri/src/utils/auth/qr_token.rs` | QR Token 管理（128-bit hex，一次性） |
| `src-tauri/src/server/services/auth_service.rs` | WS 认证消息处理（核心路由） |
| `src-tauri/src/server/services/pairing_service.rs` | 配对码业务逻辑 |
| `src-tauri/src/server/controllers/auth_controller.rs` | HTTP 认证 API |
| `src-tauri/src/server/ws/terminal_ws.rs` | WS Actor，消息分发 |
| `src-tauri/src/server/ws/websocket_manager.rs` | WS 连接管理器（单例） |
| `src-tauri/src/mdns/advertiser.rs` | mDNS 服务广播 |
| `src-tauri/src/enums/auth.rs` | AuthStage / AuthPayload 定义 |

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
