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
//! （`wasm-apps/bedcode/rust/src/session.rs` 单测）；jwtAuth 的
//! 「注入 / 未开不注入 / 缺 token 即拒」三向锁在能力域 crate
//! （`bedcode-server-http` egress 用例组），移动 adapter 的 token 取值口径
//! 见 `bedcode-wasm-core` 的 `bare/host_impl/http.rs`。
//!
//! 2026-10-10 票 05：宿主自持的 `http_engine` 退役，出站执行并入能力域 crate
//! （`bedcode-server-http::plugin_binding::egress::http_fetch`）。本文件因此改为
//! 用一个**测试就地端口 impl** 驱动能力域真执行器：权限门 / 出站授权放行、
//! 事件通道缺席（本套件不测流式）、token 取宿主全局态（`HostPorts::global_token`，
//! 与生产同源）——即「宿主级调用者」形态，与插件调用共用同一执行本体。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use bedcode_server_http::plugin_binding::egress::http_fetch;
use bedcode_server_http::plugin_binding::ports::{
    BoxedBlocked, HttpEventSink, HttpPorts, OutboundAuth,
};
use bedcode_wasm_core::host_api::ports::HostEnginePorts;
use bedcode_mobile_lib::state::{clear_global_token, set_global_token};
use serde_json::{json, Value};

/// 宿主级端口：能力域要求的三件宿主服务在本套件里都取「放行」形态
///
/// - 权限门：宿主自身发起（非插件 wasm 路径）→ 恒放行；
/// - 出站授权：本套件锁的是 wire 契约与代注语义，目标为本地 loopback mock
///   → 直接放行（生产里的三层判定 + 弹窗在 egress 引擎，另有其锁）；
/// - 事件通道：缺席（本套件不覆盖流式）。
struct HostLevelPorts;

impl HttpPorts for HostLevelPorts {
    fn check_permission(&self, _plugin_id: &str, _permission: &str, _api: &str) -> bool {
        true
    }

    fn authorize_outbound(&self, _plugin_id: &str, url: &str, _may_prompt: bool) -> OutboundAuth {
        OutboundAuth::Allowed {
            origin: url.split(['/', '?']).take(3).collect::<Vec<_>>().join("/"),
        }
    }

    fn event_sink(&self) -> Option<Arc<dyn HttpEventSink>> {
        None
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn std::any::Any + Send> {
        // 用例体是 async：能力域的同步桥由 `tokio::task::block_in_place` 提供
        // 阻塞上下文后，再用当前 runtime 句柄驱动（与适配器侧同法）。
        // 直接返回端口要求的 `Box<dyn Any + Send>`——**不得再包一层 Box**
        // （能力域按输出类型 downcast，多包一层即 panic 点名类型不符）。
        tokio::runtime::Handle::current().block_on(fut)
    }

    /// token 与生产同源：读宿主全局态（`HostPorts::global_token`），
    /// 空 = 未认证 ⇒ 显错（fail-visible，与生产 adapter 同文案）
    fn jwt_auth_token(&self, _plugin_id: &str) -> Result<Option<String>, String> {
        let token = bedcode_mobile_lib::plugin::host_ports::HostPorts::default().global_token();
        if token.is_empty() {
            Err("jwtAuth requested but no global token available (not authenticated)".to_string())
        } else {
            Ok(Some(token))
        }
    }
}

/// 执行器包装：源起能力域真执行器（同步接口），在异步用例里经 block_in_place 驱动
async fn exec(request: &serde_json::Value) -> Result<Value, String> {
    let request_json = request.to_string();
    let plugin_id = "com.bedcode.terminal-session";
    let ports: Arc<dyn HttpPorts> = Arc::new(HostLevelPorts);
    tokio::task::block_in_place(|| {
        // 非流式请求必带 `{status, body, headers}` 响应 JSON；解码失败 = 契约破损
        let raw = http_fetch(&ports, plugin_id, &request_json, false)?
            .expect("非流式请求必须返回响应体（None 是流式 ack 的形态）");
        serde_json::from_str(&raw).map_err(|e| format!("response is not JSON: {e}"))
    })
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
///
/// `multi_thread` 为必需：能力域执行器走同步桥（`block_in_place`），current_thread
/// runtime 上该调用直接 panic（tokio 语义），与生产「宿主从非异步调用点发起」同形。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_http_full_suite() {
    let _serial = SERIAL.lock().unwrap();
    scenario_list_wire_and_jwt().await;
    scenario_start_stop_remove_input_shapes().await;
    scenario_business_error_envelope_passes_through().await;
    scenario_non_2xx_status_passes_through().await;
}
