//! 会话控制面 HTTP 集成测试（专项票 04 P3 / 票 07 P6 统一运行）
//!
//! 用 actix-web 起 mock 桌面端 HTTP 服务器（移动端 crate 自带 actix-web 主依赖，
//! 零新增），逐路由返回与桌面端 `ApiResponse` 对称的 `{code,message,data?}` 信封，
//! 真实 `SessionHttpClient` + reqwest 打真请求，验证：
//! - 四端点 wire shape（GET /api/sessions、POST /api/sessions/start、
//!   POST /{id}/stop、DELETE /{id}/remove、POST /{id}/input）
//! - JWT Bearer Authorization 头注入
//! - 成功形状（含无 data 的 ok 信封）、业务码 1002 → AppError::Auth、
//!   非 2xx → AppError::Internal
//! - 输入载荷透传（data + specialKey 均原样携带；specialKey 由桌面翻译）
//!
//! session.list / terminal.sendInput 的**权限判定**（plugin/context.ts）是前端单测面
//! （`pluginContextHttp.test.ts` 7 例，票 04 已绿），本文件只锁 HTTP 传输层形状。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use bedcode_lib::connection::manager::ConnectionManager;
use bedcode_lib::session::http::{SessionHttpClient, session_base_url};
use bedcode_lib::state::{clear_global_token, set_global_token};
use bedcode_lib::AppError;
use serde_json::{json, Value};

/// 全局串行闸：`set_global_token`（SessionHttpClient 经 bearer_auth 读取）是
/// 进程级共享静态，用例并发会互相踩 token——与 http_auth_flow / http_proxy_flow
/// 的 SERIAL 同构（跨 await 持锁被规则引擎判 blocker，故用单入口 suite 串行）。
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const MOCK_TOKEN: &str = "mock-jwt-token";

/// mock 响应模式：控制个别端点的业务拒绝/传输故障行为
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MockMode {
    /// 全 happy path
    Happy,
    /// stop 回 200 + {code:1002}（会话不存在）
    StopRejected,
    /// input 回 500（基础设施故障）
    InputHttpError,
}

impl MockMode {
    fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::StopRejected,
            2 => Self::InputHttpError,
            _ => Self::Happy,
        }
    }
    fn bits(self) -> u8 {
        self as u8
    }
}

/// mock 服务器共享状态：模式 + 收到的全部请求（path / method / Authorization 头 / body）
struct MockState {
    mode: AtomicU8,
    received: Mutex<Vec<(String, String, Option<String>, Value)>>,
}

impl MockState {
    fn new(mode: MockMode) -> Self {
        Self {
            mode: AtomicU8::new(mode.bits()),
            received: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, path: &str, method: &str, auth: Option<&str>, body: &Value) {
        self.received.lock().unwrap().push((
            path.to_string(),
            method.to_string(),
            auth.map(str::to_string),
            body.clone(),
        ));
    }

    /// 取指定 path 的最后一次请求（断言请求字段 / JWT 头）
    fn last_req(&self, path_suffix: &str) -> (String, String, Option<String>, Value) {
        self.received
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(p, _, _, _)| p.ends_with(path_suffix))
            .cloned()
            .expect("no request recorded for path suffix")
    }
}

/// actix 路由闭包需要的共享数据（web::Data）
#[derive(Clone)]
struct MockData {
    state: Arc<MockState>,
}

struct MockDesktop {
    addr: SocketAddr,
    state: Arc<MockState>,
    handle: actix_web::dev::ServerHandle,
}

