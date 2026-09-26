//! Session Control HTTP Client（票 04：控制面迁 HTTP）
//!
//! 会话起停删 / 会话列表 / 终端输入从 WS `Message` 信封迁到桌面 HTTP 面
//! （URL 与响应形状已由桌面冻结，见 `wasm-apps/terminal-session/sessions_http.rs`）：
//!
//! | 方法 | 端点 | 响应 data |
//! | --- | --- | --- |
//! | `list_sessions` | `GET /api/sessions` | `{sessions: SessionItem[]}` |
//! | `start_session` | `POST /api/sessions/start`（`{configId, cols?, rows?}`） | `{sessionId, status}` |
//! | `stop_session` | `POST /api/sessions/{id}/stop` | 无 data（`{code:0}`） |
//! | `remove_session` | `DELETE /api/sessions/{id}/remove` | 无 data（`{code:0}`） |
//! | `send_input` | `POST /api/sessions/{id}/input`（`{data, specialKey?}`） | 无 data（`{code:0}`） |
//!
//! 错误语义沿用桌面口径：**HTTP 200 + `{code:1002, message}`**（业务码而非 HTTP
//! 码）→ 按既有 `auth::http::parse_envelope` 规则映射 `AppError::Auth`；非 2xx /
//! 网络故障 → `AppError::Internal`（与 `AuthHttpClient` 三态收敛一致）。
//!
//! 传输：直连 reqwest（`no_proxy`，与 `AuthHttpClient` 同策略——系统代理会劫持
//! 局域网目标）；JWT 经 `Authorization: Bearer` 注入（`/api/auth/*` 白名单只属
//! 认证链路，本域非 auth 路径必带）。前端主调用面仍走 `http_proxy`（链路加密
//! 信封）；本客户端服务 Rust 侧调用面（命令层 / WASM host 输入），保持 JWT 语义
//! 一致即可。

use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::auth::http::{parse_envelope, resolve_base_url};
use crate::connection::manager::ConnectionManager;
use crate::connection::request::timeouts;
use crate::state::get_global_token;
use crate::{AppError, Result};

/// 会话列表响应 data（`{sessions: [...]}`）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListData {
    pub sessions: Vec<Value>,
}

/// 启动会话响应 data（`{sessionId, status}`）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionData {
    pub session_id: String,
    #[serde(default)]
    pub status: String,
}

/// 会话控制 HTTP 客户端
///
/// 与 `AuthHttpClient` 同构：持有 reqwest 连接池，不持 base URL——由调用方
/// 经 `resolve_base_url` 每请求解析，目标设备切换后立即生效。
pub struct SessionHttpClient {
    client: reqwest::Client,
}

