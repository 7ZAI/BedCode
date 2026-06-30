# Actix Web Migration Design

## Overview

Replace the custom `tokio-tungstenite` + `hyper` WebSocket server with Actix Web, providing unified HTTP REST API + WebSocket on a single port. Mobile端 terminal I/O and push events use WebSocket; all other operations use HTTP REST API called directly from the frontend.

## Architecture

### Server (Desktop)

```
actix_web::HttpServer (single port, e.g. 8080)
├── /api/auth/pairing          POST   → AuthController::request_pairing
├── /api/auth/verify           POST   → AuthController::verify_pairing_code
├── /api/auth/qr-connect       POST   → AuthController::qr_connect
├── /api/auth/reauth           POST   → AuthController::reauthenticate
├── /api/sessions              GET    → SessionController::list_sessions
├── /api/sessions/start        POST   → SessionController::start_session
├── /api/sessions/{id}/stop    POST   → SessionController::stop_session
├── /api/sessions/{id}/resize  POST   → SessionController::resize_session
├── /api/sessions/{id}/remove  DELETE → SessionController::remove_session
├── /api/configs               GET    → ConfigController::list_configs
├── /api/quick-actions         GET    → ConfigController::list_quick_actions
├── /api/file-tree             POST   → FileController::get_file_tree
├── /ws/terminal               WS     → TerminalWs (terminal I/O + push events)
└── JwtAuthMiddleware on /api/*
```

### Mobile Client

```
Mobile Frontend (Vue 3)
├── HTTP 直接调用桌面端 API (fetch/axios)
│   ├── /api/auth/*          → 认证
│   ├── /api/sessions/*      → 会话管理
│   ├── /api/configs/*       → 配置查询
│   └── /api/file-tree       → 文件树
└── Tauri Commands (Rust)
    ├── ws_connect(addr, port)         → 建立 WS 连接
    ├── ws_disconnect()                → 断开 WS
    ├── ws_send_input(session_id, data) → 终端输入
    ├── ws_subscribe(session_id)       → 订阅输出
    ├── ws_unsubscribe(session_id)     → 取消订阅
    └── Tauri Events (Rust → Frontend)
        ├── terminal_output       → 终端输出数据
        ├── session_event         → 会话变更通知
        ├── sync_data             → 数据同步推送
        └── ws_status             → 连接状态变化
```

## WebSocket Protocol

### Scope

WebSocket is only used for:
- Terminal I/O (input, output, subscribe, unsubscribe)
- Server push events (session_event, sync_data, server_closed, client_disconnected)
- WS-level authentication (JWT validation after connection)
- Error messages and Ack confirmations

### Messages Removed from WS (Now HTTP)

| Old WS Message | New HTTP Endpoint |
|---|---|
| Auth (RequestPairing/VerifyCode/QrConnect) | POST /api/auth/pairing, /api/auth/verify, /api/auth/qr-connect |
| Auth (Authenticated/reauth) | POST /api/auth/reauth |
| SessionControl (ListSessions) | GET /api/sessions |
| SessionControl (StartSession) | POST /api/sessions/start |
| SessionControl (StopSession) | POST /api/sessions/{id}/stop |
| SessionControl (ResizeSession) | POST /api/sessions/{id}/resize |
| SessionControl (RemoveSession) | DELETE /api/sessions/{id}/remove |
| SessionConfig (ListSessionConfigs) | GET /api/configs |
| SessionConfig (ListQuickActions) | GET /api/quick-actions |

### WS Authentication Flow

1. Mobile frontend authenticates via HTTP `/api/auth/*` → obtains JWT
2. Mobile Rust layer connects to `ws://host:port/ws/terminal` (no token in URL)
3. After WS connection established, client sends auth message with JWT
4. Server validates JWT:
   - Valid → auth success, terminal interaction begins
   - Invalid → server returns error, client can:
     - Re-pair via HTTP `/api/auth/pairing` (enter pairing code)
     - Re-scan QR via HTTP `/api/auth/qr-connect`
     - Obtain new JWT and reconnect WS

### WS Message Format

Retain the existing `Message` enum format (JSON), but only the following variants are valid over WS:

**Client → Server:**
- `Message::Auth { stage: Authenticated, session_token }` — JWT auth after connect
- `Message::Terminal { Input }` — keyboard input to PTY
- `Message::Terminal { Subscribe }` — subscribe to session output
- `Message::Terminal { Unsubscribe }` — unsubscribe from session output