impl MockDesktop {
    async fn start(mode: MockMode) -> Self {
        let state = Arc::new(MockState::new(mode));
        let data = MockData { state: state.clone() };
        let server = HttpServer::new(move || {
            let data = data.clone();
            App::new()
                .app_data(web::Data::new(data))
                .route("/api/sessions", web::get().to(mock_list))
                .route("/api/sessions/start", web::post().to(mock_start))
                .route("/api/sessions/{id}/stop", web::post().to(mock_stop))
                .route("/api/sessions/{id}/remove", web::delete().to(mock_remove))
                .route("/api/sessions/{id}/input", web::post().to(mock_input))
        })
        .bind(("127.0.0.1", 0))
        .expect("bind mock session http server");
        let addr = server.addrs()[0];
        let http_server = server.run();
        let handle = http_server.handle();
        tokio::spawn(async move {
            let _ = http_server.await;
        });
        Self { addr, state, handle }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    async fn shutdown(&self) {
        let _ = self.handle.stop(false).await;
    }
}

impl Drop for MockDesktop {
    fn drop(&mut self) {
        // 兜底停机（主路径是每场景末尾的 shutdown()）：万一断言 panic 提前退出，
        // 别让 actix worker 线程泄漏到后续场景（与 http_auth_flow 同款；
        // 必须用 tokio::spawn——actix_rt::spawn 是 spawn_local，非 LocalSet 下 panic）
        let handle = self.handle.clone();
        tokio::spawn(async move {
            let _ = handle.stop(false).await;
        });
    }
}

// ==================== 路由处理器 ====================

fn auth_header(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

async fn mock_list(data: web::Data<MockData>, req: HttpRequest) -> HttpResponse {
    data.state.record("/api/sessions", "GET", auth_header(&req).as_deref(), &json!({}));
    HttpResponse::Ok().json(json!({
        "code": 0, "message": "ok",
        "data": { "sessions": [
            { "id": "s1", "status": "running", "name": "dev" },
            { "id": "s2", "status": "waitingInput", "name": "itest" },
        ]}
    }))
}

async fn mock_start(data: web::Data<MockData>, req: HttpRequest, body: web::Json<Value>) -> HttpResponse {
    data.state.record("/api/sessions/start", "POST", auth_header(&req).as_deref(), &body);
    HttpResponse::Ok().json(json!({"code": 0, "message": "ok", "data": {"sessionId": "s-new"}}))
}

async fn mock_stop(data: web::Data<MockData>, req: HttpRequest, path: web::Path<String>) -> HttpResponse {
    // stop 请求无 body（客户端只发空 POST；Content-Type 亦非 json）——不挂 web::Json
    data.state
        .record(&format!("/api/sessions/{path}/stop"), "POST", auth_header(&req).as_deref(), &json!({}));
    match MockMode::from_bits(data.state.mode.load(Ordering::SeqCst)) {
        MockMode::StopRejected => {
            HttpResponse::Ok().json(json!({"code": 1002, "message": "session not found"}))
        }
        _ => HttpResponse::Ok().json(json!({"code": 0, "message": "ok"})),
    }
}

async fn mock_remove(data: web::Data<MockData>, req: HttpRequest, path: web::Path<String>) -> HttpResponse {
    data.state
        .record(&format!("/api/sessions/{path}/remove"), "DELETE", auth_header(&req).as_deref(), &json!({}));
    HttpResponse::Ok().json(json!({"code": 0, "message": "ok"}))
}

async fn mock_input(data: web::Data<MockData>, req: HttpRequest, path: web::Path<String>, body: web::Json<Value>) -> HttpResponse {
    data.state
        .record(&format!("/api/sessions/{path}/input"), "POST", auth_header(&req).as_deref(), &body);
    match MockMode::from_bits(data.state.mode.load(Ordering::SeqCst)) {
        MockMode::InputHttpError => HttpResponse::InternalServerError().json(json!({"code": 1, "message": "boom"})),
        _ => HttpResponse::Ok().json(json!({"code": 0, "message": "ok"})),
    }
}

// ==================== 场景（单入口串行：全局 token 共享） ====================

/// 建客户端 + 保存 target 并解析 base URL（`session_base_url` = resolve_base_url）
async fn setup(port: u16) -> (Arc<SessionHttpClient>, String) {
    let conn = ConnectionManager::new();
    conn.set_target("127.0.0.1".to_string(), port, None).await;
    let base = session_base_url(&conn).await.expect("base url from target");
    (SessionHttpClient::new(), base)
}

async fn scenario_list_and_jwt_header() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    set_global_token(MOCK_TOKEN);
    let (client, base) = setup(mock.addr.port()).await;

    let sessions = client.list_sessions(&base).await.expect("list should succeed");
    assert_eq!(sessions.len(), 2, "应解析出 2 条会话");
    assert_eq!(sessions[0]["id"], "s1");
    assert_eq!(sessions[0]["status"], "running");
    // wire 字面量锁：桌面会话状态用 camelCase waitingInput（票 06 统一点）
    assert_eq!(sessions[1]["status"], "waitingInput");

    // JWT Bearer 头断言
    let (_path, _method, auth, _body) = mock.state.last_req("/api/sessions");
    assert_eq!(auth.as_deref(), Some("Bearer mock-jwt-token"));

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_start_stop_remove_input_shapes() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    set_global_token(MOCK_TOKEN);
    let (client, base) = setup(mock.addr.port()).await;

    let sid = client
        .start_session(&base, "cfg-1", Some(120), Some(30))
        .await
        .expect("start should succeed");
    assert_eq!(sid, "s-new");
    let (_p, _m, _a, body) = mock.state.last_req("/api/sessions/start");
    assert_eq!(body["configId"], "cfg-1", "启动载荷应带 configId（camelCase，桌面 wire）");
    assert_eq!(body["cols"], 120);

    // stop / remove / input：无 data 的 ok 信封（`parse_ok_envelope` 对无 data 不违约）
    client.stop_session(&base, &sid).await.expect("stop ok envelope");
    client.remove_session(&base, &sid).await.expect("remove ok envelope");
    client
        .send_input(&base, &sid, "ls -la", None)
        .await
        .expect("input data");
    client
        .send_input(&base, &sid, "", Some("ctrl+c"))
        .await
        .expect("input specialKey");

    // 输入载荷透传：data / specialKey 原样携带（specialKey 由桌面翻译，本端不解释）
    let (_p, _m, _a, body) = mock.state.last_req("/api/sessions/s-new/input");
    assert_eq!(body["data"], "");
    assert_eq!(body["specialKey"], "ctrl+c");

    // remove 用 DELETE 方法
    let (_p, method, _a, _b) = mock.state.last_req("/api/sessions/s-new/remove");
    assert_eq!(method, "DELETE");

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_business_error_maps_to_auth() {
    let mock = MockDesktop::start(MockMode::StopRejected).await;
    set_global_token(MOCK_TOKEN);
    let (client, base) = setup(mock.addr.port()).await;

    let err = client
        .stop_session(&base, "s-missing")
        .await
        .expect_err("业务码应映射错误");
    match err {
        AppError::Auth(msg) => {
            assert!(msg.contains("1002"), "AppError::Auth 应透传桌面业务码 1002: {msg}");
        }
        other => panic!("code=1002 应映射 AppError::Auth，实际 {other:?}"),
    }

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_http_error_maps_to_internal() {
    let mock = MockDesktop::start(MockMode::InputHttpError).await;
    set_global_token(MOCK_TOKEN);
    let (client, base) = setup(mock.addr.port()).await;

    let err = client
        .send_input(&base, "s1", "x", None)
        .await
        .expect_err("非 2xx 应映射错误");
    match err {
        AppError::Internal(msg) => {
            assert!(msg.contains("500"), "非 2xx 应映射 AppError::Internal 且带状态码: {msg}");
        }
        other => panic!("HTTP 500 应映射 AppError::Internal，实际 {other:?}"),
    }

    clear_global_token();
    mock.shutdown().await;
}

// ==================== 全文件串行入口 ====================

/// 全局 token 共享静态：本二进制内场景必须串行（单入口按序驱动）。
#[tokio::test]
async fn session_http_full_suite() {
    let _serial = SERIAL.lock().unwrap();
    scenario_list_and_jwt_header().await;
    scenario_start_stop_remove_input_shapes().await;
    scenario_business_error_maps_to_auth().await;
    scenario_http_error_maps_to_internal().await;
}