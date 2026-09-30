//! 场景 3：会话控制 HTTP 面（移动端真实客户端 ↔ 桌面真实插件）
//!
//! 覆盖的盲区：**HTTP 面契约失真**。移动端既有测试对面是假桌面服务器（响应
//! 由本仓按文档手写），桌面端既有测试对面是 reqwest（请求由本仓手写）——
//! 请求体字段名、路由模板、错误信封三者的真实一致性从未被两端同时跑过。
//!
//! ## 行为契约
//!
//! | 契约 | 来源 | 行为 | 场景 |
//! |---|---|---|---|
//! | C-001 | `SessionHttpClient::start_session` + 插件 `start_session` | `{configId,cols,rows}` → 建真会话 → 非空 sessionId | 正例 |
//! | C-002 | 插件 `list_sessions` | 移动端列表里查得到该会话（登记域真源） | 正例 |
//! | C-003 | `stop_session` → `pty:exit` 终态 | 轮询到该会话 `status != running`（终态由插件异步收尾） | 正例 |
//! | C-004 | `remove_session` | 移除后列表不再含该会话 | 正例 |
//! | C-005 | 插件 `sessions_error`（`{code:1002}`） | 未知 configId 启动 → 1002 → 移动端 `AppError::Auth` 且消息含 1002 | 反例 |
//! | C-006 | 插件 `sessions_error`（`{code:1002}`）与 `actions::remove_via_host` 幂等语义 | 对不存在的会话：`input` / `stop` 严格报 1002，`remove` **幂等成功**且不产生副作用 | 反例 + 不对称 |
//! | C-007 | `send_input(..., special_key)` | 真实会话上带特殊键写入被桌面接受（键写路径无错） | 正例 |
//!
//! 特殊键的**效果**断言（`ctrl+c` 真的打断了前台进程）在
//! `terminal_ws_flow.rs`——它才有终端输出可看。

mod common;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::system::error::AppError;

use common::desktop_ctx;
use common::mobile_ctx;

/// 断言桌面业务码 1002 到达移动端时映射为 `AppError::Auth` 且消息含码
fn assert_session_error_1002(err: AppError, what: &str) {
    match err {
        AppError::Auth(msg) => assert!(
            msg.contains("1002"),
            "{what}：错误消息应携带桌面会话域业务码 1002，got: {msg}"
        ),
        other => panic!("{what}：会话域业务拒绝应映射为 AppError::Auth，got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_http_surface_matches_real_desktop() {
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

    // ==================== 认证 ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "crossend-session-device",
        device_name: "CrossEnd Sessions",
        fingerprint: "crossend-session-fp",
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
    // 会话域端点是 JWT 档位：移动端客户端从全局 token 取 Bearer
    mobile_ctx::remember_token(&token);

    let sessions = SessionHttpClient::new();
    let config_id = desktop_ctx::seed_shell_config("cross-end-sessions").await;

    // ==================== C-005 未知 configId → 1002（先跑反例，避免污染后续） ====================
    let bad_start = sessions
        .start_session(&base, "config-does-not-exist", None, None)
        .await
        .expect_err("C-005 未知 configId 必须被桌面拒绝");
    assert_session_error_1002(bad_start, "C-005 未知 configId");

    // ==================== C-001 建会话 ====================
    let session_id = sessions
        .start_session(&base, &config_id, Some(100), Some(30))
        .await
        .expect("C-001 启动会话");
    assert!(!session_id.is_empty(), "C-001 sessionId 不得为空");

    // ==================== C-002 列表可查 ====================
    let list = sessions.list_sessions(&base).await.expect("C-002 读列表");
    let entry = list
        .iter()
        .find(|s| s["id"] == session_id)
        .unwrap_or_else(|| panic!("C-002 会话应出现在列表，list={list:#?}"));
    assert_eq!(entry["status"], "running", "C-002 新建会话应为 running");

    // ==================== C-007 特殊键写入被接受 ====================
    sessions
        .send_input(&base, &session_id, "", Some("ctrl+c"))
        .await
        .expect("C-007 真实会话上带 specialKey 的写入必须被桌面接受");

    // ==================== C-006 不存在的会话：stop/input 严格、remove 幂等 ====================
    // 这条**不对称**本身就是必须跨端钉死的契约：两端若对「remove 不存在的会话」
    // 的判断不一致，移动端会把自己的重试逻辑写错。
    let ghost = "00000000-0000-0000-0000-0000000000ff";
    let before = sessions.list_sessions(&base).await.expect("C-006 取基线列表");
    assert_session_error_1002(
        sessions
            .send_input(&base, ghost, "x", None)
            .await
            .expect_err("C-006 对不存在会话写输入必须被拒"),
        "C-006 不存在会话 input",
    );
    assert_session_error_1002(
        sessions
            .stop_session(&base, ghost)
            .await
            .expect_err("C-006 对不存在会话停止必须被拒"),
        "C-006 不存在会话 stop",
    );
    // 移除是**幂等**的（插件 actions::remove_via_host 注释：存在性宽容，移动端移除
    // 已消失会话不应报错）——所以这里断言 Ok + 列表不出现任何副作用
    sessions
        .remove_session(&base, ghost)
        .await
        .expect("C-006 移除不存在的会话必须幂等成功（不是错误）");
    let after_ghost = sessions.list_sessions(&base).await.expect("C-006 幂等移除后读列表");
    assert_eq!(
        after_ghost.len(),
        before.len(),
        "C-006 幂等移除不得凭空增删会话（before={} after={}）",
        before.len(),
        after_ghost.len()
    );
    assert!(
        !after_ghost.iter().any(|s| s["id"] == ghost),
        "C-006 幂等移除不得造出幽灵会话条目"
    );

    // ==================== C-003 停止 → 终态 ====================
    sessions.stop_session(&base, &session_id).await.expect("C-003 停止会话");
    // 终态由插件 pty:exit 事件异步收尾：轮询列表直到该会话不再 running
    let mut terminal_status = None;
    for _ in 0..100 {
        let list = sessions.list_sessions(&base).await.expect("C-003 终态轮询读列表");
        if let Some(entry) = list.iter().find(|s| s["id"] == session_id) {
            let status = entry["status"].as_str().unwrap_or("").to_string();
            if status != "running" {
                terminal_status = Some(status);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        terminal_status.as_deref(),
        Some("stopped"),
        "C-003 停止后会话终态必须是 stopped（pty:exit 收尾）"
    );

    // ==================== C-004 移除 → 列表不再含 ====================
    sessions
        .remove_session(&base, &session_id)
        .await
        .expect("C-004 移除会话");
    let after = sessions.list_sessions(&base).await.expect("C-004 移除后读列表");
    assert!(
        !after.iter().any(|s| s["id"] == session_id),
        "C-004 移除后会话不得仍在列表，list={after:#?}"
    );

    // ==================== 收尾 ====================
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
