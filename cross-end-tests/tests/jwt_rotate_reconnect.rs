//! 场景 2：入场密钥轮换后的宽限期（ADR 0033 密钥环）
//!
//! 覆盖的盲区：**轮换时序**。ADR 0033 起入场签发密钥归认证中心自持，密钥环最多
//! 两代：轮换不撤销既有 token，上一代在宽限期内继续可验签。这条「不撤销」语义
//! 此前只在认证中心插件自己的单测里成立——**没有任何测试从客户端一侧证明
//! 「轮换前拿到的 token 在轮换后还能连上」**。本文件从移动端真实客户端侧证明。
//!
//! ## 行为契约
//!
//! | 契约 | 来源 | 行为 | 场景 |
//! |---|---|---|---|
//! | S-001 | 配对签发 + `handle_reauth` | 轮换前 token 可换发（前置状态） | 正例 |
//! | S-002 | `auth-grant jwt/rotate-key` → `rotate_signing_key` | 轮换返回 `{rotated:true, kid, previousKid}`，kid 推进 | 正例 |
//! | S-003 | 密钥环逐代尝试 + 宽限期 | **轮换前的旧 token 仍可换发**（不撤销） | 正例（核心） |
//! | S-004 | `issue_device_token` 带 active kid | 轮换后新签发的 token 带新 `kid` | 正例 |
//! | S-005 | WS 端点同一闸门 | 轮换前拿到的 token 仍能过 WS 端点认证（链路进 live） | 正例（跨端时序） |
//!
//! spec §6-7 顾虑的「rotate-key 需宿主命令面接线」在本场景不成立：轮换触发面是
//! 插件互调 `auth-grant`（插件.json 已声明该 api），无头装配直接驱动即可。

mod common;

use std::sync::Arc;
use std::time::Duration;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::terminal_link_manager;

use common::desktop_ctx;
use common::mobile_ctx;

/// 中心侧 `auth-grant` 互调（`method` = 注册的认证方式，`params` = 该方式入参）
///
/// api 声明为双参 `(method, params)` → SDK 宏按元组反序列化，故 params 传**数组**。
async fn auth_grant(method: &str, params: serde_json::Value) -> serde_json::Value {
    desktop_ctx::plugin_api_call(
        "com.bedcode.terminal-session.auth-grant",
        serde_json::json!([method, params]),
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn key_rotation_keeps_previous_token_usable_from_mobile() {
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    desktop_ctx::init_app_context().await;
    let (port, handle, server_task) = desktop_ctx::start_server().await;
    mobile_ctx::set_target(port).await;
    let base = mobile_ctx::base_url(port);
    let address = format!("127.0.0.1:{port}");

    // ==================== S-001 轮换前：配对 + 换发 ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "crossend-rotate-device",
        device_name: "CrossEnd Rotate",
        fingerprint: "crossend-rotate-fp",
        uid_hash: None,
    };
    let pairing = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect("配对码签发");
    let pre_rotation = auth
        .verify_pairing_code(&base, ctx, &pairing.pairing_code, &address)
        .await
        .expect("配对码换 token")
        .token;
    mobile_ctx::remember_token(&pre_rotation);
    auth.reauth(&base, ctx.device_id, ctx.fingerprint, ctx.uid_hash, &pre_rotation)
        .await
        .expect("S-001 轮换前 token 必须可换发");
    let pre_kid = auth_grant("jwt", serde_json::json!({ "action": "verify", "token": pre_rotation })).await["kid"]
        .as_str()
        .expect("S-001 轮换前 token 应带 kid")
        .to_string();

    // ==================== S-002 轮换入场签发密钥 ====================
    let rotated = auth_grant("jwt", serde_json::json!({ "action": "rotate-key" })).await;
    assert_eq!(rotated["rotated"], true, "S-002 轮换应显式报告成功");
    let new_kid = rotated["kid"].as_str().expect("S-002 轮换结果应带新 kid").to_string();
    assert_ne!(new_kid, pre_kid, "S-002 轮换后 kid 必须推进");
    assert_eq!(
        rotated["previousKid"].as_str(),
        Some(pre_kid.as_str()),
        "S-002 轮换结果应点名上一代 kid（密钥环两代的证据）"
    );

    // ==================== S-003 旧 token 仍在宽限期内（不撤销） ====================
    let reauthed = auth
        .reauth(&base, ctx.device_id, ctx.fingerprint, ctx.uid_hash, &pre_rotation)
        .await
        .unwrap_or_else(|e| panic!("S-003 轮换前 token 必须仍在宽限期内可验签，got {e}"));
    assert!(!reauthed.token.is_empty(), "S-003 换发 token 不得为空");
    // 换发产物应带**新** kid（签发侧已切到新一代密钥）
    let reauthed_kid = auth_grant(
        "jwt",
        serde_json::json!({ "action": "verify", "token": reauthed.token }),
    )
    .await["kid"]
        .as_str()
        .expect("S-003 换发 token 应带 kid")
        .to_string();
    assert_eq!(
        reauthed_kid, new_kid,
        "S-003 轮换后换发的 token 必须用新一代密钥签发（签发侧已切换）"
    );

    // ==================== S-005 轮换前的 token 仍能过 WS 端点认证 ====================
    // 这是本场景的跨端核心：客户端持有的是「轮换前」那张 token，桌面端必须放它进。
    let sessions = SessionHttpClient::new();
    let config_id = desktop_ctx::seed_shell_config("cross-end-rotate").await;
    let session_id = sessions
        .start_session(&base, &config_id, Some(100), Some(30))
        .await
        .expect("S-005 启动会话");

    // 故意把全局 token 换回**轮换前**那张，模拟「设备没重新认证就重连」
    mobile_ctx::remember_token(&pre_rotation);
    let recorder = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs = Arc::new(mobile_ctx::OutputRecorder::default());
    terminal_link_manager().page_subscribe(&session_id, mobile_ctx::output_channel(outputs.clone()));
    terminal_link_manager().subscribe(recorder.clone(), session_id.clone());
    mobile_ctx::wait_event(&recorder, "S-005 轮换前 token 连上 WS 端点", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;
    // 端点认证放行 = 真数据可流：让真实 PTY 产出一段输出
    sessions
        .send_input(&base, &session_id, "echo ROTATE_GRACE_OK\n", None)
        .await
        .expect("S-005 写入输入");
    mobile_ctx::wait_output(&outputs, "ROTATE_GRACE_OK").await;

    // ==================== 收尾 ====================
    sessions.stop_session(&base, &session_id).await.expect("收尾停止会话");
    terminal_link_manager().remove(&session_id);
    tokio::time::sleep(Duration::from_millis(200)).await;
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
