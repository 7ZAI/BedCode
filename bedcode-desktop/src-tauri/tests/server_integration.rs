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
//! 注释亦记录了此限制）。认证中间件先于路由执行：对未注册路径，
//! 放行的请求由路由返回 404，未放行的在中间件被 401 拦截。
//!
//! **v33（ADR 0033）后本文件不再有「合法 token 放行」那一格**：宿主既无签发面
//! 也无验签面，认证判定全部在认证中心插件里，而本文件**没有 AppContext**（上面
//! 说了装不了）⇒ 无头进程里认证中心不可达 ⇒ 任何凭证都被拒（fail-closed）。
//! 这正是本文件现在锁的性质：**无中心即全拒**。放行那一半由自带真实 AppContext
//! + 真实中心产物的集成二进制覆盖（`ws_auth_rules.rs` / `http_auth_biometric.rs`）。

use std::io;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_desktop_lib::server::composition::start_http_server;
use bedcode_desktop_lib::AppConfig;
use bedcode_server_core::supervisor::ServerSupervisor;

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
async fn send_until(request: reqwest::RequestBuilder, timeout: Duration) -> reqwest::Result<reqwest::Response> {
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
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");
    let base = format!("http://127.0.0.1:{port}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client failed");

    // ==================== 场景 1：健康检查（公开端点） ====================

    let resp = send_until(client.get(format!("{base}/api/health")), Duration::from_secs(5))
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

    // 2c. 结构合法但无中心可验的 token（HS256 三段、签名位非空）→ 同样 401
    //
    // v33 前这里是「用错误密钥签的 token」；v33 后宿主**没有任何验签面**，所以
    // 「签名对不对」根本轮不到宿主判——无中心一律拒（fail-closed）。用一个
    // 结构上完全合法的 token 钉住这一点：**别再把 401 归因成「token 坏了」**。
    let structurally_valid_token = format!(
        "{}.{}.{}",
        "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9",
        "eyJzdWIiOiJ0ZXN0LWRldmljZSIsImlzcyI6IkJlZENvZGUiLCJpYXQiOjE3MDAwMDAwMDAsImV4cCI6MTcwMDYwNDgwMH0",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    );
    let resp = send_until(
        client.get(&protected).bearer_auth(&structurally_valid_token),
        Duration::from_secs(5),
    )
    .await
    .expect("request with structurally valid token must reach server");
    assert_eq!(
        resp.status(),
        401,
        "无认证中心可达时结构合法的 token 同样必须被拒（fail-closed）"
    );
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1007, "401 body must carry code 1007");

    // 2d. **无中心 = 全拒**（ADR 0031 K3 的可观测形态之一）
    //
    // 本进程无 AppContext ⇒ 无认证中心 ⇒ 无论凭证长什么样都到不了路由
    // （404）。这是 v33 之后本文件唯一能诚实断言的鉴权性质：宿主侧**没有**
    // 「本地验签通过就放行」的旁路（v33 前有，靠 `utils/auth/jwt.rs`）。
    for token in ["", "a.b.c", "definitely.not.a.jwt"] {
        let mut req = client.get(&protected);
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
        let resp = send_until(req, Duration::from_secs(5))
            .await
            .expect("request must reach server");
        assert_eq!(resp.status(), 401, "无认证中心时任何凭证都不得放行（token={token:?}）");
    }

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
    let resp = send_until(client2.get(format!("{base}/api/health")), Duration::from_secs(5))
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
