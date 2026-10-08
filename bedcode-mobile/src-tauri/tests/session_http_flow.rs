//! 会话控制 HTTP wire 契约集成测试（票 13 改造）
//!
//! 客户端已迁插件：原宿主 `session::http::SessionHttpClient` + `commands::session`
//! 整体退役，会话控制（list/start/stop/remove/input）由 `com.bedcode.terminal-session`
//! 插件经 **host-http**（`jwtAuth` 宿主代注 Bearer，token 不落插件）直连桌面
//! `/api/sessions*`。本文件锁「宿主 host-http 执行器 + 桌面 wire 契约」：
//!
//! - 四端点 + input 的请求形状（configId camelCase / stop POST / remove DELETE /
//!   input `{data, specialKey}` 透传；specialKey 由桌面翻译）
//! - jwtAuth → `Authorization: Bearer` 注入（token 与全局存储同源）
//! - 非 2xx 与业务码信封在执行器层的透出形态（`{status, body}`）
//!
//! 边界：插件侧的请求构造 / 响应分类纯函数锁在插件 crate
//! （`wasm-apps/terminal-session/rust/src/session.rs` 单测）；无 token 的
//! fail-visible 裁决锁在宿主 lib 单测（`resolve_jwt_auth_header` 三向）。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use bedcode_mobile_lib::plugin::wasm_host::execute_http_request;
use bedcode_mobile_lib::state::{clear_global_token, set_global_token};
use serde_json::{json, Value};

/// 执行器包装（批次 2b：execute_http_request 增端口参数——用宿主真端口，
/// jwtAuth 用例的 set_global_token / redirect 策略语义与迁移前一致）
async fn exec(request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let ports: std::sync::Arc<dyn bedcode_wasm_core_mobile::host_api::ports::HostEnginePorts> =
        std::sync::Arc::new(bedcode_mobile_lib::plugin::host_ports::HostPorts);
    execute_http_request(request, &ports).await
}


/// 全局串行闸：`set_global_token`（jwtAuth 经宿主代注读取）是进程级共享静态，
/// 用例并发会互相踩 token——与 http_auth_flow / http_proxy_flow 的 SERIAL 同构
/// （跨 await 持锁被规则引擎判 blocker，故用单入口 suite 串行）。
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

// ==================== 请求构造（插件 host-http 契约同形） ====================

/// 与插件 crate `session.rs::build_request` 同形的请求 JSON：headers 固定 JSON、
/// `jwtAuth: true`（宿主代注 Bearer）、body 以字符串承载。此形状即 host-http
/// 执行器契约（插件侧由纯函数单测锁定），本文件经真实 reqwest 验证落线行为。
fn plugin_request(method: &str, url: String, body: Option<Value>) -> Value {
    let mut req = json!({
        "method": method,
        "url": url,
        "headers": { "Content-Type": "application/json" },
        "jwtAuth": true,
    });
    if let Some(b) = body {
        req["body"] = Value::String(b.to_string());
    }
    req
}

