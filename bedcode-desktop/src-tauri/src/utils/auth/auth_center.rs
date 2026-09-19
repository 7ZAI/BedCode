//! 认证中心宿主桥接（票 11 C2 —— 宿主命令面桥接）
//!
//! 双轨并存期的桥接层：认证中心（`com.bedcode.devices`）激活时，配对码 / QR
//! token 生命周期操作经互调 api（ADR 0017、JSON-RPC 2.0 over host-bus）转发
//! 认证中心实现（状态以认证中心为准）；认证中心未激活或互调失败时**降级宿主
//! 实现**（迁移前行为，无单点）。
//!
//! 消费方：
//! - Tauri 命令面：`commands/system.rs`（配对码）、`commands/qr.rs`（QR）
//! - server 配对端点：`controllers/auth_controller.rs`（/api/auth/pairing ·
//!   /verify · /qr-connect）——移动端验签与前端展示必须同源，否则生成与验证
//!   落在不同状态存储上会破坏配对流程
//!
//! 边界（本票）：
//! - TTL 配置（`pairing_code_ttl` / `qr_token_ttl`）留宿主 DB 设置（配置域；
//!   生成命令把配置值传插件），TTL get/set 命令不转发
//! - 连接历史 / 已配对设备表（DB pairings）/ 在线设备列表（WS 注册表）留宿主
//!   内核存储（spec §3「不动」表；插件 trust 为独立镜像，转发会造成数据分叉
//!   回归），归票 13 退役时随数据源整合一并处理
//!
//! 互调调用约定：请求 topic `bedcode.api.<plugin-id>.<method>`，回复 topic
//! `bedcode.api.reply.<caller>.<request-id>`；caller 为宿主虚拟身份
//! [`crate::plugin::manager::wasm_runtime::host_impl::api::HOST_API_CALLER_ID`]
//! （互调门禁只校验目标 api 声明，不校验调用方）。

use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::server::services::pairing_service::PairingService;
use crate::utils::auth::{PairingCode, QrTokenManager};
use crate::{AppError, Result};

/// 认证中心插件 ID
pub const AUTH_CENTER_PLUGIN_ID: &str = "com.bedcode.devices";
/// 桥接探活锚点 api：注册表含它 ⇔ 认证中心已激活且互调面已声明
/// （激活登记 / 停用注销，见 ApiRegistry）
pub const AUTH_CENTER_MARKER_API: &str = "com.bedcode.devices.pairing-code-status";
/// 宿主→插件互调超时（毫秒）：单次操作远快于此，超时视为故障走降级
pub const AUTH_CENTER_TIMEOUT_MS: u64 = 5_000;

/// 认证中心是否激活（api 注册表只含激活态插件的声明）
pub fn auth_center_active(host_ctx: &WasmHostContext) -> bool {
    host_ctx.api_registry().contains(AUTH_CENTER_MARKER_API)
}

/// 宿主互调请求 id 计数器（全局单调；宿主多线程并发调用，reply topic 含
/// caller+id，id 必须全局唯一防止并发调用串台——SDK 侧用 thread_local（wasm
/// 单线程），宿主必须 Atomic）
static HOST_REQUEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_host_request_id() -> String {
    let n = HOST_REQUEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    format!("host-req-{n}")
}

/// 调用认证中心互调 api：构造 JSON-RPC 请求 → 发布等待 → 解码 reply → result 值
///
/// wire 形状与 SDK `api_call` 约定一致（spec §9.3）：请求
/// `{jsonrpc, id, method, params}`，响应 `{jsonrpc, id, result | error}`；
/// SDK api_call 模块被 `wasm` feature 门禁（guest 侧），宿主按同一约定本地实现。
fn call_api(host_ctx: &WasmHostContext, api: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let request_topic = format!("bedcode.api.{api}");
    // JSON-RPC `method` 字段 = 短方法名（与 SDK client 同约定：topic 全限定、
    // payload 短名——插件宏分派 `match req.method` 按短名匹配）
    let method = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(api);
    let id = next_host_request_id();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let reply_json = host_ctx
        .call_plugin_api_host(&request_topic, &payload.to_string(), AUTH_CENTER_TIMEOUT_MS)
        .map_err(|e| AppError::Plugin(format!("auth center api call '{api}' failed: {e}")))?;
    let reply: serde_json::Value = serde_json::from_str(&reply_json)
        .map_err(|e| AppError::Plugin(format!("auth center api '{api}' reply invalid JSON: {e}")))?;
    if let Some(err) = reply.get("error") {
        let message = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("rpc error")
            .to_string();
        return Err(AppError::Plugin(format!("auth center api '{api}' error: {message}")));
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| AppError::Plugin(format!("auth center api '{api}' reply missing result/error")))
}

/// 认证中心不可用时记录降级（结构化字段；认证中心未激活是常态，静默跳过）
fn log_fallback(api: &str, err: &AppError) {
    tracing::warn!(
        api = %api,
        error = %err,
        "auth center unavailable, fallback to host implementation"
    );
}

