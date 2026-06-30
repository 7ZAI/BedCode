# Actix Web Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace the custom tokio-tungstenite + hyper WebSocket/HTTP server with Actix Web, providing unified HTTP REST API + WebSocket on a single port. Mobile terminal I/O uses WebSocket; all other operations use HTTP REST API called directly from the frontend.

**Architecture:** Actix Web runs a single `HttpServer` on one port. HTTP REST controllers handle auth, sessions, configs, and file-tree. An Actix WebSocket actor at `/ws/terminal` handles terminal I/O and push events. JWT middleware protects `/api/*` routes. Mobile frontend calls HTTP API directly via `fetch()`, bypassing the Rust Tauri command layer for HTTP operations. Only WS connect/disconnect/terminal-input/subscribe go through Tauri commands.

**Tech Stack:** Actix Web 4, actix-web-actors 4, actix-rt 2, tokio-tungstenite 0.24 (mobile WS client only), reqwest 0.12 (mobile HTTP client in Rust layer, minimal use)

---

## Phase 1: Add Actix Web + HTTP Controllers

### Task 1: Add Actix Web Dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [x] **Step 1: Add Actix Web dependencies to Cargo.toml**

Add these under the `[dependencies]` section, after the `tokio` line:

```toml
# Actix Web (desktop server)
actix-web = { version = "4", optional = true }
actix-web-actors = { version = "4", optional = true }
actix-rt = { version = "2", optional = true }
```

Add a feature-based section for desktop-only Actix dependencies:

```toml
[features]
default = []
desktop = ["actix-web", "actix-web-actors", "actix-rt"]
```

Update the desktop-only dependencies section to include the feature:

```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
actix-web = "4"
actix-web-actors = "4"
actix-rt = "2"
portable-pty = "0.8"
keyring = "3"
```

This keeps Actix Web desktop-only — mobile doesn't need it.

- [x] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors (no code changes yet, just dependency addition)

- [x] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore(deps): add Actix Web dependencies for desktop server"
```

---

### Task 2: Create DTOs (Request/Response Types)

**Files:**
- Create: `src-tauri/src/desktop/server/dtos.rs`
- Create: `src-tauri/src/desktop/server/dtos/common.rs`
- Create: `src-tauri/src/desktop/server/dtos/auth_dto.rs`
- Create: `src-tauri/src/desktop/server/dtos/session_dto.rs`
- Create: `src-tauri/src/desktop/server/dtos/config_dto.rs`

- [x] **Step 1: Create `dtos/common.rs` — ApiResponse and error codes**

```rust
//! Common DTOs for HTTP API responses

use serde::{Deserialize, Serialize};

/// HTTP API 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub code: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl ApiResponse<()> {
    pub fn ok() -> Self {
        Self { code: 0, message: "ok".to_string(), data: None }
    }

    pub fn error(code: u16, message: &str) -> Self {
        Self { code, message: message.to_string(), data: None }
    }
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok_with_data(data: T) -> Self {
        ApiResponse { code: 0, message: "ok".to_string(), data: Some(data) }
    }
}

// HTTP API 错误代码
pub const CODE_OK: u16 = 0;
pub const CODE_AUTH_FAILED: u16 = 1001;
pub const CODE_SESSION_NOT_FOUND: u16 = 1002;
pub const CODE_INVALID_REQUEST: u16 = 1003;
pub const CODE_TIMEOUT: u16 = 1004;
pub const CODE_PAIRING_FAILED: u16 = 1005;
pub const CODE_QR_FAILED: u16 = 1006;
```

- [x] **Step 2: Create `dtos/auth_dto.rs` — Auth request/response DTOs**

```rust
//! Auth DTOs

use serde::{Deserialize, Serialize};

/// POST /api/auth/pairing request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
}

/// POST /api/auth/pairing response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResponseData {
    pub pairing_code: String,
    pub expires_in: u64,
}

/// POST /api/auth/verify request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub pairing_code: String,
}

/// Auth token response (shared by verify, qr-connect, reauth)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokenResponseData {
    pub token: String,
    pub expires_in: u64,
}

/// POST /api/auth/qr-connect request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrConnectRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub qr_token: String,
}

/// POST /api/auth/reauth request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReauthRequest {
    pub device_id: String,
    pub fingerprint: String,
    pub session_token: String,
}
```

- [x] **Step 3: Create `dtos/session_dto.rs` — Session request/response DTOs**

```rust
//! Session DTOs

use serde::{Deserialize, Serialize};

/// GET /api/sessions response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponseData {
    pub sessions: Vec<SessionItem>,
}

/// Single session item in list response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub session_type: Option<String>,
    pub config_id: Option<String>,
}

/// POST /api/sessions/start request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionRequest {
    pub config_id: String,
}

/// POST /api/sessions/start response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponseData {
    pub session_id: String,
    pub status: String,
}

/// POST /api/sessions/{id}/resize request
#[derive(Debug, Clone, Deserialize)]
pub struct ResizeSessionRequest {
    pub cols: u16,
    pub rows: u16,
}
```

- [x] **Step 4: Create `dtos/config_dto.rs` — Config request/response DTOs**

```rust
//! Config DTOs

use serde::{Deserialize, Serialize};

/// GET /api/configs response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigListResponseData {
    pub configs: Vec<ConfigItem>,
}

/// Single config item
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigItem {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
}

/// GET /api/quick-actions response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickActionListResponseData {
    pub actions: Vec<QuickActionItem>,
}

/// Quick action item
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickActionItem {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// POST /api/file-tree request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeRequest {
    pub session_id: String,
    pub exclude_dirs: Vec<String>,
}

/// POST /api/file-tree response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeResponseData {
    pub tree: Vec<FileTreeNode>,
}

/// File tree node
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}
```

- [x] **Step 5: Create `dtos.rs` module file**

```rust
//! DTOs — HTTP API Request/Response types

pub mod common;
pub mod auth_dto;
pub mod session_dto;
pub mod config_dto;
```

- [x] **Step 6: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 7: Commit**

```bash
git add src-tauri/src/desktop/server/dtos.rs src-tauri/src/desktop/server/dtos/
git commit -m "feat(server): add HTTP API DTOs for Actix Web migration"
```

---

### Task 3: Create JWT Auth Middleware for Actix Web

**Files:**
- Create: `src-tauri/src/desktop/server/middleware.rs`
- Create: `src-tauri/src/desktop/server/middleware/jwt_auth.rs`
- Create: `src-tauri/src/desktop/server/middleware/cors.rs`

- [x] **Step 1: Create `middleware/jwt_auth.rs`**

```rust
//! JWT Authentication Middleware for Actix Web
//!
//! 从 Authorization header 提取 Bearer token 并验证 JWT
//! 验证通过后将 claims 存入 request extensions