/// 执行器返回 `{status, body}` → 解析 body 为 JSON（信封）
fn json_body(resp: &Value) -> Value {
    let body = resp["body"].as_str().expect("response body is string");
    serde_json::from_str(body).expect("body is JSON")
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

async fn scenario_list_wire_and_jwt() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    set_global_token(MOCK_TOKEN);

    let resp = exec(&plugin_request(
        "GET",
        format!("{}/api/sessions", mock.base_url()),
        None,
    ))
    .await
    .expect("list transport must succeed");
    assert_eq!(resp["status"], 200);

    let body = json_body(&resp);
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["sessions"].as_array().expect("sessions array").len(), 2);
    // wire 字面量锁：桌面会话状态用 camelCase waitingInput（票 06 统一点）
    assert_eq!(body["data"]["sessions"][1]["status"], "waitingInput");

    // JWT Bearer 头断言（插件不持 token：宿主代注）
    let (_path, _method, auth, _body) = mock.state.last_req("/api/sessions");
    assert_eq!(auth.as_deref(), Some("Bearer mock-jwt-token"));

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_start_stop_remove_input_shapes() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    set_global_token(MOCK_TOKEN);
    let base = mock.base_url();

    let resp = exec(&plugin_request(
        "POST",
        format!("{base}/api/sessions/start"),
        // 与插件 build_start_body 同形（cols/rows 齐备才出现）
        Some(json!({ "configId": "cfg-1", "cols": 120, "rows": 30 })),
    ))
    .await
    .expect("start transport must succeed");
    assert_eq!(json_body(&resp)["data"]["sessionId"], "s-new");
    let (_p, _m, _a, body) = mock.state.last_req("/api/sessions/start");
    assert_eq!(body["configId"], "cfg-1", "启动载荷应带 configId（camelCase，桌面 wire）");
    assert_eq!(body["cols"], 120);

    // stop / remove / input：无 data 的 ok 信封
    exec(&plugin_request(
        "POST",
        format!("{base}/api/sessions/s-new/stop"),
        None,
    ))
    .await
    .expect("stop transport");
    exec(&plugin_request(
        "DELETE",
        format!("{base}/api/sessions/s-new/remove"),
        None,
    ))
    .await
    .expect("remove transport");
    exec(&plugin_request(
        "POST",
        format!("{base}/api/sessions/s-new/input"),
        Some(json!({ "data": "ls -la", "specialKey": null })),
    ))
    .await
    .expect("input data transport");
    exec(&plugin_request(
        "POST",
        format!("{base}/api/sessions/s-new/input"),
        Some(json!({ "data": "", "specialKey": "ctrl+c" })),
    ))
    .await
    .expect("input specialKey transport");

    // 输入载荷透传：data / specialKey 原样携带（specialKey 由桌面翻译，本端不解释）
    let (_p, _m, _a, body) = mock.state.last_req("/api/sessions/s-new/input");
    assert_eq!(body["data"], "");
    assert_eq!(body["specialKey"], "ctrl+c");

    // remove 用 DELETE 方法；stop 用 POST
    let (_p, method, _a, _b) = mock.state.last_req("/api/sessions/s-new/remove");
    assert_eq!(method, "DELETE");
    let (_p, method, _a, _b) = mock.state.last_req("/api/sessions/s-new/stop");
    assert_eq!(method, "POST");

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_business_error_envelope_passes_through() {
    let mock = MockDesktop::start(MockMode::StopRejected).await;
    set_global_token(MOCK_TOKEN);

    // 桌面错误口径：HTTP 200 + `{code:1002, message}`——执行器不解释业务码，
    // 原样透出 body（分类归插件 session.rs：前端据此拿到 code=1002）
    let resp = exec(&plugin_request(
        "POST",
        format!("{}/api/sessions/s-missing/stop", mock.base_url()),
        None,
    ))
    .await
    .expect("transport layer ok: business error rides on HTTP 200");
    assert_eq!(resp["status"], 200);
    let body = json_body(&resp);
    assert_eq!(body["code"], 1002);
    assert_eq!(body["message"], "session not found");

    clear_global_token();
    mock.shutdown().await;
}

async fn scenario_non_2xx_status_passes_through() {
    let mock = MockDesktop::start(MockMode::InputHttpError).await;
    set_global_token(MOCK_TOKEN);

    // 非 2xx：执行器返回 `{status:500, body}`（不 Err）——插件分类层映射前端
    // `{code: status}`（插件 session.rs 单测锁），本层锁 status 透出
    let resp = exec(&plugin_request(
        "POST",
        format!("{}/api/sessions/s1/input", mock.base_url()),
        Some(json!({ "data": "x", "specialKey": null })),
    ))
    .await
    .expect("non-2xx is a transport-level success at executor layer");
    assert_eq!(resp["status"], 500);
    assert!(resp["body"].as_str().unwrap().contains("boom"));

    clear_global_token();
    mock.shutdown().await;
}

// ==================== 全文件串行入口 ====================

/// 全局 token 共享静态：本二进制内场景必须串行（单入口按序驱动）。
#[tokio::test]
async fn session_http_full_suite() {
    let _serial = SERIAL.lock().unwrap();
    scenario_list_wire_and_jwt().await;
    scenario_start_stop_remove_input_shapes().await;
    scenario_business_error_envelope_passes_through().await;
    scenario_non_2xx_status_passes_through().await;
}