impl SessionHttpClient {
    /// 创建客户端（默认连接池，无全局超时——每请求显式设置；`no_proxy` 理由
    /// 同 `AuthHttpClient`）
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("build session http client"),
        })
    }

    /// 会话列表（`GET /api/sessions`）
    pub async fn list_sessions(&self, base_url: &str) -> Result<Vec<Value>> {
        let url = format!("{}/api/sessions", base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(get_global_token())
            .timeout(timeouts::SESSION_CONTROL)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
        let text = response_text(resp, &url).await?;
        parse_session_list(&text)
    }

    /// 启动会话（`POST /api/sessions/start`），返回 sessionId
    pub async fn start_session(
        &self,
        base_url: &str,
        config_id: &str,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<String> {
        let url = format!("{}/api/sessions/start", base_url);
        let body = serde_json::json!({
            "configId": config_id,
            "cols": cols,
            "rows": rows,
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(get_global_token())
            .json(&body)
            .timeout(timeouts::SESSION_CONTROL)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
        let text = response_text(resp, &url).await?;
        parse_start_session_id(&text)
    }

    /// 停止会话（`POST /api/sessions/{id}/stop`）
    pub async fn stop_session(&self, base_url: &str, session_id: &str) -> Result<()> {
        let url = format!("{}/api/sessions/{}/stop", base_url, session_id);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(get_global_token())
            .timeout(timeouts::SESSION_CONTROL)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
        let text = response_text(resp, &url).await?;
        parse_ok_envelope(&text)
    }

    /// 删除会话（`DELETE /api/sessions/{id}/remove`）
    pub async fn remove_session(&self, base_url: &str, session_id: &str) -> Result<()> {
        let url = format!("{}/api/sessions/{}/remove", base_url, session_id);
        let resp = self
            .client
            .delete(&url)
            .bearer_auth(get_global_token())
            .timeout(timeouts::SESSION_CONTROL)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
        let text = response_text(resp, &url).await?;
        parse_ok_envelope(&text)
    }

    /// 写入终端输入（`POST /api/sessions/{id}/input`）
    ///
    /// `data` 为可打印文本（无控制字符）；`special_key` 为组合串（如
    /// `"ctrl+c"`），桌面端负责翻译。两者可同时缺席（空操作也发，桌面端照回
    /// 成功）。
    pub async fn send_input(
        &self,
        base_url: &str,
        session_id: &str,
        data: &str,
        special_key: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/api/sessions/{}/input", base_url, session_id);
        let body = serde_json::json!({
            "data": data,
            "specialKey": special_key,
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(get_global_token())
            .json(&body)
            .timeout(timeouts::SESSION_CONTROL)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP request to {} failed: {}", url, e)))?;
        let text = response_text(resp, &url).await?;
        parse_ok_envelope(&text)
    }
}

/// 读取响应体并把非 2xx 映射为 `AppError::Internal`（桌面端业务错误包 200 信封，
/// 非 2xx 属基础设施故障——与 `AuthHttpClient::post_and_parse` 同口径）
async fn response_text(resp: reqwest::Response, url: &str) -> Result<String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read response body from {}: {}", url, e)))?;
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "HTTP {} from {}: {}",
            status.as_u16(),
            url,
            text
        )));
    }
    Ok(text)
}

// ==================== 纯解析函数（单测锚点，无网络） ====================

/// 解析会话列表响应：`data.sessions`（code==0 时）
pub fn parse_session_list(body: &str) -> Result<Vec<Value>> {
    parse_envelope::<SessionListData>(body).map(|d| d.sessions)
}

/// 解析启动会话响应：`data.sessionId`（code==0 时）
pub fn parse_start_session_id(body: &str) -> Result<String> {
    parse_envelope::<StartSessionData>(body).map(|d| d.session_id)
}

/// 解析无 data 的成功信封（stop / remove / input）：code==0 → Ok；code!=0 →
/// `AppError::Auth`（透传桌面业务码）；缺失 data 不算违约（桌面 `ok()` 只回
/// `{code, message}`）。
pub fn parse_ok_envelope(body: &str) -> Result<()> {
    let envelope: crate::auth::http::ApiEnvelope<Value> =
        serde_json::from_str(body).map_err(|e| AppError::Parse(format!("Invalid API response JSON: {}", e)))?;
    if envelope.code == 0 {
        Ok(())
    } else {
        Err(AppError::Auth(format!("code {}: {}", envelope.code, envelope.message)))
    }
}