use actix_web::{dev::ServiceRequest, Error, HttpMessage};
use actix_web::error::ErrorUnauthorized;
use crate::shared::auth::jwt::{JwtService, JwtClaims};

/// JWT 认证验证器
///
/// 从 Authorization: Bearer <token> 提取并验证 JWT
/// 验证成功后将 JwtClaims 注入 request extensions
pub fn validate_jwt(req: &ServiceRequest) -> Result<JwtClaims, Error> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ErrorUnauthorized("Missing Authorization header"))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ErrorUnauthorized("Invalid Authorization format, expected Bearer <token>"))?;

    let jwt_service = JwtService::new();
    let claims = jwt_service
        .verify_token_with_expiry(token)
        .map_err(|e| {
            let msg = match e {
                crate::shared::auth::jwt::JwtError::TokenExpired => "Token expired",
                _ => "Invalid token",
            };
            ErrorUnauthorized(msg)
        })?;

    // 将 claims 存入 request extensions，后续 handler 可提取
    req.extensions_mut().insert(claims.clone());

    Ok(claims)
}

/// 从 request extensions 提取 JWT claims
pub fn get_claims_from_request(req: &actix_web::HttpRequest) -> Option<JwtClaims> {
    req.extensions().get::<JwtClaims>().cloned()
}
```

- [x] **Step 2: Create `middleware/cors.rs`**

```rust
//! CORS Middleware for Actix Web

use actix_web::middleware::cors::Cors;
use actix_cors::Cors as ActixCors;

/// 创建 CORS 配置
///
/// 允许所有来源（移动端 IP 不固定），支持认证 header
pub fn cors_config() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .max_age(3600)
}
```

**Note:** We need `actix-cors` crate. Add it to Cargo.toml desktop dependencies. Update Task 1's Cargo.toml changes to also include:

```toml
actix-cors = "0.7"
```

- [x] **Step 3: Create `middleware.rs` module file**

```rust
//! Actix Web Middleware

pub mod jwt_auth;
pub mod cors;
```

- [x] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/desktop/server/middleware.rs src-tauri/src/desktop/server/middleware/ src-tauri/Cargo.toml
git commit -m "feat(server): add JWT auth and CORS middleware for Actix Web"
```

---

### Task 4: Create Auth Controller

**Files:**
- Create: `src-tauri/src/desktop/server/controllers.rs`
- Create: `src-tauri/src/desktop/server/controllers/auth_controller.rs`

- [x] **Step 1: Create `controllers/auth_controller.rs`**

This controller wraps the existing `auth_service::handle_auth` and `handle_jwt_auth` functions, but returns HTTP JSON responses instead of WS Messages.

```rust
//! Auth Controller
//!
//! HTTP REST API endpoints for authentication
//! Routes:
//! - POST /api/auth/pairing
//! - POST /api/auth/verify
//! - POST /api/auth/qr-connect
//! - POST /api/auth/reauth

use actix_web::{web, HttpRequest, HttpResponse};
use crate::desktop::app_context::AppContext;
use crate::desktop::server::dtos::common::ApiResponse;
use crate::desktop::server::dtos::auth_dto::*;
use crate::shared::auth::jwt::JwtService;
use crate::shared::enums::auth::AuthStage;

/// POST /api/auth/pairing
///
/// 请求配对码，桌面端弹出配对码供移动端输入
pub async fn request_pairing(
    req: HttpRequest,
    body: web::Json<PairingRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();
    let app_handle = ctx.app_handle();

    let code = pairing_service.generate_code().await;

    // 通知桌面端前端显示配对码
    if let Err(e) = app_handle.emit("pairing-code-generated", &crate::desktop::server::connection_types::PairingCodeGeneratedEvent {
        code: code.code.clone(),
        expires_in: code.remaining_seconds(),
        device_name: Some(body.device_name.clone()),
    }) {
        tracing::error!("Failed to emit pairing code event: {}", e);
    }

    let data = PairingResponseData {
        pairing_code: code.code,
        expires_in: code.remaining_seconds(),
    };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/auth/verify
///
/// 验证配对码，成功返回 JWT token
pub async fn verify_pairing_code(
    body: web::Json<VerifyPairingRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();

    let is_valid = pairing_service.verify_and_consume_code(&body.pairing_code).await;

    if !is_valid {
        let current_code = pairing_service.get_current_code().await;
        let msg = if current_code.is_none() {
            "No pairing code available. Please generate a new code."
        } else {
            "Invalid or expired pairing code"
        };
        return HttpResponse::Ok().json(ApiResponse::<()>::error(1005, msg));
    }

    let jwt_service = JwtService::new();
    let token = match jwt_service.generate_token(
        body.device_id.clone(),
        Some(body.device_name.clone()),
        Some(body.fingerprint.clone()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT generation failed: {}", e);
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
        }
    };

    let data = AuthTokenResponseData {
        expires_in: crate::shared::auth::jwt::DEFAULT_TOKEN_EXPIRY_SECS,
        token,
    };

    // 通知桌面端有设备连接
    let app_handle = ctx.app_handle();
    let _ = app_handle.emit("device-connected", &crate::desktop::server::connection_types::DeviceConnectionEvent {
        addr: String::new(),
        device_id: body.device_id.clone(),
        device_name: Some(body.device_name.clone()),
        event: "authenticated".to_string(),
    });

    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/auth/qr-connect
///
/// QR 码认证，成功返回 JWT token
pub async fn qr_connect(
    body: web::Json<QrConnectRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let qr_manager = ctx.qr_manager();

    match qr_manager.verify(&body.qr_token).await {
        Ok(()) => {
            // QR token 已消耗，通知桌面端重新生成
            let app_handle = ctx.app_handle();
            let _ = app_handle.emit("qr-token-consumed", ());

            let device_id = body.device_id.clone();
            let device_name = body.device_name.clone();
            let fingerprint = body.fingerprint.clone();

            let jwt_service = JwtService::new();
            let token = match jwt_service.generate_token(
                device_id.clone(),
                Some(device_name.clone()),
                Some(fingerprint.clone()),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("JWT generation failed: {}", e);
                    return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
                }
            };

            // 通知桌面端有设备连接
            let _ = app_handle.emit("device-connected", &crate::desktop::server::connection_types::DeviceConnectionEvent {
                addr: String::new(),
                device_id,
                device_name: Some(device_name),
                event: "authenticated".to_string(),
            });

            let data = AuthTokenResponseData {
                expires_in: crate::shared::auth::jwt::DEFAULT_TOKEN_EXPIRY_SECS,
                token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            let error_msg = e.to_string();
            let user_msg = if error_msg.contains("expired") {
                "二维码已过期，请重新生成"
            } else if error_msg.contains("already used") {
                "二维码已绑定其他设备，请重新扫描"
            } else if error_msg.contains("No active QR token") {
                "请先在桌面端生成二维码"
            } else {
                &error_msg
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1006, user_msg))
        }
    }
}

/// POST /api/auth/reauth
///
/// 使用已有 JWT token 重新认证
pub async fn reauthenticate(
    body: web::Json<ReauthRequest>,
) -> HttpResponse {
    let jwt_service = JwtService::new();

    match jwt_service.verify_token_with_expiry(&body.session_token) {
        Ok(claims) => {
            // 验证成功，生成新 token
            let new_token = match jwt_service.generate_token(
                claims.sub.clone(),
                claims.device_name.clone(),
                claims.fingerprint.clone(),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("JWT generation failed: {}", e);
                    return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
                }
            };

            let data = AuthTokenResponseData {
                expires_in: crate::shared::auth::jwt::DEFAULT_TOKEN_EXPIRY_SECS,
                token: new_token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            let msg = match e {
                crate::shared::auth::jwt::JwtError::TokenExpired => "Token expired",
                _ => "Invalid token",
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1001, msg))
        }
    }
}
```

