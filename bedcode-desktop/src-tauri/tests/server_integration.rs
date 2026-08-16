//! 集成测试基建 + HTTP 契约（spec L1 场景 1–2）
//!
//! 原理：进程内真实启动 Actix HTTP 服务器（OS 分配端口，与真实实例 8765 隔离）
//! → 真实 reqwest 客户端从外部连入 → 走完整请求链路（中间件 → 路由 → handler），
//! 断言端到端行为。测试 seam：服务器外部 HTTP 接口，不 mock 服务器内部任何组件。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）——
//! `ServerSupervisor` / `MetricsCollector` 等全局单例跨测试共享，
//! 禁止并行起停服务器（教训：metrics 单测曾因并行污染失败）。
//!
//! 为什么用「未注册路径」做鉴权 A/B：所有受保护业务 handler 均直接调用
//! `AppContext::global()`，而 `AppContext.app_handle` 是硬编码的
//! `Arc<AppHandle<Wry>>`，tauri 的 `mock_app()` 只能给出 `MockRuntime` 句柄，
//! 二者类型不兼容，集成测试无法初始化 AppContext（api_bridge.rs 的 cfg(test)
//! 注释亦记录了此限制）。JWT 中间件先于路由执行：对未注册路径，
//! 无/非法 token 在中间件被 401 拦截，合法 token 放行后由路由返回 404——
//! 「401 vs 404」之差即是放行契约的可观测证明，全程不触发 handler。

use std::io;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::server::supervisor::ServerSupervisor;
use bedcode_lib::utils::auth::jwt::{JwtClaims, JwtService};
use bedcode_lib::AppConfig;

