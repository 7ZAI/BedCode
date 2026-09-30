//! 场景 6：插件停用 / 激活联动（跨端生命周期）
//!
//! 覆盖的盲区：**桌面端插件生命周期对移动端连接的影响**。两端既有测试都把
//! 插件视为常驻（桌面测试激活后不再停），移动端测试对面是假端点（不存在
//! 「插件没了」这回事）。真实链路里桌面端用户随时可能停用某个 wasm 应用，
//! 移动端正在跑的会话与终端流会怎样，必须跨端钉死。
//!
//! 期望语义（ADR 0031 + 生命周期闸门 + fail-visible 三形态 ①）：
//! 停用 = 认证中心角色回收 + WS 资源回收 + 端点注销 → 移动端**显性**失败，
//! 而不是「静默拿到空数据」；重新激活后**不丢**已配对身份（认证记录与密钥环
//! 都不在停用清理范围内）。
//!
//! ## 行为契约
//!
//! | 契约 | 来源 | 行为 | 场景 |
//! |---|---|---|---|
//! | L-001 | 正常链路 | 建会话 + 终端链路 live（前置状态） | 正例 |
//! | L-002 | `deactivate_plugin` 回收 WS 资源 | 移动端链路掉线并退避重连（`reconnecting`） | 正例 |
//! | L-003 | 端点注销（fail-visible ①） | 停用后受保护端点显性失败（不返回空数据） | 反例 |
//! | L-004 | 重新激活 | 端点恢复可达，**同一 token 仍然有效**（停用不丢已配对身份） | 正例 |

mod common;

use std::sync::Arc;
use std::time::Duration;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::terminal_link_manager;

use common::desktop_ctx;
use common::mobile_ctx;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plugin_deactivation_is_visible_to_the_mobile_side() {
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

    // ==================== L-001 前置：真会话 + live 链路 ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "crossend-lifecycle-device",
        device_name: "CrossEnd Lifecycle",
        fingerprint: "crossend-lifecycle-fp",
        uid_hash: None,
    };
    let pairing = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect("配对码签发");
    let token = auth
        .verify_pairing_code(&base, ctx, &pairing.pairing_code, &address)
        .await
        .expect("配对码换 token")
        .token;
    mobile_ctx::remember_token(&token);

    let sessions = SessionHttpClient::new();
    let config_id = desktop_ctx::seed_shell_config("cross-end-lifecycle").await;
    let session_id = sessions
        .start_session(&base, &config_id, Some(90), Some(25))
        .await
        .expect("L-001 启动会话");

    let recorder = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs = Arc::new(mobile_ctx::OutputRecorder::default());
    terminal_link_manager().page_subscribe(&session_id, mobile_ctx::output_channel(outputs.clone()));
    terminal_link_manager().subscribe(recorder.clone(), session_id.clone());
    mobile_ctx::wait_event(&recorder, "L-001 链路 live", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;

    // ==================== 桌面端停用认证中心插件 ====================
    let host_ctx = bedcode_desktop_lib::system::app_context::AppContext::global();
    host_ctx
        .plugin_host()
        .deactivate_plugin(desktop_ctx::SESSION_PLUGIN_ID, false)
        .await
        .expect("L-002 停用认证中心插件");

    // ==================== L-002 移动端链路掉线并退避重连 ====================
    mobile_ctx::wait_event(&recorder, "L-002 掉线后的重连事件", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("reconnecting"))
    })
    .await;

    // ==================== L-003 端点注销：显性失败而非空数据 ====================
    let after_stop = sessions.list_sessions(&base).await;
    assert!(
        after_stop.is_err(),
        "L-003 停用后受保护端点必须显性失败（fail-visible ①），实际返回 Ok：{after_stop:?}"
    );

    // ==================== L-004 重新激活：端点恢复 + 同一 token 仍有效 ====================
    desktop_ctx::activate_session_center().await;
    // 端点恢复后，同一 token 必须仍被中心认（认证记录与密钥环都不在停用清理范围内）
    let reauthed = auth
        .reauth(&base, ctx.device_id, ctx.fingerprint, ctx.uid_hash, &token)
        .await
        .expect("L-004 重新激活后同一 token 必须仍可验签（停用不得丢已配对身份）");
    assert!(!reauthed.token.is_empty(), "L-004 换发 token 不得为空");
    // 停用前建的会话亦被回收：这是显式停用的预期语义，移动端应看到列表变化
    // （这里只断言端点恢复可达，不断言会话仍在——会话真源随插件实例停用而失效）
    let list = sessions
        .list_sessions(&base)
        .await
        .expect("L-004 重新激活后端点必须恢复可达");
    assert!(
        !list.iter().any(|s| s["id"] == session_id),
        "L-004 停用/激活后不得凭空多出会话（停用前的会话应随实例停用消失），list={list:#?}"
    );

    // ==================== 收尾 ====================
    terminal_link_manager().remove(&session_id);
    tokio::time::sleep(Duration::from_millis(200)).await;
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
