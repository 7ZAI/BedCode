//! 场景 5：fail-closed 失败路径（无中心在册 / 非法凭证）
//!
//! 覆盖的盲区：**认证边界的真实行为**。两端既有测试都只测「中心在册且放行」
//! 这一条正向路径；ADR 0031 的 fail-closed 语义（无中心 / 中心不可用 / 中心
//! 拒绝 → 一律拒绝，无降级）在真实互连下没有任何一侧测过。
//!
//! ## 关于 `deny_kind` 三态的可观测性（spec §5 与现实的一处偏差，记录在案）
//!
//! 宿主的 `deny_kind`（`no_center` / `unavailable` / `policy`）是**日志结构化
//! 字段**，不是 wire 字段：客户端只能看到「一律 401」。这是**有意的**——把拒绝
//! 原因分类暴露给客户端会泄露部署信息。因此本文件断言的是客户端真正能观测的
//! 契约：
//!
//! - 一律显性拒绝（401 / 404），不是「查不到就当没数据」的静默降级；
//! - 放行时才返回数据，且必须**同时**满足「中心在册 + 凭证有效」；
//! - WS 面同样 fail-closed：无效凭证下链路不得进入 live。
//!
//! 三态分类本身的覆盖在宿主侧 `utils/auth/auth_center` 的单测里（不在本工程
//! 职责内，也不该在跨端层重复造 mock）。
//!
//! ## 行为契约
//!
//! | 契约 | 来源 | 行为 | 场景 |
//! |---|---|---|---|
//! | F-001 | `enforce_connection_policy` 无中心在册 → 拒 | 无中心时携带 Bearer 访问 JWT 档位端点 → 401，客户端报错且无数据 | 反例 |
//! | F-002 | 认证链端点免凭证档位由插件 activate 期登记 | 无中心在册时连 `/api/auth/pairing` 本身也是 401——不存在「无中心即可自助配对」的引导入口 | 反例 |
//! | F-003 | 激活中心后同请求 | 带真实 token → 200 且有数据（证明 F-001/F-002 不是「总是拒绝」） | 正例 |
//! | F-004 | 中心策略拒绝 | 伪造 token → 401（即便中心在册） | 反例 |
//! | F-005 | WS 端点同一闸门 | 无效凭证下终端链路重连但**永不 live**（无 `subscribed`） | 反例 |

mod common;

use std::sync::Arc;
use std::time::Duration;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::system::error::AppError;
use bedcode_mobile_lib::terminal_link::terminal_link_manager;

use common::desktop_ctx;
use common::mobile_ctx;

/// 断言失败落在「传输 / 基础设施」档（HTTP 401 / 404）而非「桌面业务拒绝」档
fn assert_transport_denial(err: AppError, status: u16, what: &str) {
    match err {
        AppError::Internal(msg) => assert!(
            msg.contains(&status.to_string()),
            "{what}：应报 HTTP {status}（显性拒绝），got: {msg}"
        ),
        other => panic!(
            "{what}：跨端契约要求显性传输级拒绝，实际变体 {other:?}（若是 AppError::Auth \
             说明请求根本没被拦在网关层，认证边界被绕过）"
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_center_and_bad_credential_are_denied_fail_closed() {
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    // ==================== 阶段一：认证中心【未激活】 ====================
    desktop_ctx::init_app_context_without_center().await;
    let (port, handle, server_task) = desktop_ctx::start_server().await;
    mobile_ctx::set_target(port).await;
    let base = mobile_ctx::base_url(port);

    let auth = AuthHttpClient::new();
    let sessions = SessionHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "crossend-failclosed-device",
        device_name: "CrossEnd FailClosed",
        fingerprint: "crossend-failclosed-fp",
        uid_hash: None,
    };

    // F-001 保护端点：即便带了形似凭证的 Bearer，无中心在册一律 401
    mobile_ctx::remember_token("not-a-real-token");
    let denied = sessions
        .list_sessions(&base)
        .await
        .expect_err("F-001 无认证中心在册时不得放行会话列表");
    assert_transport_denial(denied, 401, "F-001 无中心在册");

    // F-002 认证链自身端点也被封死：不存在「无中心即可自助配对」的引导入口。
    // （实测：`/api/auth/pairing` 的免凭证档位由插件在 activate 期登记；未激活时
    //  该路径不是公开路径，网关先于路由解析就拒 → 401 `{code:1007}`）
    let no_bootstrap = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect_err("F-002 无中心在册时认证链端点不得开放（否则等于给出无认证的自助配对入口）");
    assert_transport_denial(no_bootstrap, 401, "F-002 无中心时认证链端点");

    // ==================== 阶段二：激活中心 → F-004 伪造凭证 ====================
    desktop_ctx::activate_session_center().await;

    // F-004 中心在册但凭证无效：仍是一律拒绝（无「验签失败就放行」分支）
    mobile_ctx::remember_token("forged.jwt.token");
    let forged = sessions.list_sessions(&base).await.expect_err("F-004 伪造凭证必须被拒");
    assert_transport_denial(forged, 401, "F-004 伪造凭证");

    // ==================== F-003 正例对照：真凭证 → 放行 ====================
    // （放在最后跑：它是本文件唯一的放行路径，用来证明前面的 401 不是「总是拒绝」）
    let address = format!("127.0.0.1:{port}");
    let pairing = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect("F-003 中心激活后配对端点必须可达");
    let token = auth
        .verify_pairing_code(&base, ctx, &pairing.pairing_code, &address)
        .await
        .expect("F-003 配对换 token")
        .token;
    mobile_ctx::remember_token(&token);
    let allowed = sessions
        .list_sessions(&base)
        .await
        .expect("F-003 真凭证必须放行（否则 F-001/F-004 的拒绝没有区分力）");
    assert!(
        allowed.is_empty(),
        "F-003 刚激活即无会话，列表应为空数组（空数组 = 放行了但无数据；\
         拿到非空说明存在跨场景残留）"
    );

    // ==================== F-005 WS 面同一闸门 ====================
    mobile_ctx::remember_token("forged.jwt.token");
    let recorder = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs = Arc::new(mobile_ctx::OutputRecorder::default());
    let ghost = "crossend-failclosed-ghost-session";
    terminal_link_manager().page_subscribe(ghost, mobile_ctx::output_channel(outputs.clone()));
    terminal_link_manager().subscribe(recorder.clone(), ghost.to_string());

    // 链路会带着退避反复重连（证明它确实在尝试，不是压根没建连）
    mobile_ctx::wait_event(&recorder, "F-005 链路重连事件", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("reconnecting"))
    })
    .await;
    // 给足多轮重连的时间窗：任一轮都不许通过认证闸门
    tokio::time::sleep(Duration::from_secs(4)).await;
    let details = recorder.state_details();
    assert!(
        !details.iter().any(|d| d == "subscribed"),
        "F-005 无效凭证下链路绝不能收到 subscribed（宿主 close 4001 必须真的挡住），\
         实际状态序列={details:?}"
    );
    assert!(
        outputs.bytes().is_empty(),
        "F-005 无效凭证下不得有任何终端输出字节到达移动端页面通道，got {} 字节",
        outputs.bytes().len()
    );
    terminal_link_manager().remove(ghost);

    // ==================== 收尾 ====================
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