- [x] **Step 2: Create `controllers.rs` module file**

```rust
//! HTTP REST Controllers

pub mod auth_controller;
```

- [x] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/desktop/server/controllers.rs src-tauri/src/desktop/server/controllers/
git commit -m "feat(server): add AuthController with pairing/verify/qr/reauth endpoints"
```

---

### Task 5: Create Session Controller

**Files:**
- Create: `src-tauri/src/desktop/server/controllers/session_controller.rs`

- [x] **Step 1: Create the session controller**

```rust
//! Session Controller
//!
//! HTTP REST API endpoints for session management
//! Routes:
//! - GET  /api/sessions
//! - POST /api/sessions/start
//! - POST /api/sessions/{id}/stop
//! - POST /api/sessions/{id}/resize
//! - DELETE /api/sessions/{id}/remove

use actix_web::{web, HttpRequest, HttpResponse};
use crate::desktop::app_context::AppContext;
use crate::desktop::server::dtos::common::ApiResponse;
use crate::desktop::server::dtos::session_dto::*;
use crate::desktop::server::middleware::jwt_auth::get_claims_from_request;

/// GET /api/sessions
pub async fn list_sessions(req: HttpRequest) -> HttpResponse {
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();
    let plugin_manager = ctx.plugin_manager();

    let pty_sessions = session_manager.list_sessions().await;
    let plugin_sessions = plugin_manager.list_sessions().await;

    let mut sessions = Vec::new();

    for s in pty_sessions {
        sessions.push(SessionItem {
            id: s.id,
            name: s.name,
            status: serde_json::to_value(&s.status)
                .and_then(|v| serde_json::from_value::<String>(v))
                .unwrap_or_else(|_| format!("{:?}", s.status)),
            created_at: s.created_at.to_rfc3339(),
            started_at: s.started_at.map(|t| t.to_rfc3339()),
            session_type: Some("pty".to_string()),
            config_id: Some(s.config_id),
        });
    }

    for s in plugin_sessions {
        sessions.push(SessionItem {
            id: s.id,
            name: s.name,
            status: serde_json::to_value(&s.status)
                .and_then(|v| serde_json::from_value::<String>(v))
                .unwrap_or_else(|_| format!("{:?}", s.status)),
            created_at: s.created_at.to_rfc3339(),
            started_at: s.started_at.map(|t| t.to_rfc3339()),
            session_type: Some("plugin".to_string()),
            config_id: Some(s.config_id),
        });
    }

    let data = SessionListResponseData { sessions };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/sessions/start
pub async fn start_session(
    req: HttpRequest,
    body: web::Json<StartSessionRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);

    match session_manager.create_session_with_source(&body.config_id, device_name).await {
        Ok(session_id) => {
            // 通知桌面端刷新会话列表
            let app_handle = ctx.app_handle();
            let source = device_name.unwrap_or_else(|| "mobile".to_string());
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));

            let data = StartSessionResponseData {
                session_id,
                status: "running".to_string(),
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!("Failed to start session: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/stop
pub async fn stop_session(
    req: HttpRequest,
    path: web::Path<String>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);

    match session_manager.kill_session_with_source(&session_id, device_name.clone()).await {
        Ok(()) => {
            let app_handle = ctx.app_handle();
            let source = device_name.unwrap_or_else(|| "mobile".to_string());
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            tracing::error!("Failed to stop session: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/resize
pub async fn resize_session(
    path: web::Path<String>,
    body: web::Json<ResizeSessionRequest>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    if let Err(e) = session_manager.resize_session(&session_id, body.cols, body.rows).await {
        tracing::warn!("Failed to resize session: {}", e);
        return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
    }

    HttpResponse::Ok().json(ApiResponse::ok())
}

/// DELETE /api/sessions/{id}/remove
pub async fn remove_session(
    req: HttpRequest,
    path: web::Path<String>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);

    match session_manager.remove_session_with_source(&session_id, device_name.clone()).await {
        Ok(()) => {
            let app_handle = ctx.app_handle();
            let source = device_name.unwrap_or_else(|| "mobile".to_string());
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            tracing::error!("Failed to remove session: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}
```

- [x] **Step 2: Update `controllers.rs` to include session_controller**

```rust
pub mod auth_controller;
pub mod session_controller;
```

- [x] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/desktop/server/controllers/
git commit -m "feat(server): add SessionController with CRUD endpoints"
```

---

### Task 6: Create Config and File Controllers

**Files:**
- Create: `src-tauri/src/desktop/server/controllers/config_controller.rs`
- Create: `src-tauri/src/desktop/server/controllers/file_controller.rs`

- [x] **Step 1: Create `controllers/config_controller.rs`**

```rust
//! Config Controller
//!
//! Routes:
//! - GET /api/configs
//! - GET /api/quick-actions

use actix_web::{web, HttpResponse};
use crate::desktop::app_context::AppContext;
use crate::desktop::server::dtos::common::ApiResponse;
use crate::desktop::server::dtos::config_dto::*;
use crate::desktop::session::SessionConfigManager;

/// GET /api/configs
pub async fn list_configs() -> HttpResponse {
    let ctx = AppContext::global();
    let manager = SessionConfigManager::new(ctx.db().clone());

    match manager.list_configs().await {
        Ok(configs) => {
            let items: Vec<ConfigItem> = configs.into_iter().map(|c| ConfigItem {
                id: c.id,
                name: c.name,
                environment: c.environment,
                wsl_distro: c.wsl_distro,
                working_dir: c.working_dir,
                command: c.command,
            }).collect();
            let data = ConfigListResponseData { configs: items };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!("Failed to list configs: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
    }
}

/// GET /api/quick-actions
pub async fn list_quick_actions() -> HttpResponse {
    let ctx = AppContext::global();
    let db = ctx.db();
    let db_guard = db.lock().await;

    match db_guard.get_quick_actions() {
        Ok(actions) => {
            let items: Vec<QuickActionItem> = actions.into_iter().map(|a| QuickActionItem {
                id: a.id,
                name: a.name,
                content: a.content,
                icon: a.icon,
                color: a.color,
            }).collect();
            let data = QuickActionListResponseData { actions: items };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!("Failed to list quick actions: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
    }
}
```

- [x] **Step 2: Create `controllers/file_controller.rs`**

Reuse the file tree logic from `handlers/file_tree_handler.rs`, adapted for Actix Web:

```rust
//! File Controller
//!
//! Routes:
//! - POST /api/file-tree

use actix_web::{web, HttpResponse};
use crate::desktop::app_context::AppContext;
use crate::desktop::server::dtos::common::ApiResponse;
use crate::desktop::server::dtos::config_dto::*;
use std::path::PathBuf;

const MAX_DEPTH: usize = 20;

/// POST /api/file-tree
pub async fn get_file_tree(body: web::Json<FileTreeRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    // 根据 session_id 查找 working_dir
    let working_dir = match ctx
        .config_manager()
        .get_config_by_session_id(&body.session_id, ctx.session_manager())
        .await
    {
        Ok(config) => config.working_dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let root = PathBuf::from(&working_dir);
    if !root.is_dir() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(400, &format!("Working dir is not a directory: {}", working_dir)));
    }

    let filters = build_exclude_filters(&body.exclude_dirs);
    let root_clone = root.clone();
    let filters_clone = filters.clone();

    let tree_result = tokio::task::spawn_blocking(move || {
        scan_dir(&root_clone, &root_clone, &filters_clone, 0)
    }).await;

    match tree_result {
        Ok(Ok(tree)) => {
            let data = FileTreeResponseData { tree };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
        Err(e) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &format!("File tree scan failed: {}", e)))
        }
    }
}

// ==================== Filter Logic ====================
#[derive(Clone)]
enum ExcludeFilter {
    Name(String),
    Path { parent: String, name: String },
}

fn build_exclude_filters(exclude_dirs: &[String]) -> Vec<ExcludeFilter> {
    exclude_dirs.iter().map(|pattern| {
        if let Some(slash_pos) = pattern.rfind('/') {
            ExcludeFilter::Path {
                parent: pattern[..slash_pos].to_string(),
                name: pattern[slash_pos + 1..].to_string(),
            }
        } else {
            ExcludeFilter::Name(pattern.clone())
        }
    }).collect()
}

fn should_exclude(relative_path: &str, dir_name: &str, filters: &[ExcludeFilter]) -> bool {
    for f in filters {
        match f {
            ExcludeFilter::Name(name) => { if dir_name == name { return true; } }
            ExcludeFilter::Path { parent, name } => {
                if dir_name == name && relative_path == parent { return true; }
            }
        }
    }
    false
}

fn scan_dir(root: &PathBuf, dir: &PathBuf, filters: &[ExcludeFilter], depth: usize) -> crate::Result<Vec<FileTreeNode>> {
    if depth > MAX_DEPTH { return Ok(Vec::new()); }
    let mut entries: Vec<FileTreeNode> = Vec::new();
    let mut folders: Vec<FileTreeNode> = Vec::new();
    let mut files: Vec<FileTreeNode> = Vec::new();

    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        crate::AppError::Internal(format!("Failed to read dir {}: {}", dir.display(), e))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| crate::AppError::Internal(format!("Failed to read entry: {}", e)))?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') { continue; }
        let file_type = entry.file_type().map_err(|e| crate::AppError::Internal(format!("Failed to get file type: {}", e)))?;

        if file_type.is_dir() {
            let relative = dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().to_string();
            if should_exclude(&relative, &file_name, filters) { continue; }
            let child_dir = dir.join(&file_name);
            let children = scan_dir(root, &child_dir, filters, depth + 1)?;
            folders.push(FileTreeNode {
                name: file_name,
                node_type: "folder".to_string(),
                children: Some(children),
            });
        } else if file_type.is_file() {
            files.push(FileTreeNode {
                name: file_name,
                node_type: "file".to_string(),
                children: None,
            });
        }
    }

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries.extend(folders);
    entries.extend(files);
    Ok(entries)
}
```

- [x] **Step 3: Update `controllers.rs`**

```rust
pub mod auth_controller;
pub mod session_controller;
pub mod config_controller;
pub mod file_controller;
```

- [x] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/desktop/server/controllers/
git commit -m "feat(server): add ConfigController and FileController endpoints"
```

---

### Task 7: Create Actix Web App Configuration and Server Startup

**Files:**
- Create: `src-tauri/src/desktop/server/app.rs`
- Modify: `src-tauri/src/desktop/server.rs` (add `mod app`)

- [x] **Step 1: Create `desktop/server/app.rs` — Actix Web App config**

```rust
//! Actix Web Application Configuration
//!
//! 配置路由、中间件和服务器启动

use actix_web::{web, App, HttpServer, HttpResponse, Error, dev::ServiceRequest, HttpMessage};
use actix_web::middleware::cors::Cors;
use actix_cors::Cors as ActixCors;

use crate::desktop::server::controllers::{
    auth_controller, session_controller, config_controller, file_controller,
};
use crate::desktop::server::middleware::jwt_auth::validate_jwt;
use crate::shared::system::config::AppConfig;

/// JWT 认证中间件 wrapper
///
/// 对 /api/* 路由（除了 /api/auth/* 开头的公开路由）进行 JWT 验证
async fn jwt_middleware(
    req: ServiceRequest,
    next: actix_web::dev::ServiceNext<impl actix_web::dev::Service>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, Error> {
    let path = req.path().to_string();

    // 公开路由跳过 JWT 验证
    if path.starts_with("/api/auth/") {
        return next.call(req).await;
    }

    validate_jwt(&req)?;
    next.call(req).await
}

/// 构建路由配置
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // 公开路由（无需 JWT）
    cfg.service(
        web::scope("/api/auth")
            .route("/pairing", web::post().to(auth_controller::request_pairing))
            .route("/verify", web::post().to(auth_controller::verify_pairing_code))
            .route("/qr-connect", web::post().to(auth_controller::qr_connect))
            .route("/reauth", web::post().to(auth_controller::reauthenticate))
    );

    // 受保护路由（需要 JWT）
    cfg.service(
        web::scope("/api")
            .route("/sessions", web::get().to(session_controller::list_sessions))
            .route("/sessions/start", web::post().to(session_controller::start_session))
            .route("/sessions/{id}/stop", web::post().to(session_controller::stop_session))
            .route("/sessions/{id}/resize", web::post().to(session_controller::resize_session))
            .route("/sessions/{id}/remove", web::delete().to(session_controller::remove_session))
            .route("/configs", web::get().to(config_controller::list_configs))
            .route("/quick-actions", web::get().to(config_controller::list_quick_actions))
            .route("/file-tree", web::post().to(file_controller::get_file_tree))
    );
}

/// 启动 Actix Web HTTP 服务器
///
/// 在独立端口上启动（Phase 1 使用 8081，Phase 3 合并到主端口）
pub async fn start_http_server(port: u16) -> std::io::Result<()> {
    tracing::info!("Starting Actix Web HTTP server on port {}", port);

    HttpServer::new(|| {
        let cors = ActixCors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .configure(configure_routes)
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await
}
```

- [x] **Step 2: Update `desktop/server.rs` to add `mod app`**

Add `pub mod app;` to `src-tauri/src/desktop/server.rs`.

- [x] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/desktop/server/app.rs src-tauri/src/desktop/server.rs
git commit -m "feat(server): add Actix Web app config with routes and server startup"
```

---

### Task 8: Integrate Actix Web Server into Desktop Startup

**Files:**
- Modify: `src-tauri/src/desktop/websocket_manager.rs` (add Actix Web start alongside existing WsServer)
- Modify: `src-tauri/src/desktop/events/ws_server_handler.rs` (add HTTP server start event)

- [x] **Step 1: Add Actix Web server start in `websocket_manager.rs`**

In `WebSocketManager`, find the `start_server` method. After the existing `ws_server.start()` call (around line where the tokio::spawn starts the WsServer), add:

```rust
// 启动 Actix Web HTTP 服务器（Phase 1: 独立端口 8081）
let http_port = config.network.port + 1;
tokio::spawn(async move {
    if let Err(e) = crate::desktop::server::app::start_http_server(http_port).await {
        tracing::error!("Actix Web HTTP server error: {}", e);
    }
});
tracing::info!("Actix Web HTTP server started on port {}", http_port);
```

The exact location: in `websocket_manager.rs`, search for `ws_server.start()` or the `tokio::spawn` block that runs the WsServer. Insert the Actix Web spawn immediately after that block.

- [x] **Step 2: Verify compilation and test**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

Start the desktop app with `npm run tauri:dev`, then test with curl:

```bash
curl http://localhost:8081/api/sessions
```

Expected: Returns 401 (JWT required) or JSON response if auth routes are tested first.

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/desktop/websocket_manager.rs
git commit -m "feat(server): start Actix Web HTTP server alongside existing WsServer"
```

---

## Phase 2: Mobile Frontend HTTP API

### Task 9: Create useHttpApi Composable

**Files:**
- Create: `src/modules/mobile/composables/useHttpApi.ts`

- [x] **Step 1: Create the HTTP API client composable**

```typescript
//! HTTP API Client Composable
//!
//! 移动端直接调用桌面端 HTTP REST API
//! JWT token 自动注入到 Authorization header

import { ref, readonly } from 'vue'
import { useMobileConnection } from './useMobileConnection'

// ==================== Config ====================

const API_BASE_URL = ref<string>('')

// ==================== Core HTTP Client ====================

interface ApiResult<T = any> {
  code: number
  message: string
  data?: T
}

async function request<T = any>(
  path: string,
  options: RequestInit = {}
): Promise<ApiResult<T>> {
  const { authCredentials } = useMobileConnection()
  const baseUrl = API_BASE_URL.value

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  }

  // 注入 JWT token（auth 路由除外）
  if (!path.startsWith('/api/auth/') && authCredentials.value?.sessionToken) {
    headers['Authorization'] = `Bearer ${authCredentials.value.sessionToken}`
  }

  const response = await fetch(`http://${baseUrl}${path}`, {
    ...options,
    headers,
  })

  return response.json()
}

// ==================== Auth API ====================

export async function httpRequestPairing(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
}) {
  return request<{ pairingCode: string; expiresIn: number }>(
    '/api/auth/pairing',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpVerifyPairingCode(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
  pairingCode: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/verify',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpQrConnect(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
  qrToken: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/qr-connect',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpReauth(data: {
  deviceId: string
  fingerprint: string
  sessionToken: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/reauth',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

// ==================== Session API ====================

export async function httpListSessions() {
  return request<{ sessions: any[] }>('/api/sessions')
}

export async function httpStartSession(configId: string) {
  return request<{ sessionId: string; status: string }>(
    '/api/sessions/start',
    { method: 'POST', body: JSON.stringify({ configId }) }
  )
}

export async function httpStopSession(sessionId: string) {
  return request(`/api/sessions/${sessionId}/stop`, { method: 'POST' })
}

export async function httpResizeSession(sessionId: string, cols: number, rows: number) {
  return request(`/api/sessions/${sessionId}/resize`, {
    method: 'POST',
    body: JSON.stringify({ cols, rows }),
  })
}

export async function httpRemoveSession(sessionId: string) {
  return request(`/api/sessions/${sessionId}/remove`, { method: 'DELETE' })
}

// ==================== Config API ====================

export async function httpListConfigs() {
  return request<{ configs: any[] }>('/api/configs')
}

export async function httpListQuickActions() {
  return request<{ actions: any[] }>('/api/quick-actions')
}

// ==================== File API ====================

export async function httpGetFileTree(sessionId: string, excludeDirs: string[] = []) {
  return request<{ tree: any[] }>(
    '/api/file-tree',
    { method: 'POST', body: JSON.stringify({ sessionId, excludeDirs }) }
  )
}

// ==================== Setup ====================

export function setApiBaseUrl(address: string, port: number) {
  API_BASE_URL.value = `${address}:${port + 1}`
}

export function useHttpApi() {
  return {
    setApiBaseUrl,
    // Auth
    httpRequestPairing,
    httpVerifyPairingCode,
    httpQrConnect,
    httpReauth,
    // Session
    httpListSessions,
    httpStartSession,
    httpStopSession,
    httpResizeSession,
    httpRemoveSession,
    // Config
    httpListConfigs,
    httpListQuickActions,
    // File
    httpGetFileTree,
  }
}
```

- [x] **Step 2: Commit**

```bash
git add src/modules/mobile/composables/useHttpApi.ts
git commit -m "feat(mobile): add useHttpApi composable for direct HTTP API calls"
```

---

### Task 10: Migrate Mobile Frontend from WS to HTTP for Non-Terminal Operations

**Files:**
- Modify: `src/modules/mobile/composables/useMobileConnection.ts`
- Modify: `src/modules/mobile/composables/useMobileCommands.ts` (if exists)

- [x] **Step 1: Update `useMobileConnection.ts` to use HTTP for session/config operations**

Replace WS-based calls with HTTP API calls:

- `loadSessionConfigs()` → call `httpListConfigs()` instead of `wsLoadSessionConfigs()`
- `loadActiveSessions()` → call `httpListSessions()` instead of `wsLoadSessions()`
- `startSession()` → call `httpStartSession()` instead of `wsStartSession()`
- `stopSession()` → call `httpStopSession()` then update local state
- `removeSession()` → call `httpRemoveSession()` then update local state

Import `useHttpApi` at the top and call `setApiBaseUrl(address, port)` in the `connect()` function.

Keep WS for:
- Terminal input (`sendInput` still goes through WS)
- Terminal subscribe/unsubscribe (still through WS)
- Auth pairing flow (can use HTTP, but keep WS connect for now)

- [x] **Step 2: Test mobile frontend**

Run: `npm run tauri:dev`
Then connect from mobile and verify session list, config list, and session start/stop work through HTTP.

- [x] **Step 3: Commit**

```bash
git add src/modules/mobile/composables/
git commit -m "feat(mobile): migrate session/config operations from WS to HTTP API"
```

---

## Phase 3: Migrate WS to Actix Web

### Task 11: Create Actix WebSocket Actor for Terminal I/O

**Files:**
- Create: `src-tauri/src/desktop/server/ws.rs`
- Create: `src-tauri/src/desktop/server/ws/terminal_ws.rs`
- Create: `src-tauri/src/desktop/server/ws/session.rs`

- [x] **Step 1: Create `ws/session.rs` — WS session state management**

```rust
//! WebSocket Session State
//!
//! 管理 WS 连接的认证状态和订阅信息

use std::collections::HashSet;
use std::net::SocketAddr;

/// WebSocket 会话状态
pub struct WsSession {
    /// 客户端地址
    pub addr: SocketAddr,
    /// 设备 ID（认证后设置）
    pub device_id: Option<String>,
    /// 设备名称
    pub device_name: Option<String>,
    /// 是否已认证
    pub authenticated: bool,
    /// 订阅的会话列表
    pub subscribed_sessions: HashSet<String>,
}

impl WsSession {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            device_id: None,
            device_name: None,
            authenticated: false,
            subscribed_sessions: HashSet::new(),
        }
    }
}
```

- [x] **Step 2: Create `ws/terminal_ws.rs` — Actix WS actor**

```rust
//! Terminal WebSocket Actor
//!
//! 处理终端 I/O 的 WebSocket 连接
//! 使用 actix-web-actors 的 WS actor 模式

use actix::prelude::*;
use actix_web_actors::ws;
use actix_web_actors::ws::{Message as WsMessage, ProtocolError};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::desktop::server::ws::session::WsSession;
use crate::desktop::server::message::Message;
use crate::desktop::app_context::AppContext;
use crate::desktop::session::GlobalOutputManager;
use crate::shared::auth::jwt::JwtService;
use crate::shared::enums::{TerminalAction, TerminalPayload};

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// 心跳超时
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Terminal WebSocket Actor
pub struct TerminalWs {
    /// 会话状态
    session: WsSession,
    /// 最后心跳时间
    hb: Instant,
}

impl TerminalWs {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            session: WsSession::new(addr),
            hb: Instant::now(),
        }
    }

    /// 心跳检测
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                tracing::warn!("WebSocket heartbeat timeout for {}", act.session.addr);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for TerminalWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Terminal WS connected: {}", self.session.addr);
        self.hb(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("Terminal WS disconnected: {}", self.session.addr);

        // 取消所有订阅
        let global_manager = GlobalOutputManager::global();
        for session_id in &self.session.subscribed_sessions {
            let _ = global_manager.unsubscribe(session_id, &self.session.addr.to_string()).await;
        }

        Running::Stop
    }
}

/// 处理 WebSocket 消息
impl StreamHandler<Result<WsMessage, ProtocolError>> for TerminalWs {
    fn handle(&mut self, msg: Result<WsMessage, ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!("WS protocol error: {}", e);
                ctx.stop();
                return;
            }
        };

        match msg {
            WsMessage::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            WsMessage::Pong(_) => {
                self.hb = Instant::now();
            }
            WsMessage::Text(text) => {
                self.handle_text_message(text.to_string(), ctx);
            }
            WsMessage::Binary(_) => {
                // 二进制消息暂不支持
            }
            WsMessage::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl TerminalWs {
    /// 处理文本消息（JSON 格式的 Message）
    fn handle_text_message(&mut self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
        let message = match Message::from_json(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse WS message: {}", e);
                let error = Message::error("PARSE_ERROR", &e.to_string());
                if let Ok(json) = error.to_json() {
                    ctx.text(json);
                }
                return;
            }
        };

        match message {
            Message::Auth { payload, message_id, .. } => {
                self.handle_auth(payload, message_id, ctx);
            }
            Message::Terminal { session_id, payload, message_id, .. } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() { ctx.text(json); }
                    return;
                }
                self.handle_terminal(session_id, payload, ctx);
            }
            _ => {
                tracing::debug!("Unsupported WS message type from {}", self.session.addr);
            }
        }
    }

    /// 处理认证消息
    fn handle_auth(
        &mut self,
        payload: crate::shared::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let jwt_service = JwtService::new();
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                let error = Message::error_with_id(&message_id, "NO_TOKEN", "No JWT token provided");
                if let Ok(json) = error.to_json() { ctx.text(json); }
                return;
            }
        };

        match jwt_service.verify_token_with_expiry(&token) {
            Ok(claims) => {
                self.session.authenticated = true;
                self.session.device_id = Some(claims.sub.clone());
                self.session.device_name = claims.device_name.clone();

                // 通知桌面端
                let ctx = AppContext::global();
                let _ = ctx.app_handle().emit("device-connected", &crate::desktop::server::connection_types::DeviceConnectionEvent {
                    addr: self.session.addr.to_string(),
                    device_id: claims.sub,
                    device_name: claims.device_name,
                    event: "authenticated".to_string(),
                });

                let response = Message::Auth {
                    message_id,
                    expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: crate::shared::enums::AuthPayload {
                        stage: crate::shared::enums::AuthStage::Authenticated,
                        device_id: self.session.device_id.clone(),
                        device_name: self.session.device_name.clone(),
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        ..Default::default()
                    },
                };
                if let Ok(json) = response.to_json() { ctx.text(json); }
            }
            Err(e) => {
                let msg = match e {
                    crate::shared::auth::jwt::JwtError::TokenExpired => "Token expired",
                    _ => "Invalid token",
                };
                let error = Message::error_with_id(&message_id, "AUTH_FAILED", msg);
                if let Ok(json) = error.to_json() { ctx.text(json); }
            }
        }
    }

    /// 处理终端消息
    fn handle_terminal(
        &mut self,
        session_id: String,
        payload: TerminalPayload,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.action {
            TerminalAction::Input { data, special_key } => {
                let app_ctx = AppContext::global();
                let sm = app_ctx.session_manager();
                if let Err(e) = crate::desktop::server::services::terminal_service::handle_input(
                    &session_id,
                    TerminalPayload { action: TerminalAction::Input { data, special_key } },
                    &Some(sm.clone()),
                ) {
                    tracing::error!("Terminal input error: {}", e);
                }
            }
            TerminalAction::Subscribe { start_seq } => {
                self.handle_subscribe(&session_id, ctx);
            }
            TerminalAction::Unsubscribe => {
                self.handle_unsubscribe(&session_id, ctx);
            }
            _ => {}
        }
    }

    /// 订阅会话输出
    fn handle_subscribe(
        &mut self,
        session_id: &str,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();

        // 创建输出转发通道
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<crate::desktop::session::OutputEvent>(256);

        match global_manager.subscribe(session_id, &client_id, output_tx).await {
            Some(response) => {
                self.session.subscribed_sessions.insert(session_id.to_string());

                // 发送订阅响应
                let msg = Message::subscribe_response(
                    session_id,
                    response.min_seq,
                    response.max_seq,
                    response.history_count,
                );
                if let Ok(json) = msg.to_json() { ctx.text(json); }
            }
            None => {
                let error = Message::error("SESSION_NOT_FOUND", &format!("Session {} not found", session_id));
                if let Ok(json) = error.to_json() { ctx.text(json); }
            }
        }
    }

    /// 取消订阅
    fn handle_unsubscribe(
        &mut self,
        session_id: &str,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();

        if global_manager.unsubscribe(session_id, &client_id).await {
            self.session.subscribed_sessions.remove(session_id);
            let msg = Message::unsubscribe_response(session_id);
            if let Ok(json) = msg.to_json() { ctx.text(json); }
        }
    }
}
```

**Note:** The `subscribe`/`unsubscribe` methods use `.await` inside the WS actor's synchronous `handle_text_message`. To resolve this, use `ctx.run_later` with a zero-duration delay to defer the async work, or use `actix::fut::wrap_future` to spawn the async operation. The subscribe handler should:

1. Create the `output_tx` channel
2. Use `actix::spawn` to run the subscription logic in a separate task
3. On success, send the subscribe response back via `ctx.address().send()`

This pattern is standard for Actix WS actors that need to call async services. The implementation will use `ArwLock` or `actix::spawn` to bridge the sync/async boundary.

- [x] **Step 3: Create `ws.rs` module file**

```rust
//! WebSocket Handler Module

pub mod terminal_ws;
pub mod session;
```

- [x] **Step 4: Update `server.rs` to add `mod ws`**

Add `pub mod ws;` to `src-tauri/src/desktop/server.rs`.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/desktop/server/ws.rs src-tauri/src/desktop/server/ws/
git commit -m "feat(server): add Actix WS actor for terminal I/O"
```

---

### Task 12: Add WS Route to Actix Web and Merge to Single Port

**Files:**
- Modify: `src-tauri/src/desktop/server/app.rs` (add WS route)

- [x] **Step 1: Add WebSocket route handler**

Add to `app.rs`:

```rust
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws as actix_ws;
use crate::desktop::server::ws::terminal_ws::TerminalWs;

/// WS 握手端点
async fn terminal_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let addr = req.peer_addr().unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
    let ws_actor = TerminalWs::new(addr);
    actix_ws::start(ws_actor, &req, stream)
}
```

Update `configure_routes` to include:

```rust
cfg.route("/ws/terminal", web::get().to(terminal_ws));
```

- [x] **Step 2: Change server to use main port (same as old WsServer)**

Update `start_http_server` to accept the main port and use the same port as the existing WsServer. In `websocket_manager.rs`, update the spawn call to use the main port instead of port+1.

- [x] **Step 3: Test WS connection**

Start desktop app, connect from mobile via `ws://host:port/ws/terminal`.

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/desktop/server/app.rs src-tauri/src/desktop/websocket_manager.rs
git commit -m "feat(server): add WS route and merge to single port"
```

---

### Task 13: Update Mobile Rust WS Client to Connect to /ws/terminal

**Files:**
- Modify: `src-tauri/src/mobile/remote/connection.rs`
- Modify: `src-tauri/src/shared/websocket/client/ws_client.rs` (or config struct)

- [x] **Step 1: Update WS client config to include path**

Find `WsClientConfig` and add/modify the URL construction to include `/ws/terminal`:

```rust
// Old: ws://host:port
// New: ws://host:port/ws/terminal
```

The config should include a `path` field defaulting to `/ws/terminal`.

- [x] **Step 2: Update mobile connection manager**

Update `ConnectionManager::connect()` to use the new WS URL format with path.

- [x] **Step 3: Test mobile WS connection**

Connect from mobile and verify terminal I/O works through the new Actix WS endpoint.

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/mobile/remote/connection.rs src-tauri/src/shared/websocket/client/
git commit -m "feat(mobile): update WS client to connect to /ws/terminal"
```

---

## Phase 4: Cleanup Mobile Rust

### Task 14: Simplify Mobile Rust Layer

**Files:**
- Delete: `src-tauri/src/mobile/remote/request.rs` (WS message builders for removed message types)
- Delete: `src-tauri/src/mobile/handler/` (old WS handlers)
- Delete: `src-tauri/src/mobile/router/` (ClientBusinessRouter)
- Modify: `src-tauri/src/mobile/remote/connection.rs` (simplify to WS-only)
- Modify: `src-tauri/src/mobile/commands/` (remove HTTP-related Tauri commands)

- [x] **Step 1: Identify and remove unused mobile handlers**

Remove files that handled messages now going through HTTP:
- `mobile/handler/auth.rs` — auth now via HTTP
- `mobile/handler/sync.rs` — sync events still via WS push
- `mobile/handler/system.rs` — system messages still via WS push
- `mobile/handler/terminal.rs` — KEEP (terminal still via WS)

Actually, sync/system handlers receive push events from server, so they stay. But the auth handler's request logic (pairing/verify) is replaced by HTTP. The WS auth handler only needs to handle the initial JWT auth message after WS connect.

- [x] **Step 2: Simplify `mobile/remote/request.rs`**

Remove message builders for types that now use HTTP (session_control, session_config, auth pairing/verify). Keep terminal message builders.

- [x] **Step 3: Simplify mobile router**

Remove routes for message types now going through HTTP. Keep terminal and push event routes.

- [x] **Step 4: Remove HTTP-related Tauri commands**

Remove commands like `ws_load_sessions`, `ws_start_session`, `ws_stop_session`, etc. that are now replaced by direct HTTP calls from the frontend. Keep `ws_connect`, `ws_disconnect`, `ws_send_input`, `ws_subscribe_session`.

- [x] **Step 5: Verify compilation and test**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 6: Commit**

```bash
git add -A src-tauri/src/mobile/
git commit -m "refactor(mobile): simplify Rust layer, remove HTTP-related WS handlers"
```

---

## Phase 5: Remove Old Dependencies and Code

### Task 15: Delete Old Server Infrastructure

**Files:**
- Delete: `src-tauri/src/shared/websocket/server/` (old WsServer, connection_manager, heartbeat, etc.)
- Delete: `src-tauri/src/desktop/server/router/` (old BusinessRouter, middleware, registry)
- Delete: `src-tauri/src/desktop/server/handlers/` (old WS handlers, replaced by controllers)
- Delete: `src-tauri/src/desktop/server/auth_interceptor.rs`
- Delete: `src-tauri/src/desktop/events/ws_server_handler.rs` (or update)
- Modify: `src-tauri/src/desktop/server.rs` (remove old module declarations)
- Modify: `src-tauri/src/shared/websocket.rs` (remove server module)
- Modify: `src-tauri/src/desktop/websocket_manager.rs` (remove old WsServer usage)

- [x] **Step 1: Remove old WsServer from WebSocketManager**

Replace the old `WsServer` field and `start_server` method with the Actix Web server startup.

- [x] **Step 2: Delete old shared/server modules**

Delete:
- `shared/websocket/server.rs`
- `shared/websocket/server/` directory (all files)

- [x] **Step 3: Delete old desktop server modules**

Delete:
- `desktop/server/router/` directory
- `desktop/server/handlers/` directory (already replaced by `controllers/`)
- `desktop/server/auth_interceptor.rs`
- `desktop/server/router.rs`

- [x] **Step 4: Update module declarations**

Update `desktop/server.rs` to remove deleted modules and keep the new ones:
```rust
pub mod app;
pub mod client_info;
pub mod connection_types;
pub mod controllers;
pub mod dtos;
pub mod message;
pub mod middleware;
pub mod port_checker;
pub mod services;
pub mod ws;
```

Update `shared/websocket.rs` to remove server module:
```rust
pub mod client;
pub mod codec;
pub mod traits;
// server module removed
```

- [x] **Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Errors related to removed modules — fix all references.

This step is iterative: remove → compile → fix references → repeat.

- [x] **Step 6: Commit**

```bash
git add -A src-tauri/src/
git commit -m "refactor(server): remove old WsServer and WS handler infrastructure"
```

---

### Task 16: Remove Old Dependencies from Cargo.toml

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [x] **Step 1: Remove replaced dependencies**

Remove from desktop dependencies (or move to mobile-only):
- `hyper` — replaced by Actix Web
- `hyper-util` — replaced by Actix Web
- `http-body-util` — replaced by Actix Web

Keep:
- `tokio-tungstenite = "0.24"` — still needed for mobile WS client
- `futures-util = "0.3"` — check if still used; remove if not
- `reqwest = "0.12"` — still used by mobile HTTP client

Since `tokio-tungstenite` is used by the mobile WS client but not the desktop server, consider making it a mobile-only dependency. However, `shared/websocket/client/` uses it, so it stays in shared dependencies for now.

- [x] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [x] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore(deps): remove hyper/hyper-util/http-body-util, replaced by Actix Web"
```

---

### Task 17: Update docs/code-map.md

**Files:**
- Modify: `docs/code-map.md`

- [x] **Step 1: Update the code map to reflect new structure**

Update directory tree and module descriptions to reflect:
- New `desktop/server/controllers/` directory
- New `desktop/server/dtos/` directory
- New `desktop/server/middleware/` directory
- New `desktop/server/ws/` directory
- New `desktop/server/app.rs`
- Removed old modules (handlers, router, shared/websocket/server)
- Updated mobile structure (simplified)

- [x] **Step 2: Commit**

```bash
git add docs/code-map.md
git commit -m "docs: update code-map for Actix Web migration"
```

---

### Task 18: Final Integration Test

- [x] **Step 1: Desktop-only test**

Run: `npm run tauri:dev`

Test:
1. Desktop frontend works normally (session list, config CRUD, terminal)
2. QR code generation works
3. Pairing code display works

- [x] **Step 2: Mobile HTTP test**

From mobile:
1. Request pairing code via HTTP → verify code appears on desktop
2. Verify pairing code via HTTP → verify JWT token returned
3. List sessions via HTTP → verify session list
4. Start session via HTTP → verify session starts
5. List configs via HTTP → verify config list
6. Get file tree via HTTP → verify tree returned

- [x] **Step 3: Mobile WS test**

From mobile:
1. Connect WS to `/ws/terminal`
2. Send JWT auth message → verify auth success
3. Subscribe to session → verify subscribe response
4. Send terminal input → verify output received
5. Unsubscribe → verify unsubscribe response
6. Disconnect → verify clean disconnect

- [x] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration test fixes for Actix Web migration"
```
