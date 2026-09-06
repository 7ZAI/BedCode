//! HTTP 认证客户端（移动端 → 桌面端）
//!
//! 认证已从 WS 握手迁移到 HTTP（spec §4.5：配对 / 验码 / QR / reauth / 生物
//! 六端点），JWT 由 Rust 持有（D3）。本模块封装：
//! - 桌面端 `ApiResponse` 统一信封解析（与 `common_dto.rs` 逐字段对称）
//! - 六端点的不可变请求体与响应 data DTO（camelCase）
//! - 目标设备 base URL 解析（复用 `ConnectionManager.target`）
//!
//! 桌面端业务错误统一包 200 + `{code, message}` 信封返回（如 1005 配对码错误、
//! 1008 挑战签发失败、1009 验签失败），本模块把非 0 的业务码映射为
//! `AppError::Auth("code {code}: {message}")`，传输层故障（拒绝/超时）映射为
//! `AppError::Internal`——调用方可按变体区分「桌面端拒绝」与「网络不可达」。

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use crate::connection::manager::ConnectionManager;
use crate::connection::request::timeouts;
use crate::{AppError, Result};

/// 桌面端 HTTP API 统一响应信封
///
/// 与 `bedcode-desktop/src-tauri/src/server/dtos/common_dto.rs::ApiResponse`
/// 对称：`code`（0=成功，非 0=业务错误）+ `message` + `data`（成功负载）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEnvelope<T> {
    pub code: u16,
    pub message: String,
    pub data: Option<T>,
}

/// POST /api/auth/pairing 响应 data
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResponseData {
    pub pairing_code: String,
    pub expires_in: u64,
}

/// token 响应 data（verify / qr-connect / reauth / biometric-verify 共用）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokenResponseData {
    pub token: String,
    pub expires_in: u64,
    /// 桌面端链路加密身份公钥（base64；老桌面端未下发时省略，serde 默认 None）
    #[serde(default)]
    pub kd_public_b64: Option<String>,
    /// 桌面端身份指纹（SHA-256 前 16 hex，供设置页人工核对）
    #[serde(default)]
    pub kd_fingerprint: Option<String>,
}

/// POST /api/auth/biometric-challenge 响应 data
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricChallengeResponseData {
    pub challenge_nonce: String,
    pub expires_in: u64,
}

/// 从目标设备地址拼 HTTP base URL
///
/// 桌面端服务监听明文 HTTP（无 TLS），address 为局域网 IP。端口复用
/// `TargetDevice.port`（WS/HTTP 同端口，见桌面端 app.rs 单端口服务）。
pub fn format_base_url(address: &str, port: u16) -> String {
    format!("http://{}:{}", address, port)
}

/// 解析目标设备 base URL
///
/// target 缺失（未 connect）说明尚未选定桌面端，认证请求无从发起。
pub async fn resolve_base_url(conn: &ConnectionManager) -> Result<String> {
    let target = conn
        .get_target()
        .await
        .ok_or_else(|| AppError::Auth("No target device".to_string()))?;
    Ok(format_base_url(&target.address, target.port))
}

/// 解析桌面端 HTTP 响应信封，取出 `data`（code==0 时）
///
/// - code==0：返回 data；data 缺失视为协议违约（`AppError::Parse`）
/// - code!=0：`AppError::Auth("code {code}: {message}")`，透传桌面业务码
///   （1001/1005/1006/1007/1008/1009），与桌面 `ApiResponse::error` 语义一致
/// - 非法 JSON：`AppError::Parse`
pub fn parse_envelope<T: DeserializeOwned>(body: &str) -> Result<T> {
    let envelope: ApiEnvelope<T> =
        serde_json::from_str(body).map_err(|e| AppError::Parse(format!("Invalid API response JSON: {}", e)))?;
    if envelope.code == 0 {
        envelope
            .data
            .ok_or_else(|| AppError::Parse(format!("API response code=0 but data missing: {}", body)))
    } else {
        Err(AppError::Auth(format!("code {}: {}", envelope.code, envelope.message)))
    }
}

