//! 认证链 HTTP 编排域（票 07）——`/api/auth/*` 七端点的插件侧实现
//!
//! **职责边界（票 07 用户裁定，ADR 0022 修订口径）**：认证**执行编排**归本插件
//! （配对码 / QR / 挑战状态机、JWT 签发与 Claims 构造、连接记录的调用决策），
//! **密钥托管与信任表留宿主**——插件经 host-auth 原语（v19 追加 ×7）回调宿主
//! 统一认证：`trusted-device-upsert` / `trusted-device-touch` /
//! `connection-history-record`（信任与历史记录写面）、`biometric-credential-bound`
//! / `biometric-verify-signature` / `biometric-credential-bind`（生物凭证，
//! 公钥不出宿主）、`link-identity-parts`（链路身份公开材料）。
//!
//! 认证插件**默认常开**（用户裁定）：网关对这七条是 `Public + PluginRequired`
//! 公开路由——JWT 之前的入口免验签转发，插件未激活即明确报错，无宿主降级轨。
//!
//! 字节级契约：宿主旧 `auth_controller` 的响应形状、业务码（1001/1005/1006/
//! 1007/1008/1009/1010）与确定性错误文案逐字复刻；JWT 与宿主 `JwtService`
//! 逐字节同构（同一 secret-store 密钥 + HS256 + 同 Claims 形状），宿主中间件
//! `enforce_connection_policy`（本插件 policy 导出）无感。

pub mod biometric;
pub mod jwt;

use bedcode_plugin_api::http_response;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 本域 HTTP 端点清单（plugin.json `contributes.httpEndpoints` 的单一事实源，
/// 与网关别名表逐字一致；契约用例锁死）
pub const AUTH_HTTP_ENDPOINTS: &[&str] = &[
    "auth/pairing",
    "auth/verify",
    "auth/qr-connect",
    "auth/reauth",
    "auth/biometric-challenge",
    "auth/biometric-verify",
    "auth/biometric-bind",
];

/// 配对码 TTL（与宿主旧端点 `constants::auth::PAIRING_CODE_TTL_SECS` 同值：
/// HTTP 路径沿用该默认值，字节级行为保持）
const PAIRING_CODE_TTL_SECS: u64 = 60;

/// 认证业务码（与宿主旧 controller 逐字一致）
const CODE_TOKEN_FAILURE: i32 = 1001;
const CODE_PAIRING_INVALID: i32 = 1005;
const CODE_QR_INVALID: i32 = 1006;
const CODE_PLUGIN_NOT_ACTIVATED: i32 = 1007;
const CODE_BIO_CHALLENGE: i32 = 1008;
const CODE_BIO_VERIFY: i32 = 1009;
const CODE_BIO_BIND: i32 = 1010;

/// 连接历史取值（**与内核 `db::models::connection_method` / `connection_result` 逐字一致**）
///
/// 宿主原语 `auth_connection_history_record` 原样落库、**不做大小写归一化**，而消费侧
/// （桌面连接历史页 `useConnectionHistory.ts`：`METHOD_KEY_SUFFIX` 映射 i18n key、
/// `result === 'success'` 计数）只认小写——大小写是**对外形状**，不是内部枚举，
/// 不得在这里「统一大写」（票 13 实测：写大写会让方式显示为未知、成功/失败计数全错）。
/// native 构建无调用方（写入点全部 `#[cfg(target_arch = "wasm32")]`）→ 不报未使用。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod history_value {
    pub const PAIRING_CODE: &str = "pairing_code";
    pub const QR: &str = "qr";
    pub const BIOMETRIC: &str = "biometric";
    pub const JWT: &str = "jwt";
    pub const SUCCESS: &str = "success";
    pub const FAILED: &str = "failed";
}

// ==================== 响应信封辅助（HTTP 200 + 业务码，宿主同口径） ====================

/// 宿主旧 auth 端点的错误口径是 `HttpResponse::Ok().json(ApiResponse::error(..))`
/// —— HTTP 200 + 业务码信封；与文件域同构，放本域避免跨域耦合
fn error_response(code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": code, "message": message },
    })
}

fn ok_with_data(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok", "data": data },
    })
}

// ==================== 分派（wasm 运行时有实现；native 显性失败） ====================