/// 解析目标设备 base URL（未 connect 时显性报错）
pub async fn session_base_url(conn: &ConnectionManager) -> Result<String> {
    resolve_base_url(conn).await
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 成功形状 ----------

    #[test]
    fn parse_session_list_ok() {
        let body = r#"{"code":0,"message":"ok","data":{"sessions":[{"id":"s1","status":"running"}]}}"#;
        let sessions = parse_session_list(body).expect("list parse");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["id"], "s1");
        assert_eq!(sessions[0]["status"], "running");
    }

    #[test]
    fn parse_start_session_id_ok() {
        let body = r#"{"code":0,"message":"ok","data":{"sessionId":"s-abc","status":"running"}}"#;
        assert_eq!(parse_start_session_id(body).expect("start parse"), "s-abc");
    }

    #[test]
    fn parse_ok_envelope_without_data_is_success() {
        // 桌面 `http_response::ok()` 只回 `{code:0,message}`，无 data——不算违约
        let body = r#"{"code":0,"message":"ok"}"#;
        assert!(parse_ok_envelope(body).is_ok());
    }

    #[test]
    fn parse_ok_envelope_with_data_is_success() {
        let body = r#"{"code":0,"message":"ok","data":{}}"#;
        assert!(parse_ok_envelope(body).is_ok());
    }

    // ---------- 失败语义（code!=0 → AppError::Auth，透传桌面业务码） ----------

    #[test]
    fn business_error_maps_to_auth_with_code() {
        // 桌面错误口径：HTTP 200 + `{code:1002, message}`（会话不存在等）
        let body = r#"{"code":1002,"message":"session not found"}"#;
        let err = parse_ok_envelope(body).expect_err("business error");
        match err {
            AppError::Auth(msg) => {
                assert!(msg.contains("1002"), "err 应携带业务码: {}", msg);
                assert!(msg.contains("session not found"), "err 应透传消息: {}", msg);
            }
            other => panic!("expected AppError::Auth, got {:?}", other),
        }
    }

    #[test]
    fn start_business_error_maps_to_auth() {
        let body = r#"{"code":1002,"message":"config not found"}"#;
        match parse_start_session_id(body).expect_err("business error") {
            AppError::Auth(msg) => assert!(msg.contains("1002") && msg.contains("config not found")),
            other => panic!("expected AppError::Auth, got {:?}", other),
        }
    }

    #[test]
    fn list_business_error_maps_to_auth() {
        let body = r#"{"code":1001,"message":"invalid token"}"#;
        match parse_session_list(body).expect_err("business error") {
            AppError::Auth(msg) => assert!(msg.contains("1001") && msg.contains("invalid token")),
            other => panic!("expected AppError::Auth, got {:?}", other),
        }
    }

    // ---------- 畸形 / 违约 ----------

    #[test]
    fn invalid_json_is_parse_error() {
        assert!(matches!(parse_ok_envelope("not-json"), Err(AppError::Parse(_))));
        assert!(matches!(parse_start_session_id("not-json"), Err(AppError::Parse(_))));
        assert!(matches!(parse_session_list("not-json"), Err(AppError::Parse(_))));
    }

    #[test]
    fn data_carrying_missing_data_is_parse_error() {
        // list / start 缺 data（协议违约）→ Parse，不是静默空
        let body = r#"{"code":0,"message":"ok"}"#;
        assert!(matches!(parse_session_list(body), Err(AppError::Parse(_))));
        assert!(matches!(parse_start_session_id(body), Err(AppError::Parse(_))));
    }

    #[test]
    fn session_list_data_type_mismatch_is_parse_error() {
        // data.sessions 给成对象而非数组 → 解析失败显性报错
        let body = r#"{"code":0,"message":"ok","data":{"sessions":{}}}"#;
        assert!(matches!(parse_session_list(body), Err(AppError::Parse(_))));
    }

    // ==================== 结构锁（票 04：旧信封协议零生产残留） ====================

    /// 旧 WS 信封命令名（票 04 删除清单）：src/ 生产代码零命中。
    /// 合法豁免：本测试文件自身（字面量）、注释行（文档说明删除缘由）。
    #[test]
    fn retired_envelope_command_names_have_no_production_hits() {
        const RETIRED: &[&str] = &[
            "ws_load_sessions",
            "ws_send_input_async",
            "get_terminal_ws_info",
            "ws_load_session_configs",
            "ws_join_session",
            "ws_resize_terminal",
            "ws_send_message",
            "ws_send_and_wait",
        ];
        let root = env!("CARGO_MANIFEST_DIR");
        let src_dir = std::path::Path::new(root).join("src");
        let mut checked = 0;
        let mut dirs = vec![src_dir.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("read dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // 本测试文件自身含字面量，跳过
                if path.file_name().and_then(|n| n.to_str()) == Some("http.rs")
                    && path.parent().and_then(|p| p.file_name().and_then(|n| n.to_str())) == Some("session")
                {
                    continue;
                }
                let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                checked += 1;
                for (idx, raw) in src.lines().enumerate() {
                    let line = raw.trim_start();
                    if line.starts_with("//") {
                        continue;
                    }
                    for name in RETIRED {
                        assert!(
                            !line.contains(name),
                            "{}:{} 旧信封命令名残留: {}",
                            path.display(),
                            idx + 1,
                            line.trim()
                        );
                    }
                }
            }
        }
        assert!(checked > 20, "structure lock 应扫描到全仓 Rust 文件，实际 {checked}");
    }

    /// `Message::` / 旧请求构建器在控制面迁移文件实现段零使用（信封已退役）。
    /// 保留面：`model/message.rs`（枚举本体）、`connection/request.rs`
    /// （AuthRequest，集成测试用）、`connection/{codec,request_response}.rs`、
    /// `router/{registry,router}.rs`、`handler/{auth,system}.rs`（协议级收尾）。
    #[test]
    fn migrated_control_plane_has_no_envelope_usage() {
        let root = env!("CARGO_MANIFEST_DIR");
        for file in [
            "src/session.rs",
            "src/session/http.rs",
            "src/commands/session.rs",
            "src/plugin/wasm_runtime/host_impl/terminal.rs",
        ] {
            let path = format!("{root}/{file}");
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            let impl_part = src.split("\n#[cfg(test)]").next().unwrap_or(&src);
            for (idx, raw) in impl_part.lines().enumerate() {
                let line = raw.trim_start();
                if line.starts_with("//") {
                    continue;
                }
                for token in [
                    "Message::",
                    "SessionRequest::",
                    "TerminalRequest::",
                    "ConfigRequest::",
                    "ResponseParser::",
                ] {
                    assert!(
                        !line.contains(token),
                        "{file}:{} 控制面实现段不得出现信封引用 `{token}`: {}",
                        idx + 1,
                        line.trim()
                    );
                }
            }
        }
    }

    /// 结构锁：WS 帧级链路加密退役后（票 06，桌面 `TrafficChannel::WsPlugin => false`），
    /// 生产 src/ 下不得回接任何 WS 加密符号/通道名。旁路任一会转红（变异自检：
    /// 在 ws_client.rs 把 `ClientWsCrypto` 注释回一行即可验证锁有效）。
    #[test]
    fn ws_link_crypto_has_no_production_residue() {
        const RETIRED_WS_CRYPTO: &[&str] = &[
            "install_link_crypto",
            "install_event_crypto",
            "extract_crypto_echo",
            "EVENT_WS_AUTH_TIMEOUT_MS",
            "LINK_CRYPTO_CHANNEL_EVENT",
            "is_event_encryption_active",
            "encrypt_ws_event",
            "ClientWsCrypto",
        ];
        let root = env!("CARGO_MANIFEST_DIR");
        let src_dir = std::path::Path::new(root).join("src");
        let mut dirs = vec![src_dir.clone()];
        let mut checked = 0;
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("read dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // 本测试文件自身含字面量（RETIRED_WS_CRYPTO），跳过
                if path.file_name().and_then(|n| n.to_str()) == Some("http.rs")
                    && path.parent().and_then(|p| p.file_name().and_then(|n| n.to_str())) == Some("session")
                {
                    continue;
                }
                let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                for raw in src.lines() {
                    let line = raw.trim_start();
                    if line.starts_with("//") {
                        continue;
                    }
                    // 注释块里的 <!-- 「ws-event」字面量也属退役通道名，一并锁（但仅禁代码/注释
                    // 里的匹配——文档另见 code-map）
                    for token in RETIRED_WS_CRYPTO {
                        assert!(
                            !line.contains(token),
                            "{}: WS 帧级加密已退役，不得回接 `{token}`: {}",
                            path.display(),
                            line.trim()
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 50, "结构锁扫描范围异常：仅扫到 {checked} 个 .rs 文件");
    }
}