/// HTTP 认证客户端
///
/// 持有 reqwest 连接池；每个方法按端点语义带超时（普通认证 30s，
/// 生物认证含系统生物识别弹窗 120s）。不持有 base URL——由调用方
/// 经 `resolve_base_url` 每请求解析，保证目标设备切换（重新 connect）
/// 后立即生效。
pub struct AuthHttpClient {
    client: reqwest::Client,
}

impl AuthHttpClient {
    /// 创建客户端（默认连接池，无全局超时——每请求显式设置）
    ///
    /// 显式 `no_proxy()`：移动端只直连局域网桌面端（HTTP 明文、单端口），
    /// 本机系统代理会把局域网目标也劫持走（实测回环 127.0.0.1:1 被抓代理
    /// 返回 502），认证会全部被打断；与桌面端 reqwest 显式处理代理一致。
    pub fn new() -> Arc<Self> {
        Self::with_client(reqwest::Client::builder().no_proxy().build().expect("build client"))
    }

    /// 用自定义 reqwest Client 构造
    ///
    /// 测试注入 `no_proxy` 等行为用（系统代理会把「连接拒绝」伪装成 502）。
    pub fn with_client(client: reqwest::Client) -> Arc<Self> {
        Arc::new(Self { client })
    }

    /// POST /api/auth/pairing：发起配对，桌面端返回一次性配对码
    pub async fn request_pairing(
        &self,
        base_url: &str,
        device_id: &str,
        device_name: &str,
        fingerprint: &str,
    ) -> Result<PairingResponseData> {
        let url = format!("{}/api/auth/pairing", base_url);
        let body = json!({
            "deviceId": device_id,
            "deviceName": device_name,
            "fingerprint": fingerprint,
        });
        post_and_parse(&self.client, url, body, timeouts::AUTH).await
    }

    /// POST /api/auth/verify：验证配对码，签发 JWT
    ///
    /// `address` 必填：桌面端把客户端局域网地址写入配对记录
    /// （`VerifyPairingRequest.address`），取 `TargetDevice.address`。
    pub async fn verify_pairing_code(
        &self,
        base_url: &str,
        device_id: &str,
        device_name: &str,
        fingerprint: &str,
        pairing_code: &str,
        address: &str,
    ) -> Result<AuthTokenResponseData> {
        let url = format!("{}/api/auth/verify", base_url);
        let body = json!({
            "deviceId": device_id,
            "deviceName": device_name,
            "fingerprint": fingerprint,
            "pairingCode": pairing_code,
            "address": address,
        });
        post_and_parse(&self.client, url, body, timeouts::AUTH).await
    }

    /// POST /api/auth/qr-connect：QR 码认证，签发 JWT
    pub async fn qr_connect(
        &self,
        base_url: &str,
        device_id: &str,
        device_name: &str,
        fingerprint: &str,
        qr_token: &str,
        address: &str,
    ) -> Result<AuthTokenResponseData> {
        let url = format!("{}/api/auth/qr-connect", base_url);
        let body = json!({
            "deviceId": device_id,
            "deviceName": device_name,
            "fingerprint": fingerprint,
            "qrToken": qr_token,
            "address": address,
        });
        post_and_parse(&self.client, url, body, timeouts::AUTH).await
    }

    /// POST /api/auth/reauth：JWT 重认证，签发刷新后的新 token
    ///
    /// token 走 body（桌面端从 body 验 token），不需要 Authorization 头——
    /// 与前端 `useHttpApi.ts` 对 `/api/auth/` 排除 Bearer 注入的约定一致。
    pub async fn reauth(
        &self,
        base_url: &str,
        device_id: &str,
        fingerprint: &str,
        session_token: &str,
    ) -> Result<AuthTokenResponseData> {
        let url = format!("{}/api/auth/reauth", base_url);
        let body = json!({
            "deviceId": device_id,
            "fingerprint": fingerprint,
            "sessionToken": session_token,
        });
        post_and_parse(&self.client, url, body, timeouts::AUTH).await
    }