// ==================== 配对码 ====================

/// 生成配对码：认证中心激活 → 互调 `pairing-code-generate`（TTL 传入宿主配置值）；
/// 降级 → 宿主 `PairingService`（迁移前行为）。返回宿主 `PairingCode` DTO 形状。
pub async fn generate_pairing_code(
    host_ctx: &WasmHostContext,
    pairing_service: &PairingService,
    ttl: u64,
) -> Result<PairingCode> {
    if auth_center_active(host_ctx) {
        match call_api(
            host_ctx,
            "com.bedcode.devices.pairing-code-generate",
            serde_json::json!(ttl),
        ) {
            Ok(v) => return serde_json::from_value(v).map_err(AppError::Serialization),
            Err(e) => log_fallback("pairing-code-generate", &e),
        }
    }
    Ok(pairing_service.generate_code_with_ttl(ttl).await)
}

/// 当前配对码（过滤过期）：认证中心激活 → 互调 `pairing-code-status`；降级 →
/// 宿主 `PairingService`。`None`（验证路径的消息分类依赖「是否有当前码」）必须
/// 与验证走同一权威——验证失败分支据此区分「无码」/「码错或过期」。
pub async fn current_pairing_code(
    host_ctx: &WasmHostContext,
    pairing_service: &PairingService,
) -> Result<Option<PairingCode>> {
    if auth_center_active(host_ctx) {
        match call_api(
            host_ctx,
            "com.bedcode.devices.pairing-code-status",
            serde_json::Value::Null,
        ) {
            Ok(v) => {
                if v.is_null() {
                    return Ok(None);
                }
                return serde_json::from_value(v).map(Some).map_err(AppError::Serialization);
            }
            Err(e) => log_fallback("pairing-code-status", &e),
        }
    }
    Ok(pairing_service.get_current_code().await)
}

/// 验证配对码（一次性消费）：认证中心激活 → 互调 `pairing-code-verify`；降级 →
/// 宿主 `PairingService::verify_and_consume_code`。
pub async fn verify_pairing_code(
    host_ctx: &WasmHostContext,
    pairing_service: &PairingService,
    code: &str,
) -> Result<bool> {
    if auth_center_active(host_ctx) {
        match call_api(
            host_ctx,
            "com.bedcode.devices.pairing-code-verify",
            serde_json::json!(code),
        ) {
            Ok(v) => {
                return v.as_bool().ok_or_else(|| {
                    AppError::Plugin("auth center pairing-code-verify returned non-boolean".to_string())
                })
            }
            Err(e) => log_fallback("pairing-code-verify", &e),
        }
    }
    Ok(pairing_service.verify_and_consume_code(code).await)
}

/// 清除当前配对码：认证中心激活 → 互调 `pairing-code-clear`；降级 → 宿主。
pub async fn clear_pairing_code(host_ctx: &WasmHostContext, pairing_service: &PairingService) -> Result<()> {
    if auth_center_active(host_ctx) {
        match call_api(
            host_ctx,
            "com.bedcode.devices.pairing-code-clear",
            serde_json::Value::Null,
        ) {
            Ok(_) => return Ok(()),
            Err(e) => log_fallback("pairing-code-clear", &e),
        }
    }
    pairing_service.clear_code().await;
    Ok(())
}

// ==================== QR token ====================

/// 生成 QR token：认证中心激活 → 互调 `qr-code-generate`（TTL 传入宿主配置值）；
/// 降级 → 宿主 `QrTokenManager`。返回 token 字符串。
pub async fn generate_qr_code(host_ctx: &WasmHostContext, qr_manager: &QrTokenManager, ttl: u64) -> Result<String> {
    if auth_center_active(host_ctx) {
        match call_api(host_ctx, "com.bedcode.devices.qr-code-generate", serde_json::json!(ttl)) {
            Ok(v) => {
                return v
                    .get("token")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| AppError::Plugin("auth center qr-code-generate missing token".to_string()))
            }
            Err(e) => log_fallback("qr-code-generate", &e),
        }
    }
    Ok(qr_manager.generate(ttl).await)
}

/// 当前 QR token 连接信息 `(token, remaining_secs)`：认证中心激活 → 互调
/// `qr-code-status`；降级 → 宿主。host/port 组装（局域网 IP / 端口配置）留命令层
/// （宿主引擎配置域）。
pub async fn qr_conn_info(host_ctx: &WasmHostContext, qr_manager: &QrTokenManager) -> Result<Option<(String, u64)>> {
    if auth_center_active(host_ctx) {
        match call_api(host_ctx, "com.bedcode.devices.qr-code-status", serde_json::Value::Null) {
            Ok(v) => {
                if v.is_null() {
                    return Ok(None);
                }
                let token = v
                    .get("token")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| AppError::Plugin("auth center qr-code-status missing token".to_string()))?;
                let remaining = v.get("remaining").and_then(|r| r.as_u64()).unwrap_or(0);
                return Ok(Some((token, remaining)));
            }
            Err(e) => log_fallback("qr-code-status", &e),
        }
    }
    Ok(qr_manager
        .get_active()
        .await
        .map(|(token, _ttl, remaining)| (token, remaining)))
}