/// 探测空闲端口：绑定 127.0.0.1:0 由 OS 分配，立即释放后交给服务器绑定 0.0.0.0
///
/// 两次 cargo test 之间 OS 重新分配，与并行跑的 lib 测试（538 个）互不干扰；
/// 探测与绑定之间存在极小竞态窗口（其他进程恰好占用），测试重跑即恢复，可接受
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 启动测试服务器：真实 `start_http_server` + 默认网络配置（端口显式传入）
///
/// 服务器 future 必须保活（drop 会触发停机），spawn 到测试 runtime 上持续轮询；
/// actix worker 运行在各自线程的独立 runtime，不受 current_thread 测试 runtime 限制
async fn spawn_test_server(port: u16) -> io::Result<(ServerHandle, tokio::task::JoinHandle<io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 发起请求，连接失败（服务器 worker 尚未就绪）时按 25ms 间隔重试直至超时
///
/// `#[tokio::test]` 是 current_thread runtime：等待异步事件必须用
/// `tokio::time::sleep + yield_now`，禁止 `std::thread::sleep` 阻塞轮询
async fn send_until(
    request: reqwest::RequestBuilder,
    timeout: Duration,
) -> reqwest::Result<reqwest::Response> {
    let deadline = Instant::now() + timeout;
    loop {
        match request.try_clone().expect("request must be cloneable").send().await {
            Ok(resp) => return Ok(resp),
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
                tokio::task::yield_now().await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// 解析响应体为 JSON；reqwest 未启用 "json" feature，直接经 serde_json 解析
async fn body_json(resp: reqwest::Response) -> serde_json::Value {
    let bytes = resp.bytes().await.expect("read response body failed");
    serde_json::from_slice(&bytes).expect("response body must be valid JSON")
}

#[tokio::test]
async fn http_contract_and_server_lifecycle() {
    // 测试日志输出到 harness（失败时可查链路）；重复 init 静默跳过
    if tracing_subscriber::fmt().with_test_writer().try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port)
        .await
        .expect("test server must start");
    let base = format!("http://127.0.0.1:{port}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client failed");

    // ==================== 场景 1：健康检查（公开端点） ====================

    let resp = send_until(
        client.get(format!("{base}/api/health")),
        Duration::from_secs(5),
    )
    .await
    .expect("health request must reach server");
    assert_eq!(resp.status(), 200, "health check must return 200");
    let body = body_json(resp).await;
    assert_eq!(body["status"], "ok", "health body must report status ok");
    // port 字段来自 ServerSupervisor 状态（本测试直启服务器、未走 supervisor 启动
    // 路径），与 supervisor 报告值一致即为真实往返；不硬编码端口值
    let supervisor_port = ServerSupervisor::global().get_status_info().await.port;
    assert_eq!(
        body["port"].as_u64(),
        Some(u64::from(supervisor_port)),
        "health body port must match supervisor-reported port"
    );
    assert!(
        body["uptime_secs"].is_null(),
        "server started directly (bypassing supervisor) so uptime must be null"
    );

    // ==================== 场景 2：JWT 鉴权契约（受保护 /api scope） ====================

    // 未注册路径：中间件先于路由执行，401/404 之差即放行契约的观测点
    let protected = format!("{base}/api/contract-test-unregistered");

    // 2a. 无 token → 401 + 契约错误体（code 1007 / message 固定文案）
    let resp = send_until(client.get(&protected), Duration::from_secs(5))
        .await
        .expect("request without token must reach server");
    assert_eq!(resp.status(), 401, "protected path without token must be rejected");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1007, "401 body must carry contract error code 1007");
    assert_eq!(
        body["message"], "Authentication required",
        "401 body must carry contract error message"
    );

    // 2b. 非法 token（乱串）→ 401
    let resp = send_until(
        client.get(&protected).bearer_auth("definitely.not.a.jwt"),
        Duration::from_secs(5),
    )
    .await
    .expect("request with garbage token must reach server");
    assert_eq!(resp.status(), 401, "garbage token must be rejected");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1007, "garbage token 401 body must carry code 1007");

    // 2c. 非法 token（结构合法但用错误密钥签名）→ 401
    // 用 jsonwebtoken（生产依赖，测试可直接使用）以不同密钥签发同结构 claims
    let wrong_key_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &JwtClaims::new("test-device".to_string(), None, None, 3600),
        &jsonwebtoken::EncodingKey::from_secret(b"different-secret-for-test"),
    )
    .expect("mint token with wrong key failed");
    let resp = send_until(
        client.get(&protected).bearer_auth(&wrong_key_token),
        Duration::from_secs(5),
    )
    .await
    .expect("request with wrong-key token must reach server");
    assert_eq!(resp.status(), 401, "wrongly signed token must be rejected");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1007, "wrongly signed token 401 body must carry code 1007");

    // 2d. 合法 token（JwtService 同密钥签发，即生产签发路径）→ 放行
    // 断言 404 而非 401：请求已通过中间件进入路由，只是该路径未注册
    let valid_token = JwtService::new()
        .generate_token(
            "test-device".to_string(),
            Some("Integration Test".to_string()),
            None,
        )
        .expect("mint valid token failed");
    let resp = send_until(
        client.get(&protected).bearer_auth(&valid_token),
        Duration::from_secs(5),
    )
    .await
    .expect("request with valid token must reach server");
    assert_eq!(resp.status(), 404, "valid token passes middleware, unmatched route must 404 (not 401)");

    // ==================== 场景 3：优雅停机 + 端口复用 ====================

    // 先关闭客户端：由客户端发起 FIN 关闭连接，服务端 socket 不留 TIME_WAIT，
    // 端口可立即复用（Windows 上 actix 不设 SO_REUSEADDR，服务端 TIME_WAIT 会
    // 阻塞同端口重绑）；随后 `stop(true)` 优雅停机等待在途请求完成
    drop(client);
    tokio::time::sleep(Duration::from_millis(100)).await;

    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");

    // 同一端口立即重启 → 证明优雅停机已释放端口（覆盖 ticket 验收项）
    let (handle2, server_task2) = spawn_test_server(port)
        .await
        .expect("port must be reusable right after graceful stop");

    let client2 = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build second reqwest client failed");
    let resp = send_until(
        client2.get(format!("{base}/api/health")),
        Duration::from_secs(5),
    )
    .await
    .expect("restarted server must answer health request");
    assert_eq!(resp.status(), 200, "restarted server must serve health check");

    // 收尾：第二个服务器同样优雅停机，保证测试进程退出干净
    drop(client2);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(Duration::from_secs(10), handle2.stop(true))
        .await
        .expect("second graceful stop must complete within timeout");
    server_task2
        .await
        .expect("second server task must not panic")
        .expect("second server must exit Ok");
}