**Server → Client:**
- `Message::Auth { ... }` — auth response (success/failure)
- `Message::Terminal { Output }` — PTY output data
- `Message::Terminal { SubscribeResponse }` — subscribe confirmation
- `Message::Terminal { UnsubscribeResponse }` — unsubscribe confirmation
- `Message::SessionEvent` — session created/stopped/removed notification
- `Message::SyncData` — incremental data push
- `Message::ServerClosed` — server shutdown notification
- `Message::ClientDisconnected` — other client disconnected
- `Message::Error` — error messages
- `Message::Ack` — confirmation for subscribe/unsubscribe operations

## HTTP API Design

### Authentication Endpoints

#### POST /api/auth/pairing
Request pairing, desktop端 returns pairing code.
```json
// Request
{
  "deviceId": "xxx",
  "deviceName": "Pixel 8",
  "fingerprint": "sha256:..."
}
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "pairingCode": "123456",
    "expiresIn": 300
  }
}
```

#### POST /api/auth/verify
Verify pairing code, returns JWT on success.
```json
// Request
{
  "deviceId": "xxx",
  "deviceName": "Pixel 8",
  "fingerprint": "sha256:...",
  "pairingCode": "123456"
}
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "token": "eyJ...",
    "expiresIn": 86400
  }
}
```

#### POST /api/auth/qr-connect
QR code authentication, returns JWT on success.
```json
// Request
{
  "deviceId": "xxx",
  "deviceName": "Pixel 8",
  "fingerprint": "sha256:...",
  "qrToken": "xxx"
}
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "token": "eyJ...",
    "expiresIn": 86400
  }
}
```

#### POST /api/auth/reauth
Re-authenticate with existing JWT session token.
```json
// Request
{
  "deviceId": "xxx",
  "fingerprint": "sha256:...",
  "sessionToken": "eyJ..."
}
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "token": "eyJ...",
    "expiresIn": 86400
  }
}
```

### Session Endpoints

#### GET /api/sessions
List active sessions.
```json
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "sessions": [
      { "id": "...", "name": "...", "status": "running", ... }
    ]
  }
}
```

#### POST /api/sessions/start
Start a new session.
```json
// Request
{ "configId": "xxx" }
// Response
{
  "code": 0,
  "message": "ok",
  "data": { "sessionId": "xxx", "status": "running" }
}
```

#### POST /api/sessions/{id}/stop
Stop a running session.
```json
// Response
{ "code": 0, "message": "ok" }
```

#### POST /api/sessions/{id}/resize
Resize terminal.
```json
// Request
{ "cols": 120, "rows": 40 }
// Response
{ "code": 0, "message": "ok" }
```

#### DELETE /api/sessions/{id}/remove
Remove a session.
```json
// Response
{ "code": 0, "message": "ok" }
```

### Config Endpoints

#### GET /api/configs
List session configurations.
```json
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "configs": [...]
  }
}
```

#### GET /api/quick-actions
List quick actions.
```json
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "actions": [...]
  }
}
```

### File Endpoint

#### POST /api/file-tree
Get file tree for a session.
```json
// Request
{ "sessionId": "xxx", "excludeDirs": ["node_modules", ".git"] }
// Response
{
  "code": 0,
  "message": "ok",
  "data": {
    "tree": [...]
  }
}
```

## Error Handling

### HTTP API Errors

Standard `ApiResponse<T>` format with HTTP status codes:
- `200 OK + code=0` → Success
- `200 OK + code!=0` → Business error (e.g., code=1001 auth failed, code=1002 session not found)
- `401 Unauthorized` → JWT invalid/expired
- `404 Not Found` → Route or resource not found
- `500 Internal Error` → Server exception

### WebSocket Errors

Retain existing `Message::Error` format for WS auth failure and similar scenarios.

### Mobile Frontend

HTTP errors handled through `useErrorHandler` composable with unified toast/notification display.

## New File Structure

### Desktop Server (New)

```
desktop/server/
├── app.rs              # Actix Web App configuration (routes, middleware)
├── controllers/        # HTTP REST controllers
│   ├── auth_controller.rs
│   ├── session_controller.rs
│   ├── config_controller.rs
│   └── file_controller.rs
├── ws/
│   ├── terminal_ws.rs  # WebSocket actor for terminal I/O
│   └── session.rs      # WS session state management
├── middleware/
│   ├── jwt_auth.rs     # JWT authentication middleware
│   └── cors.rs         # CORS middleware
├── services/           # Business logic (unchanged, shared with controllers)
│   ├── auth_service.rs
│   ├── pairing_service.rs
│   ├── session_config.rs
│   ├── session_control.rs
│   ├── session_sub.rs
│   └── terminal_service.rs
├── dtos/               # Request/Response DTOs for HTTP API
│   ├── auth_dto.rs
│   ├── session_dto.rs
│   ├── config_dto.rs
│   └── common.rs       # ApiResponse, error codes
└── message.rs          # WS Message type (simplified for terminal only)
```

### Mobile Client (New)