/// QR token 验证结果
pub enum QrVerifyOutcome {
    /// 验证通过（token 已消费）
    Valid,
    /// 拒绝：无效/过期/已用/未生成（携带分类原因文本，与宿主 `QrTokenManager`
    /// 错误消息同构——`QR token expired` 等，供 auth_controller 分类提示）
    Rejected(String),
}

/// 验证 QR token（一次性消费）：认证中心激活 → 互调 `qr-code-verify`（拒绝携带
/// reason 分类）；降级 → 宿主 `QrTokenManager::verify`（错误文本透传）。
pub async fn verify_qr_token(
    host_ctx: &WasmHostContext,
    qr_manager: &QrTokenManager,
    token: &str,
) -> Result<QrVerifyOutcome> {
    if auth_center_active(host_ctx) {
        match call_api(host_ctx, "com.bedcode.devices.qr-code-verify", serde_json::json!(token)) {
            Ok(v) => {
                let valid = v.get("valid").and_then(|b| b.as_bool()).unwrap_or(false);
                if valid {
                    return Ok(QrVerifyOutcome::Valid);
                }
                let reason = v
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("Invalid QR token")
                    .to_string();
                return Ok(QrVerifyOutcome::Rejected(reason));
            }
            Err(e) => log_fallback("qr-code-verify", &e),
        }
    }
    match qr_manager.verify(token).await {
        Ok(()) => Ok(QrVerifyOutcome::Valid),
        Err(e) => Ok(QrVerifyOutcome::Rejected(e.to_string())),
    }
}

/// 清除当前 QR token：认证中心激活 → 互调 `qr-code-clear`；降级 → 宿主。
pub async fn clear_qr_code(host_ctx: &WasmHostContext, qr_manager: &QrTokenManager) -> Result<()> {
    if auth_center_active(host_ctx) {
        match call_api(host_ctx, "com.bedcode.devices.qr-code-clear", serde_json::Value::Null) {
            Ok(_) => return Ok(()),
            Err(e) => log_fallback("qr-code-clear", &e),
        }
    }
    qr_manager.clear().await;
    Ok(())
}

/// QR 验证失败的用户提示（移动端可见）：分类与 auth_controller 既有 contains
/// 子串逻辑一致（`expired` / `already used` / `No active QR token`），单点维护。
pub fn qr_failure_user_message(reason: &str) -> &str {
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 插件 `pairing-code-generate/status` 返回 JSON 可直接映射宿主 `PairingCode`
    /// DTO（created_at RFC3339 秒级、expires_in 剩余秒；`created_instant` 反序列化
    /// 不可恢复 → chrono fallback 语义）
    #[test]
    fn pairing_code_json_maps_to_host_dto() {
        let v = serde_json::json!({
            "code": "123456",
            "created_at": "2026-09-19T08:00:00Z",
            "expires_in": 40,
        });
        let code: PairingCode = serde_json::from_value(v).expect("mapping");
        assert_eq!(code.code, "123456");
        assert_eq!(code.expires_in, 40);
        assert!(code.created_instant.is_none(), "反序列化走 chrono fallback");
        assert_eq!(
            code.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "2026-09-19T08:00:00Z"
        );
    }

    /// 配对码 JSON 损坏 → 显性错误（不静默）
    #[test]
    fn pairing_code_json_corrupt_fails_loudly() {
        let v = serde_json::json!({ "code": 123456, "created_at": 0 });
        assert!(serde_json::from_value::<PairingCode>(v).is_err());
    }

    #[test]
    fn qr_failure_user_message_classifies() {
        // 与 auth_controller 既有分类一致（子串匹配）
        assert_eq!(qr_failure_user_message("QR token expired"), "二维码已过期，请重新生成");
        assert_eq!(
            qr_failure_user_message("QR token already used"),
            "二维码已绑定其他设备，请重新扫描"
        );
        assert_eq!(qr_failure_user_message("No active QR token"), "请先在桌面端生成二维码");
        // 未命中分类：原文透传（宿主 Invalid QR token 等）
        assert_eq!(qr_failure_user_message("Invalid QR token"), "Invalid QR token");
    }

    /// QR status JSON → (token, remaining) 提取（桥接映射核心）
    #[test]
    fn qr_status_json_extracts_token_and_remaining() {
        let v = serde_json::json!({ "token": "ab12", "ttl": 300, "remaining": 123 });
        let token = v.get("token").and_then(|t| t.as_str()).unwrap();
        let remaining = v.get("remaining").and_then(|r| r.as_u64()).unwrap_or(0);
        assert_eq!(token, "ab12");
        assert_eq!(remaining, 123);
        // 缺 remaining → 0（防御性默认）
        let v2 = serde_json::json!({ "token": "ab12" });
        assert_eq!(v2.get("remaining").and_then(|r| r.as_u64()).unwrap_or(0), 0);
    }
}