    /// POST /api/auth/biometric-challenge：请求一次性挑战值
    ///
    /// 失败返回 1008（未配对 / 未绑定生物凭证 / 凭证过期）。
    pub async fn biometric_challenge(
        &self,
        base_url: &str,
        device_id: &str,
        fingerprint: &str,
    ) -> Result<BiometricChallengeResponseData> {
        let url = format!("{}/api/auth/biometric-challenge", base_url);
        let body = json!({
            "deviceId": device_id,
            "deviceFingerprint": fingerprint,
        });
        post_and_parse(&self.client, url, body, timeouts::BIO_AUTH).await
    }

    /// POST /api/auth/biometric-verify：回传挑战值签名，验签通过后签发 JWT
    ///
    /// 失败返回 1009（挑战过期 / 验签失败 / 未配对）。
    pub async fn biometric_verify(
        &self,
        base_url: &str,
        device_id: &str,
        fingerprint: &str,
        nonce: &str,
        signature: &str,
    ) -> Result<AuthTokenResponseData> {
        let url = format!("{}/api/auth/biometric-verify", base_url);
        let body = json!({
            "deviceId": device_id,
            "deviceFingerprint": fingerprint,
            "challengeNonce": nonce,
            "signature": signature,
        });
        post_and_parse(&self.client, url, body, timeouts::BIO_AUTH).await
    }
}

/// POST 并解析信封（transport 错误 / 非 2xx / 信封解析三态收敛）
async fn post_and_parse<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: String,
    body: serde_json::Value,
    timeout: Duration,
) -> Result<T> {
    let resp = client
        .post(&url)
        .json(&body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read response body from {}: {}", url, e)))?;
    // 桌面端业务错误包 200 信封；非 2xx 属基础设施故障（404/500），单独归 Internal
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "HTTP {} from {}: {}",
            status.as_u16(),
            url,
            text
        )));
    }
    parse_envelope(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn parse_envelope_ok_takes_data() {
        let body = r#"{"code":0,"message":"ok","data":{"pairingCode":"123456","expiresIn":300}}"#;
        let data: PairingResponseData = parse_envelope(body).expect("envelope parse");
        assert_eq!(data.pairing_code, "123456");
        assert_eq!(data.expires_in, 300);
    }

    #[test]
    fn parse_envelope_business_error_maps_to_auth() {
        let body = r#"{"code":1005,"message":"Invalid or expired pairing code"}"#;
        let err = parse_envelope::<AuthTokenResponseData>(body).expect_err("business error");
        match err {
            AppError::Auth(msg) => {
                assert!(msg.contains("1005"), "err 应携带业务码: {}", msg);
                assert!(
                    msg.contains("Invalid or expired pairing code"),
                    "err 应透传消息: {}",
                    msg
                );
            }
            other => panic!("expected AppError::Auth, got {:?}", other),
        }
    }

    #[test]
    fn parse_envelope_missing_data_is_parse_error() {
        let body = r#"{"code":0,"message":"ok"}"#;
        assert!(matches!(
            parse_envelope::<AuthTokenResponseData>(body),
            Err(AppError::Parse(_))
        ));
    }

    #[test]
    fn parse_envelope_invalid_json_is_parse_error() {
        assert!(matches!(
            parse_envelope::<AuthTokenResponseData>("not-json"),
            Err(AppError::Parse(_))
        ));
    }

    #[test]
    fn format_base_url_joins_address_and_port() {
        assert_eq!(format_base_url("192.168.1.5", 8765), "http://192.168.1.5:8765");
    }

    #[tokio::test]
    async fn resolve_base_url_without_target_errors() {
        let conn = ConnectionManager::new();
        let err = resolve_base_url(&conn).await.expect_err("no target");
        match err {
            AppError::Auth(msg) => assert!(msg.contains("No target device"), "err: {}", msg),
            other => panic!("expected Auth error, got {:?}", other),
        }
    }

    #[test]
    fn client_is_constructible() {
        let client = AuthHttpClient::new();
        assert_eq!(Arc::strong_count(&client), 1);
    }
}