```
mobile/
├── remote/
│   ├── connection.rs   # Simplified: WS connect/disconnect + event forwarding
│   ├── ws_client.rs    # Lightweight WS client (tokio-tungstenite) for terminal only
│   └── output_receiver.rs  # Output event → Tauri event forwarding
├── commands/           # Tauri commands (simplified)
│   ├── connection.rs   # ws_connect, ws_disconnect
│   ├── terminal.rs     # ws_send_input, ws_subscribe, ws_unsubscribe
│   └── auth.rs         # ws_auth (JWT auth message after WS connect)
└── ...                 # Other mobile modules (unchanged)
```

### Frontend (New)

```
src/modules/mobile/
├── composables/
│   ├── useHttpApi.ts   # HTTP API client (fetch wrapper with JWT injection)
│   ├── useMobileConnection.ts  # Simplified: WS connection management only
│   └── ...
└── ...
```

### Shared (Modified)

```
shared/
├── model/
│   └── message.rs      # Retain but document WS-only variants
├── auth/               # Unchanged
├── db/                 # Unchanged
├── enums/              # Unchanged
└── websocket/          # Eventually deleted (after migration complete)
```

## Migration Plan (Gradual)

### Phase 1: Add Actix Web + HTTP Controllers
- Add `actix-web`, `actix-web-actors`, `actix-rt` to Cargo.toml
- Create `desktop/server/controllers/` with all HTTP endpoints
- Create `desktop/server/dtos/` for request/response types
- Create `desktop/server/middleware/` for JWT auth + CORS
- Create `desktop/server/app.rs` for Actix Web App config
- Run Actix Web on a separate port initially (e.g., 8081)
- Test all HTTP endpoints independently

### Phase 2: Mobile Frontend HTTP API
- Create `useHttpApi.ts` composable with JWT token management
- Replace WS-based session/config/auth calls with HTTP calls
- Keep WS for terminal I/O
- Test mobile frontend with both old WS + new HTTP

### Phase 3: Migrate WS to Actix Web
- Create `desktop/server/ws/terminal_ws.rs` (Actix WS actor)
- Move WS to same port as HTTP (`/ws/terminal`)
- Update mobile Rust WS client to connect to `/ws/terminal`
- Test WS terminal I/O through Actix Web

### Phase 4: Cleanup Mobile Rust
- Delete `shared/websocket/client/` (old WsClient and submodules)
- Delete `mobile/remote/request.rs` (WS message builders)
- Delete `mobile/router/` (ClientBusinessRouter)
- Delete `mobile/handler/` (old WS handlers)
- Simplify `mobile/remote/connection.rs` to WS-only management
- Remove HTTP-related Tauri commands from mobile

### Phase 5: Remove Old Dependencies
- Delete `shared/websocket/server/` (old WsServer infrastructure)
- Delete `desktop/server/router/` (old BusinessRouter, middleware, registry)
- Delete old `desktop/server/handlers/` (replaced by controllers)
- Delete old `desktop/events/ws_server_handler.rs`
- Remove `tokio-tungstenite`, `hyper`, `hyper-util`, `http-body-util` from desktop dependencies
- Keep `tokio-tungstenite` only for mobile WS client
- Update `docs/code-map.md`

## Dependencies

### Add
- `actix-web = "4"` — HTTP server framework
- `actix-web-actors = "4"` — WebSocket actor support
- `actix-rt = "2"` — Actix runtime

### Keep (for mobile WS client)
- `tokio-tungstenite = "0.24"` — mobile WS client only
- `reqwest = "0.12"` — mobile HTTP client (used by Rust layer for any remaining needs)

### Remove (after migration complete)
- `hyper = "1"` — replaced by Actix Web
- `hyper-util = "0.1"` — replaced by Actix Web
- `http-body-util = "0.1"` — replaced by Actix Web
- `futures-util = "0.3"` — may keep if still used elsewhere

## Key Design Decisions

1. **Single port**: HTTP + WS on same port via Actix Web routing — simpler for mobile discovery and firewall
2. **JWT via WS message**: Not URL query parameter — avoids token leaking in logs/URLs, supports re-auth flow
3. **Frontend direct HTTP**: Mobile frontend calls HTTP API directly (no Rust Tauri command layer for HTTP) — simpler, fewer layers
4. **Keep tokio-tungstenite for mobile WS client**: Actix WS client requires Actix runtime which conflicts with Tauri's Tokio runtime; tokio-tungstenite works natively with Tokio
5. **Gradual migration**: Old and new server coexist during transition, reducing risk
6. **Reuse existing services**: Auth, session, terminal services are preserved — only the transport layer changes
7. **WS URL change**: Mobile WS client connects to `ws://host:port/ws/terminal` instead of `ws://host:port/` — this requires updating WsClientConfig to include the path