/// 本域 HTTP 分派入口（lib.rs 业务分派落点；路径全等匹配）
#[cfg(target_arch = "wasm32")]
pub fn handle_http_endpoint(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &serde_json::Value,
    _query: &serde_json::Value,
) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    match path {
        "auth/pairing" => handle_pairing(host, body),
        "auth/verify" => handle_verify(host, body),
        "auth/qr-connect" => handle_qr_connect(host, body),
        "auth/reauth" => handle_reauth(host, body),
        "auth/biometric-challenge" => handle_biometric_challenge(host, body),
        "auth/biometric-verify" => handle_biometric_verify(host, body),
        "auth/biometric-bind" => handle_biometric_bind(host, body),
        _ => http_response::error(404, &format!("Not found: {path}")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn handle_http_endpoint(
    _host: &WasmHost,
    _method: &str,
    _path: &str,
    _body: &serde_json::Value,
    _query: &serde_json::Value,
) -> serde_json::Value {
    error_response(CODE_TOKEN_FAILURE, "auth endpoints unavailable outside wasm runtime")
}

// ==================== 入参提取（auth DTO 全部带 rename_all=camelCase） ====================

fn str_field<'a>(body: &'a serde_json::Value, key: &str) -> &'a str {
    body.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

// ==================== POST /api/auth/pairing ====================

/// 请求配对码（TTL 与宿主旧端点同值 60s——`constants::auth::PAIRING_CODE_TTL_SECS`；
/// 桌面前端经 `pairing-code-generated` 事件显示，事件名与载荷逐字节一致）
#[cfg(target_arch = "wasm32")]
fn handle_pairing(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let device_name = str_field(body, "deviceName").to_string();
    match crate::pair_code_generate(PAIRING_CODE_TTL_SECS) {
        Ok(code) => {
            // 通知桌面前端显示配对码（载荷形状 = 宿主 PairingCodeGeneratedEvent）
            let _ = host_events_emit(
                host,
                "pairing-code-generated",
                serde_json::json!({
                    "code": code["code"],
                    "expires_in": code["expires_in"],
                    "device_name": device_name,
                }),
            );
            ok_with_data(serde_json::json!({
                "pairingCode": code["code"],
                "expiresIn": code["expires_in"],
            }))
        }
        Err(e) => {
            host.log_error(&format!("auth http: pairing generate failed: {e}"));
            error_response(CODE_PAIRING_INVALID, "Failed to generate pairing code")
        }
    }
}

// ==================== POST /api/auth/verify ====================

/// 验证配对码 → 签发 JWT（一次性消耗语义与生成同源）
#[cfg(target_arch = "wasm32")]
fn handle_verify(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let device_id = str_field(body, "deviceId").to_string();
    let device_name = str_field(body, "deviceName").to_string();
    let fingerprint = str_field(body, "fingerprint").to_string();
    let uid_hash = body.get("uidHash").and_then(|v| v.as_str()).map(str::to_string);
    let pairing_code = str_field(body, "pairingCode").to_string();
    let address = str_field(body, "address").to_string();

    // 验证（一次性：成功即消耗；经认证中心桥接的同一状态机）
    let is_valid = matches!(crate::pair_code_verify(&pairing_code), Ok(true));

    if !is_valid {
        let _ = connection_history_record(
            host,
            &fingerprint,
            history_value::PAIRING_CODE,
            history_value::FAILED,
            Some(&address),
        );
        // 「是否有当前码」与验证同源（宿主同逻辑：文案二选一）
        let has_current = matches!(crate::pair_code_status(), Ok(Some(_)));
        let msg = if has_current {
            "Invalid or expired pairing code"
        } else {
            "No pairing code available. Please generate a new code."
        };
        return error_response(CODE_PAIRING_INVALID, msg);
    }

    // 签发 JWT（与宿主 JwtService 同构：同密钥 + HS256 + 同 Claims）
    let (token, expires_in) = match jwt::issue_device_token(
        &device_id,
        Some(&device_name),
        Some(&fingerprint),
    ) {
        Ok(v) => v,
        Err(e) => {
            host.log_error(&format!("auth http: jwt issue failed: {e}"));
            return error_response(CODE_TOKEN_FAILURE, "Failed to generate token");
        }
    };

    // 记录/更新配对设备（publicKey 缺省 = 保留既有值；展示名与宿主同构）
    let display_name = format_device_display_name(&device_name, &address);
    if let Err(e) = trusted_device_upsert(
        host,
        &display_name,
        &fingerprint,
        None,
        Some(&address),
        uid_hash.as_deref(),
    ) {
        host.log_warn(&format!("auth http: pairing record failed: {e}"));
    }
    let _ = connection_history_record(
        host,
        &fingerprint,
        history_value::PAIRING_CODE,
        history_value::SUCCESS,
        Some(&address),
    );

    // 通知桌面前端有设备连接
    let _ = emit_device_connected(host, &address, &device_id, Some(&device_name), Some(&fingerprint));

    ok_with_data(token_response(&token, expires_in))
}

// ==================== POST /api/auth/qr-connect ====================

/// QR 码认证 → 签发 JWT（拒绝原因分类与宿主 `qr_failure_user_message` 一致）
#[cfg(target_arch = "wasm32")]
fn handle_qr_connect(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let device_id = str_field(body, "deviceId").to_string();
    let device_name = str_field(body, "deviceName").to_string();
    let fingerprint = str_field(body, "fingerprint").to_string();
    let uid_hash = body.get("uidHash").and_then(|v| v.as_str()).map(str::to_string);
    let qr_token = str_field(body, "qrToken").to_string();
    let address = str_field(body, "address").to_string();

    // 验证（一次性；reason 分类与桥接层同构）
    let verify = match crate::qr_verify(&qr_token) {
        Ok(v) => v,
        Err(e) => {
            host.log_error(&format!("auth http: qr verify failed: {e}"));
            return error_response(CODE_QR_INVALID, "二维码验证服务不可用，请稍后重试");
        }
    };
    if verify["valid"].as_bool() != Some(true) {
        let reason = verify["reason"].as_str().unwrap_or("QR token invalid");
        let _ = connection_history_record(
            host,
            &fingerprint,
            history_value::QR,
            history_value::FAILED,
            Some(&address),
        );
        return error_response(CODE_QR_INVALID, qr_failure_user_message(reason));
    }

    let _ = host_events_emit(host, "qr-token-consumed", serde_json::Value::Null);

    let (token, expires_in) = match jwt::issue_device_token(
        &device_id,
        Some(&device_name),
        Some(&fingerprint),
    ) {
        Ok(v) => v,
        Err(e) => {
            host.log_error(&format!("auth http: jwt issue failed: {e}"));
            return error_response(CODE_TOKEN_FAILURE, "Failed to generate token");
        }
    };

    let display_name = format_device_display_name(&device_name, &address);
    if let Err(e) = trusted_device_upsert(
        host,
        &display_name,
        &fingerprint,
        None,
        Some(&address),
        uid_hash.as_deref(),
    ) {
        host.log_warn(&format!("auth http: pairing record failed: {e}"));
    }
    let _ = connection_history_record(
        host,
        &fingerprint,
        history_value::QR,
        history_value::SUCCESS,
        Some(&address),
    );

    let _ = emit_device_connected(host, &address, &device_id, Some(&device_name), Some(&fingerprint));

    ok_with_data(token_response(&token, expires_in))
}

// ==================== POST /api/auth/reauth ====================

/// 持既有 JWT 静默重认证（验签执行点 = 本插件 policy 导出，签发同源）
#[cfg(target_arch = "wasm32")]
fn handle_reauth(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let device_id = str_field(body, "deviceId").to_string();
    let fingerprint = str_field(body, "fingerprint").to_string();
    let session_token = str_field(body, "sessionToken").to_string();

    // 验签（宿主 JwtService 同一路径：签名 + 结构 + 时效，密钥不出宿主）
    let claims = match jwt::verify_device_token(host, &session_token) {
        Ok(c) => c,
        Err(e) => {
            let _ = connection_history_record(
                host,
                &fingerprint,
                history_value::JWT,
                history_value::FAILED,
                None,
            );
            host.log_warn(&format!("auth http: reauth verify failed: {e}"));
            return error_response(CODE_TOKEN_FAILURE, &e);
        }
    };
    let sub = claims["sub"].as_str().unwrap_or(&device_id).to_string();
    let device_name = claims["device_name"].as_str().map(str::to_string);
    let token_fingerprint = claims["fingerprint"].as_str().map(str::to_string);

    let (token, expires_in) = match jwt::issue_device_token(
        &sub,
        device_name.as_deref(),
        token_fingerprint.as_deref(),
    ) {
        Ok(v) => v,
        Err(e) => {
            host.log_error(&format!("auth http: jwt issue failed: {e}"));
            return error_response(CODE_TOKEN_FAILURE, "Failed to generate token");
        }
    };

    // last_seen / connect_count 刷新（HTTP 重认证路径无地址，名称刷新由 WS 承担）
    if let Some(fp) = token_fingerprint.as_deref() {
        let _ = connection_history_record(
            host,
            fp,
            history_value::JWT,
            history_value::SUCCESS,
            None,
        );
        if let Err(e) = trusted_device_touch(host, fp) {
            host.log_warn(&format!("auth http: pairing touch failed: {e}"));
        }
    }

    ok_with_data(token_response(&token, expires_in))
}

// ==================== POST /api/auth/biometric-challenge ====================

/// 生物认证挑战下发（60s TTL 单次消费；闸门 = 已配对且绑定公钥，判定经宿主原语）
#[cfg(target_arch = "wasm32")]
fn handle_biometric_challenge(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let fingerprint = str_field(body, "deviceFingerprint").to_string();

    match biometric::issue_challenge(host, &fingerprint) {
        Ok(nonce) => ok_with_data(serde_json::json!({
            "challengeNonce": nonce,
            "expiresIn": biometric::BIO_CHALLENGE_TTL_SECS,
        })),
        Err(e) => {
            host.log_warn(&format!("auth http: biometric challenge failed: {e}"));
            // 挑战签发失败 = 认证尝试失败（未配对指纹不落库——宿主原语内跳过）
            let _ = connection_history_record(
                host,
                &fingerprint,
                history_value::BIOMETRIC,
                history_value::FAILED,
                None,
            );
            // 宿主同口径：DB 故障也归 1008；其余统一「未绑定」文案
            error_response(CODE_BIO_CHALLENGE, &e)
        }
    }
}

// ==================== POST /api/auth/biometric-verify ====================

/// 生物认证验签 → 签发 JWT（验签用宿主托管公钥，验签执行点在宿主）
#[cfg(target_arch = "wasm32")]
fn handle_biometric_verify(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let fingerprint = str_field(body, "deviceFingerprint").to_string();
    let nonce = str_field(body, "challengeNonce").to_string();
    let signature = str_field(body, "signature").to_string();

    match biometric::verify_signature(host, &fingerprint, &nonce, &signature) {
        Ok(record) => {
            // 记录：device_name 取配对记录（HTTP 端点无自报名称）；publicKey 缺省保留
            let pairing_id = record["id"].as_str().unwrap_or_default().to_string();
            let record_name = record["deviceName"].as_str().unwrap_or_default().to_string();
            let (token, expires_in) =
                match jwt::issue_device_token(&pairing_id, Some(&record_name), Some(&fingerprint)) {
                    Ok(v) => v,
                    Err(e) => {
                        host.log_error(&format!("auth http: jwt issue failed: {e}"));
                        return error_response(CODE_TOKEN_FAILURE, "Failed to generate token");
                    }
                };
            if let Err(e) = trusted_device_upsert(host, &record_name, &fingerprint, None, None, None) {
                host.log_warn(&format!("auth http: pairing record failed: {e}"));
            }
            let _ = connection_history_record(
                host,
                &fingerprint,
                history_value::BIOMETRIC,
                history_value::SUCCESS,
                None,
            );
            let _ = emit_device_connected(host, "", &pairing_id, Some(&record_name), Some(&fingerprint));
            ok_with_data(token_response(&token, expires_in))
        }
        Err(msg) => {
            let _ = connection_history_record(
                host,
                &fingerprint,
                history_value::BIOMETRIC,
                history_value::FAILED,
                None,
            );
            error_response(CODE_BIO_VERIFY, &msg)
        }
    }
}

// ==================== POST /api/auth/biometric-bind ====================

/// 绑定/解绑生物凭证公钥（须已认证且 token 指纹与请求指纹一致，防跨设备覆盖）
#[cfg(target_arch = "wasm32")]
fn handle_biometric_bind(host: &WasmHost, body: &serde_json::Value) -> serde_json::Value {
    let fingerprint = str_field(body, "deviceFingerprint").to_string();
    let public_key = str_field(body, "publicKey").to_string();
    let session_token = str_field(body, "sessionToken").to_string();

    // 1. JWT 校验（宿主 JwtService 同一路径）+ token 归属校验
    let claims = match jwt::verify_device_token(host, &session_token) {
        Ok(c) => c,
        Err(e) => {
            host.log_warn(&format!("auth http: bind verify failed: {e}"));
            return error_response(CODE_TOKEN_FAILURE, &e);
        }
    };
    if claims["fingerprint"].as_str() != Some(fingerprint.as_str()) {
        return error_response(CODE_PLUGIN_NOT_ACTIVATED, "Token does not belong to this device");
    }

    // 2. 更新凭证（空串 = 解绑；只改凭证不动计数——宿主原语语义）
    match biometric_credential_bind(host, &fingerprint, &public_key) {
        Ok(true) => ok_with_data(serde_json::json!({ "bound": !public_key.is_empty() })),
        Ok(false) => error_response(CODE_BIO_BIND, "Device not paired"),
        Err(e) => {
            host.log_warn(&format!("auth http: bind failed: {e}"));
            error_response(CODE_BIO_BIND, "Failed to update biometric credential")
        }
    }
}

// ==================== 宿主原语包装（wasm 运行时） ====================

use bedcode_plugin_api::host::{HostAuth, HostEvents, HostLog};

/// host-events.emit_event：Tauri 前端事件（事件名与载荷形状 = 宿主旧 emit 逐字节一致）
#[cfg(target_arch = "wasm32")]
fn host_events_emit(host: &WasmHost, event_name: &str, payload: serde_json::Value) -> Result<(), String> {
    host.emit_event(event_name, &payload);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn emit_device_connected(
    host: &WasmHost,
    addr: &str,
    device_id: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(), String> {
    host_events_emit(
        host,
        "device-connected",
        serde_json::json!({
            "addr": addr,
            "device_id": device_id,
            "device_name": device_name,
            "fingerprint": fingerprint,
            "event": "authenticated",
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn trusted_device_upsert(
    host: &WasmHost,
    device_name: &str,
    fingerprint: &str,
    public_key: Option<&str>,
    address: Option<&str>,
    uid_hash: Option<&str>,
) -> Result<String, String> {
    // 2026-09-22 认证记录下沉：配对记录真源 = 认证中心私有库。
    // `public_key` 不再写入配对记录（§8 凭据红线：公钥留宿主 plugin_secrets，
    // 生物绑定经 host-auth `biometric-credential-bind` 原语单独处理）——此处
    // 直接忽略该参数（调用方仍传 None，保留签名避免大面积改动）。
    let _ = (host, public_key);
    crate::auth_records::upsert(device_name, fingerprint, address, uid_hash)
}

#[cfg(target_arch = "wasm32")]
fn connection_history_record(
    _host: &WasmHost,
    fingerprint: &str,
    method: &str,
    result: &str,
    address: Option<&str>,
) -> Result<(), String> {
    crate::auth_records::record_connection_event(fingerprint, method, result, address)
}

/// 连接计数 / last_seen 刷新（2026-09-22 下沉：真源 = 认证中心私有库）
#[cfg(target_arch = "wasm32")]
fn trusted_device_touch(_host: &WasmHost, fingerprint: &str) -> Result<(), String> {
    crate::auth_records::touch(fingerprint)
}

/// 绑定/解绑生物凭证（host-auth `biometric-credential-bind` 原语——**公钥留宿主**
/// plugin_secrets，配对记录下沉后此路仍走宿主，凭据红线保持）
#[cfg(target_arch = "wasm32")]
fn biometric_credential_bind(host: &WasmHost, fingerprint: &str, public_key: &str) -> Result<bool, String> {
    use bedcode_plugin_api::host::HostAuth;
    host.auth_biometric_credential_bind(fingerprint, public_key).map_err(|e| e.message)
}

/// QR 拒绝原因 → 用户提示（宿主 `auth_center::qr_failure_user_message` 逐字复刻）
fn qr_failure_user_message(reason: &str) -> &str {
    if reason.contains("expired") {
        "二维码已过期，请重新生成"
    } else if reason.contains("already used") {
        "二维码已绑定其他设备，请重新扫描"
    } else if reason.contains("No active QR token") {
        "请先在桌面端生成二维码"
    } else {
        reason
    }
}

/// 展示名格式化（宿主 `format_device_display_name` 逐字复刻：名称 + (IP)）
fn format_device_display_name(device_name: &str, address: &str) -> String {
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}

/// 认证成功响应（`kdPublicB64` / `kdFingerprint` 缺席形态 = 字段不出现，宿主同构）
#[cfg(target_arch = "wasm32")]
fn token_response(token: &str, expires_in: u64) -> serde_json::Value {
    use bedcode_plugin_api::host::HostAuth;
    let host = bedcode_plugin_api::wasm_host::WasmHost;
    let mut data = serde_json::json!({
        "token": token,
        "expiresIn": expires_in,
    });
    match host.auth_link_identity_parts() {
        Ok(Some(identity)) => {
            data["kdPublicB64"] = identity["publicB64"].clone();
            data["kdFingerprint"] = identity["fingerprint"].clone();
        }
        _ => {}
    }
    data
}
