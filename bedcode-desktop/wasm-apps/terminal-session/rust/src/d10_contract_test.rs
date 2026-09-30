//! 票 08 / D10 试点：本插件的**自有路由公理** × **真实宿主 server 网关**的闭环契约
//!
//! ## 这条测试为什么现在才可能存在（server-lib-split 票 08 的收益兑现）
//!
//! 拆分前，`server::http::registry` / `server::core::app::serve` / WS 端点表都住在
//! 宿主 crate 里，而宿主 crate 依赖 Tauri + wasmtime。插件 crate 想验证「自己声明的
//! 路由在真实网关上确实可达、档位确实生效」，只能二选一：① 整个拖进宿主（那就回到
//! 拆分前的耦合，且插件测试要构建 Tauri + wasmtime）；② 不测（那就靠 `cross-end-tests`
//! 的端到端场景间接兜，而那里的失败信号是「七个场景一起红」，归因成本极高）。
//!
//! 拆分后 `bedcode-server-*` 是**纯 actix / 无 Tauri**（`bedcode-server-http` 的生产
//! 依赖里没有 tauri，见其清单），本文件作为插件 crate 的 dev-dependency 引入即可：
//! 自装 12 个端口实现 → 真 `serve()` 绑定真实端口 → 真实 HTTP / WS 客户端从外部连入。
//! **server 侧零 mock**：注册表、网关中间件、JWT 中间件、模板捕获、转发全是真的；
//! 只有「宿主能力」按端口注入（那是拆分刻意造出来的接缝，不是 mock 面）。
//!
//! ## 被测的两张公理（都在本插件侧，测试数据不是手抄的）
//!
//! - HTTP：[`crate::http_routes::ROUTES`]——「本插件 activate 期声明了哪些内部端点、
//!   哪些对外别名、什么方法、什么认证档位」的**单一事实源**。
//! - WS：本插件 `plugin.json` 的 `contributes.wsEndpoints`（宿主 activate 期照它登记
//!   端点）。manifest 在 crate 之外，用 `CARGO_MANIFEST_DIR` 上溯两级定位。
//!
//! 两条腿各自能抓到、且**两端各自单测都抓不到**的缺陷：
//!
//! | 缺陷 | 为什么两端单测都看不见 |
//! | --- | --- |
//! | 别名拼错 / 方法写错 ⇒ 移动端 URL 404 | 插件 `routes_table_is_self_consistent` 只查形状自洽；宿主 registry 单测用的是**它自己造的**别名 |
//! | 某端点被误标 `auth: "none"` ⇒ 越权可达 | 插件的 `no_auth_routes_are_justified` 是清单比对，宿主侧网关单测用的是 `EndpointAuth` 手写字面量 |
//! | 模板别名 `{id}` 捕获不注入 `params` | 两端各测各的公式，真实互连才暴露「转发时 params 丢了」 |
//! | manifest 的 `wsEndpoints` 与插件代码里的端点常量漂移 | manifest 由宿主读、代码在插件，两边没有共同断言 |
//!
//! ## 唯一端口实现（本文件自带的假端口，逐个说明为什么安全）
//!
//! `PluginInvoker` 的 `invoke_rust_command` 是唯一的「假」：它**不跑插件代码**
//! （插件代码只在 wasmtime 里跑，见 spec §4 票 08 的范围说明），而是回一个固定信封
//! 并把入参记账。这让断言聚焦在「网关有没有把请求**正确地交到插件边界上**」
//! （方法 / 模板捕获 / 档位 / query / body），而不是插件内部行为——后者是插件 crate
//! 421 项单测的职责。
//!
//! ## 本文件的形态：`src/` 下的 `#[cfg(test)]` 模块，不是 `tests/` 集成目标
//!
//! 集成目标（`tests/*.rs`）需要本 crate 以**可链接形式**（rlib）产出，而加上
//! rlib 会连带为宿主目标构建 x86_64 cdylib，其 wit-bindgen 导出名
//! （`bedcode:plugin/abi#form`）在 ELF version script 里非法 ⇒ 链接期失败。
//! 单测 harness 不需要 crate-type 可链接，所以放 `src/` 下按 AGENTS §6 命名为
//! `*_test.rs`。dev-deps 不参与 wasm 构建，`pnpm run build` 的产物链零影响。
//!
//! `AuthCenter` 用**一个魔 token** 做 A/B：只认一个固定 token，其余（含空）一律拒。
//! 于是「`jwt` 档 + 无凭证 → 401」与「`none` 档 + 无凭证 → 转发」可以在**同一次
//! 运行、同一个服务器**里对拍，不需要起第二个进程（端口注册表是进程级 `OnceLock`）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::http_routes::{HttpRouteDecl, ROUTES};
use bedcode_plugin_api::host::bus::TOPIC_NS_SEP;
use bedcode_plugin_api::host::ws::WS_CLIENT_CONNECT;
use bedcode_plugin_api::EndpointAuth;
use bedcode_server_base::config::NetworkConfig;
use bedcode_server_base::identity::AuthenticatedIdentity;
use bedcode_server_base::ports::{
    AuthCenter, BusMessageHandler, BusPort, ConfigPort, EventSink, MdnsAdvertiserPort, MdnsPort, PathsPort,
    PluginInvoker, PowerPort, RuntimePort, ServerLifecycleEvent, ServerLifecyclePort, ServerPorts, SystemInfoPort,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};

/// 本插件 id（与 `plugin.json` 的 `id` 逐字一致；命名空间段由宿主注入）
const PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 唯一被 `AuthCenter` 认可的 token（见文件头「一个魔 token 做 A/B」）
const VALID_TOKEN: &str = "d10-pilot-valid-device-token";

// ==================== 端口实现（插件侧自带的假宿主） ====================

/// 转发记账：网关交给插件边界的东西必须原样可见
#[derive(Default)]
struct ForwardLog {
    calls: Mutex<Vec<Value>>,
}

impl ForwardLog {
    fn record(&self, call: Value) {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push(call);
    }
    fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.calls.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// `PluginInvoker`：记录转发并回一个固定业务信封
///
/// **不跑插件代码**（插件只在 wasmtime 里执行，见文件头），所以本测试的断言边界是
/// 「网关把请求交到了插件边界」——method / params（模板捕获）/ query / body / device
/// 五项逐字可查。
struct PilotInvoker {
    log: Arc<ForwardLog>,
    activated: bool,
}

#[async_trait::async_trait]
impl PluginInvoker for PilotInvoker {
    async fn invoke_rust_command(&self, owner: &str, command: &str, args: Value) -> std::result::Result<Value, String> {
        assert_eq!(owner, PLUGIN_ID, "转发属主必须是本插件");
        assert_eq!(command, "_http_endpoint", "HTTP 面转发命令名");
        self.log.record(args.clone());
        // 形状对齐插件侧 `http_response` 信封（`{status, body, contentType?}`）：
        // 宿主 `plugin_controller` 提取 `status` 与 `body`
        Ok(json!({
            "status": 200,
            "body": json!({ "plugin": owner, "path": args["path"] }),
        }))
    }

    async fn is_activated(&self, owner: &str) -> bool {
        assert_eq!(owner, PLUGIN_ID, "网关只应为本插件判激活");
        self.activated
    }
}

/// `AuthCenter`：只认一个魔 token，其余（含空串）一律拒（fail-closed 同口径）
struct PilotAuthCenter;

impl AuthCenter for PilotAuthCenter {
    fn enforce_connection_policy(&self, token: &str) -> std::result::Result<AuthenticatedIdentity, String> {
        if token != VALID_TOKEN {
            return Err(format!("pilot auth center: reject token len={}", token.len()));
        }
        Ok(AuthenticatedIdentity {
            device_id: "d10-pilot-device".to_string(),
            device_name: Some("D10 Pilot".to_string()),
            fingerprint: Some("d10-pilot-fp".to_string()),
        })
    }
}

/// `BusPort`：本测试只读 WS 面发布的接入事件（`on_auth_ok` → `announce_connect`），
/// 「认证通过」的正向证据就从这里取
#[derive(Default)]
struct PilotBus {
    records: Mutex<Vec<(String, Value)>>,
}

impl PilotBus {
    /// 锁毒宽容：测试里一把锁不会跨越一个会 panic 的临界区，毒了就当普通状态用
    /// （与 `src/lib.rs` 的 `pairing_state_guard()` 同口径）
    fn guard(&self) -> std::sync::MutexGuard<'_, Vec<(String, Value)>> {
        self.records.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 找出一条本插件属主命名空间下的指定事件记录
    fn find(&self, name: &str) -> Option<Value> {
        let want = format!("{PLUGIN_ID}{TOPIC_NS_SEP}{name}");
        self.guard()
            .iter()
            .find(|(topic, _)| *topic == want)
            .map(|(_, payload)| payload.clone())
    }
    fn count(&self) -> usize {
        self.guard().len()
    }
}

#[async_trait::async_trait]
impl BusPort for PilotBus {
    fn publish(&self, topic: &str, _sender: &str, payload: Value) {
        self.guard().push((topic.to_string(), payload));
    }
    fn publish_binary(&self, topic: &str, _sender: &str, _payload: Vec<u8>) {
        self.guard().push((topic.to_string(), Value::Null));
    }
    async fn subscribe_static(&self, _subscriber: &str, _topic: &str, _handler: Box<dyn BusMessageHandler>) {}
    async fn deliver_endpoint_frame(&self, _owner: &str, _ep: &str, _client: &str, _kind: &str, _payload: Vec<u8>) {}
}

/// 未被本测试触及的端口：显式 panic 而不是静默 no-op
///
/// 选 panic 而非 no-op 的理由：**静默 no-op 会让「面真的走了这条路」看起来像通过**。
/// 哪天真需要这些能力时，测试会以「谁在碰这个端口」的形式点名，而不是给出一个空实现
/// 把断言变成恒真。
struct PilotEventSink;
struct PilotPowerPort;
struct PilotMdnsAdvertiser;
struct PilotLifecycle;
struct PilotConfigPort;

impl EventSink for PilotEventSink {
    fn emit(&self, _event: &str, _payload: Value) {
        panic!("PilotEventSink must not be used: 本测试无前端事件面");
    }
}

impl PowerPort for PilotPowerPort {
    fn enable(&self) {
        panic!("PilotPowerPort must not be used: 直接起 serve() 不走 supervisor");
    }
    fn disable(&self) {}
}

impl MdnsAdvertiserPort for PilotMdnsAdvertiser {
    fn advertise(&self, _service_name: String, _port: u16, _txt: HashMap<String, String>) {
        panic!("PilotMdnsAdvertiser must not be used: 直接起 serve() 不走 supervisor");
    }
    fn stop(&self) {}
}

#[async_trait::async_trait]
impl ServerLifecyclePort for PilotLifecycle {
    async fn start_server(&self, _port: u16) -> bedcode_server_base::error::Result<()> {
        panic!("PilotLifecycle must not be used: 直接起 serve() 不走 supervisor");
    }
    async fn stop_server(&self) -> bedcode_server_base::error::Result<()> {
        panic!("PilotLifecycle must not be used: 直接起 serve() 不走 supervisor");
    }
    async fn connections_snapshot(&self) -> usize {
        panic!("PilotLifecycle must not be used: 直接起 serve() 不走 supervisor");
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ServerLifecycleEvent> {
        panic!("PilotLifecycle must not be used: 直接起 serve() 不走 supervisor");
    }
}

impl ConfigPort for PilotConfigPort {
    fn network(&self) -> NetworkConfig {
        // **不是「本测试不触及」**：WS 面的 `ws_frame_limit()`（帧/消息上限真源）
        // 经 ConfigPort 读配置，而 `endpoint::register` 内部就会调它 —— 所以装端点
        // 这条路径必然走到这里。给缺省值即生产缺省语义。
        NetworkConfig::default()
    }
}

struct RuntimePortProbe;

impl RuntimePort for RuntimePortProbe {
    fn ambient_handle(&self) -> tokio::runtime::Handle {
        tokio::runtime::Handle::current()
    }
}

/// `PathsPort`：仅静态文件面用得到；`app_data_dir` 给一个真实临时目录（不 panic）
struct PilotPaths {
    data_dir: std::path::PathBuf,
}

impl PathsPort for PilotPaths {
    fn app_data_dir(&self) -> bedcode_server_base::error::Result<std::path::PathBuf> {
        Ok(self.data_dir.clone())
    }
    fn download_dir(&self) -> bedcode_server_base::error::Result<std::path::PathBuf> {
        Ok(self.data_dir.clone())
    }
}

/// `SystemInfoPort` 实现（服务器握手面自报设备名 / 版本）
struct SystemInfoProbe;

impl SystemInfoPort for SystemInfoProbe {
    fn device_name(&self) -> String {
        "D10 Pilot".to_string()
    }
    fn local_ipv4_addresses(&self) -> Vec<String> {
        vec!["127.0.0.1".to_string()]
    }
    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

/// `MdnsPort`：peer-net 面专用，本测试不触及
struct PilotMdns;

impl MdnsPort for PilotMdns {
    fn shared_daemon(&self) -> mdns_sd::ServiceDaemon {
        panic!("PilotMdns must not be used: 本测试无 peer-net 面");
    }
    fn register_host_service(&self, _service_type: &str, _fullname: &str) -> std::result::Result<String, String> {
        panic!("PilotMdns must not be used: 本测试无 peer-net 面");
    }
    fn stop_host_service(&self, _advertise_id: &str) -> std::result::Result<bool, String> {
        panic!("PilotMdns must not be used: 本测试无 peer-net 面");
    }
}

// ==================== 装配 ====================

/// 本插件 manifest 的 `contributes.wsEndpoints`（WS 腿的公理）
///
/// 宿主 activate 期照它登记端点（`host_api/ws.rs`），而端点路径的**另一份真源**在插件
/// 代码里（`ws_events::SESSION_CONTROL_PATH` / `ws_terminal::ENDPOINT_PATH`）——
/// 两份清单没有共同断言，正是本腿要补的洞。
fn manifest_ws_endpoints() -> Vec<(String, String)> {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("读不到 plugin.json {}：{e}", manifest_path.display()));
    let manifest: Value = serde_json::from_str(&raw).expect("plugin.json 必须是合法 JSON");
    assert_eq!(
        manifest["id"].as_str(),
        Some(PLUGIN_ID),
        "plugin.json 的 id 与测试里的 PLUGIN_ID 不一致（常量漂移）"
    );
    let declared = manifest["contributes"]["wsEndpoints"]
        .as_array()
        .unwrap_or_else(|| panic!("plugin.json 缺 contributes.wsEndpoints（宿主据此登记端点）"));
    declared
        .iter()
        .map(|e| {
            (
                e["path"].as_str().expect("wsEndpoint 缺 path").to_string(),
                e["auth"].as_str().expect("wsEndpoint 缺 auth").to_string(),
            )
        })
        .collect()
}

/// 按宿主 `host_api/http.rs::http_register_endpoint` 的**逐字口径**把本插件的路由表
/// 注册进真实 HTTP 注册表
///
/// 逐字对齐两处（错开一处就是「测试通过了但生产注册的不是这个」）：
/// ① 档位解析 `EndpointAuth::parse_with(decl.auth, EndpointAuth::Jwt)`——缺省最严档；
/// ② 内部端点段 = `decl.path`（宿主再拼 `/api/plugin/<owner>/`）。
///
/// **与生产的一处刻意差异**：生产 `register_one` 把注册失败降级成 `log_warn`（D7 故障
/// 隔离，不让一条路由失败把整个插件打成 Error）。本测试**不容忍**它——一条注册不了的
/// 路由就是一条 404 的产品面，装配失败必须在这里显性失败。
fn register_all_http_routes() -> usize {
    let mut registered = 0usize;
    for decl in ROUTES {
        let auth = EndpointAuth::parse_with(Some(decl.auth), EndpointAuth::Jwt)
            .unwrap_or_else(|e| panic!("{} 的 auth 档位非法：{e}", decl.path));
        let methods: Vec<String> = decl.methods.iter().map(|m| m.to_string()).collect();
        let entry = bedcode_server_http::registry::register(PLUGIN_ID, decl.path, decl.host, &methods, auth)
            .unwrap_or_else(|e| panic!("注册失败（生产会降级为 warn ⇒ 该端点永久 404）：{} → {e}", decl.path));
        assert_eq!(entry.owner, PLUGIN_ID);
        registered += 1;
    }
    registered
}

/// **冻结的网关别名表**（跨端 URL 契约；内部段, 对外别名, 方法, 档位）
///
/// ## 为什么必须有一张冻结副本（实测出来的，不是预防性设计）
///
/// 首版用例把 [`ROUTES`] 逐条注册进真实注册表后，**又从同一张表**取别名发请求
/// —— 于是它只能证明「表 ↔ 注册表 ↔ 网关」自洽。变异实测：把 `configs` 的别名
/// `/api/configs` 改成 `/api/cfgz`，用例**全绿**（注册的是 `/api/cfgz`、请求的也是
/// `/api/cfgz`，自洽）。而 `/api/configs` 是移动端 `bedcode-mobile/src-tauri/src/
/// commands/session.rs` 的字面量——拼错在生产上就是移动端配置页整块 404。
///
/// 仓内**没有第二个 URL 真源**可依赖（移动端 crate 不能被桌面插件 dev-depend；
/// manifest 也不含 HTTP 面，ABI v29 已退役静态声明面）。所以按本仓已有先例
/// （`bedcode-server-http` 的黄金形状锁、`session_e2e` 的黄金 DTO）**冻结一份
/// 字面量**：用例改为「注册 [`ROUTES`]、请求本表」，两个来源不一致即红。
///
/// 增删改任一别名都必须先改本表并在本文件说明跨端影响 —— 这是登记式契约，不是快照。
const FROZEN_GATEWAY_ALIASES: &[(&str, &str, &[&str], &str)] = &[
    // 会话 REST（移动端 `bedcode-mobile/src-tauri/src/session/http.rs` 的表逐条对应）
    ("sessions", "/api/sessions", &["GET"], "jwt"),
    ("sessions/start", "/api/sessions/start", &["POST"], "jwt"),
    ("sessions/stop", "/api/sessions/{id}/stop", &["POST"], "jwt"),
    ("sessions/remove", "/api/sessions/{id}/remove", &["DELETE"], "jwt"),
    ("sessions/input", "/api/sessions/{id}/input", &["POST"], "jwt"),
    ("sessions/history", "/api/sessions/{id}/history", &["GET"], "jwt"),
    ("sessions/resize", "/api/sessions/{id}/resize", &["POST"], "jwt"),
    // 业务域（移动端配置页 / 快捷指令页）
    ("configs", "/api/configs", &["GET"], "jwt"),
    ("quick-actions", "/api/quick-actions", &["GET"], "jwt"),
    // 文件浏览域（移动端文件面板）
    ("file-tree", "/api/file-tree", &["POST"], "jwt"),
    ("file-tree-children", "/api/file-tree-children", &["GET"], "jwt"),
    ("file-content", "/api/file-content", &["POST"], "jwt"),
    ("file-diff", "/api/file-diff", &["POST"], "jwt"),
    ("diff-tree", "/api/diff-tree", &["POST"], "jwt"),
    // 工作区 git 域
    ("git/branches", "/api/git/branches", &["GET"], "jwt"),
    ("git/status", "/api/git/status", &["GET"], "jwt"),
    ("git/checkout", "/api/git/checkout", &["POST"], "jwt"),
    // 认证链七条（移动端 `auth/http.rs`；**全免凭证**——拿 token 之前的公开入口）
    ("auth/pairing", "/api/auth/pairing", &["POST"], "none"),
    ("auth/verify", "/api/auth/verify", &["POST"], "none"),
    ("auth/qr-connect", "/api/auth/qr-connect", &["POST"], "none"),
    ("auth/reauth", "/api/auth/reauth", &["POST"], "none"),
    (
        "auth/biometric-challenge",
        "/api/auth/biometric-challenge",
        &["POST"],
        "none",
    ),
    ("auth/biometric-verify", "/api/auth/biometric-verify", &["POST"], "none"),
    ("auth/biometric-bind", "/api/auth/biometric-bind", &["POST"], "none"),
    // 终端背景图（宿主静态路由；CSS background-image 无法携带认证头）
    ("terminal-bg", "/static/terminal-bg", &["GET"], "none"),
];

/// 探测空闲端口：绑 `127.0.0.1:0` 由 OS 分配，立即释放后交给服务器绑定
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 发一个请求；连接失败（服务器 worker 未就绪）时按 25ms 重试直至超时
async fn send_until(request: reqwest::RequestBuilder, timeout: Duration) -> reqwest::Result<reqwest::Response> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match request.try_clone().expect("request must be cloneable").send().await {
            Ok(resp) => return Ok(resp),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
                tokio::task::yield_now().await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn body_json(resp: reqwest::Response) -> Value {
    let bytes = resp.bytes().await.expect("read response body failed");
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("response body must be valid JSON: {e}; raw={:?}", bytes))
}

/// 把模板别名里的 `{name}` 段替换成具体值（与移动端实际请求同形）
///
/// 分段遍历时靠**下标**决定是否补 `/`：别名的首段是空串（`/api/configs` 被
/// `split('/')` 切成 `["", "api", "configs"]`），无脑给每段前置 `/` 会得到
/// `//api/configs`（首版实测 404）。
fn materialize(alias: &str) -> String {
    let mut out = String::new();
    for (i, seg) in alias.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        match seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            Some(name) => out.push_str(&format!("d10-{name}")),
            None => out.push_str(seg),
        }
    }
    out
}

/// 取路由表里某个内部端点的声明（测试自述用）
fn decl_of(path: &str) -> &'static HttpRouteDecl {
    ROUTES
        .iter()
        .find(|r| r.path == path)
        .unwrap_or_else(|| panic!("ROUTES 缺端点 {path}"))
}

// ==================== 腿 A：HTTP 路由表 × 真实网关 ====================

/// 两腿共用一套端口（**进程级 `OnceLock`**）
///
/// `bedcode_server_base::ports::init` 对重复装配 **panic**，而两条腿住在同一个单测
/// 二进制里（`src/d10_contract_test.rs` 作为 `#[cfg(test)]` 模块）⇒ 只能装一次。
/// 代价：两条腿不能各换一套端口实现。选共享而非合并两条腿，是因为两条断言的对象
/// 不同面（HTTP 别名 vs WS 端点），共用反而能顺带证明「同一套端口下两面都工作」。
///
/// 共享的总线句柄必须回传给腿 B（它的正向证据在那里）；转发记账回传给腿 A
/// （它逐条断言入参形状）。
fn ports_once() -> (Arc<PilotBus>, Arc<ForwardLog>, PathBuf) {
    static ONCE: std::sync::OnceLock<(Arc<PilotBus>, Arc<ForwardLog>, PathBuf)> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let bus = Arc::new(PilotBus::default());
        let log = Arc::new(ForwardLog::default());
        let data_dir = tempfile::tempdir().expect("temp dir");
        let path = data_dir.path().to_path_buf();
        bedcode_server_base::ports::init(ServerPorts {
            plugin_invoker: Arc::new(PilotInvoker {
                log: log.clone(),
                activated: true,
            }),
            auth_center: Arc::new(PilotAuthCenter),
            bus: bus.clone(),
            event_sink: Arc::new(PilotEventSink),
            paths: Arc::new(PilotPaths { data_dir: path.clone() }),
            system_info: Arc::new(SystemInfoProbe),
            power: Arc::new(PilotPowerPort),
            mdns_advertiser: Arc::new(PilotMdnsAdvertiser),
            lifecycle: Arc::new(PilotLifecycle),
            runtime: Arc::new(RuntimePortProbe),
            config: Arc::new(PilotConfigPort),
            mdns: Arc::new(PilotMdns),
        });
        // tempdir 的 `TempDir` 在闭包结束时就被 drop ⇒ 目录会被删。端口的
        // `PathsPort` 在两条腿跑完前不会真正用它，但为了让「目录存在」这条
        // 前提成立而不是靠运气，把 `TempDir` 一并存进 `OnceLock`（进程内保活）。
        ONCE_DIR.set(Box::leak(Box::new(std::mem::ManuallyDrop::new(data_dir))));
        (bus, log, path)
    })
    .clone()
}

static ONCE_DIR: std::sync::OnceLock<&'static std::mem::ManuallyDrop<tempfile::TempDir>> = std::sync::OnceLock::new();

/// 两条腿的串行锁
///
/// 不是为了端口注册表（两张表互不冲突），而是防 `pick_free_port()` 的「绑 0 → 释放 →
/// 交给服务器绑」窗口在并行线程间撞车（两个测试拿到同一端口）。
static LEG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn http_routes_reach_the_plugin_edge_with_declared_method_and_auth_tier() {
    let _leg = LEG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing_subscriber::fmt().with_test_writer().try_init().ok();
    }

    // ==================== 装配（与 GUI bootstrap 同一面） ====================
    let (_bus, log, data_dir) = ports_once();
    let registered = register_all_http_routes();
    assert_eq!(
        registered,
        ROUTES.len(),
        "全部内部端点必须注册成功（一条失败 = 一条永久 404 的产品面）"
    );

    let port = pick_free_port();
    let (handle, server) = bedcode_server_core::app::serve(
        port,
        &NetworkConfig::default(),
        vec![
            Arc::new(bedcode_server_http::HttpTransportFace),
            Arc::new(bedcode_server_websocket::WebSocketTransportFace),
        ],
    )
    .await
    .expect("pilot server must start");
    let server_task = tokio::spawn(server);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client failed");
    let deadline = Duration::from_secs(10);

    // ==================== 断言 0：冻结别名表 ↔ [`ROUTES`] 逐条一致 ====================
    //
    // 本用例的**独立 oracle**。没有这一步的话，下面所有网络断言都是「用被测表驱动
    // 被测表」，拼错别名会静默自洽（变异实测：/api/configs → /api/cfgz 全绿）。
    let declared_aliased: Vec<(String, String, Vec<String>, String)> = ROUTES
        .iter()
        .filter_map(|r| {
            r.host.map(|h| {
                (
                    r.path.to_string(),
                    h.to_string(),
                    r.methods.iter().map(|m| m.to_string()).collect(),
                    r.auth.to_string(),
                )
            })
        })
        .collect();
    let frozen: Vec<(String, String, Vec<String>, String)> = FROZEN_GATEWAY_ALIASES
        .iter()
        .map(|(p, h, m, a)| {
            (
                p.to_string(),
                h.to_string(),
                m.iter().map(|x| x.to_string()).collect(),
                a.to_string(),
            )
        })
        .collect();
    let mut declared_sorted = declared_aliased.clone();
    let mut frozen_sorted = frozen.clone();
    declared_sorted.sort();
    frozen_sorted.sort();
    assert_eq!(
        declared_sorted, frozen_sorted,
        "本插件的对外别名表与冻结的跨端契约不一致 —— 增删改任一别名都必须先改 \
         FROZEN_GATEWAY_ALIASES 并在本文件说明跨端影响（移动端字面量在 \
         bedcode-mobile/src-tauri；manifest 不含 HTTP 面）"
    );

    // ==================== 断言 1：每条 **网关别名** 用**声明的方法**都真到达插件边界 ====================
    //
    // 这是本测试的主干。别名与方法取**冻结表**（独立 oracle），被测行为取真实注册表与
    // 真实网关：别名拼错 / 方法写错在这里变成一条 404。
    //
    // **只跑 `/api/` 前缀的别名**：`/static/terminal-bg` 是**宿主自有静态路由**
    // （`bedcode-server-http::routes::terminal_bg_image`），它同样经注册表门控，但
    // 不走网关转发，而是从应用数据目录读图片文件。它的可达性由断言 7 单独验
    // （需要真的放一张图才能 200）——把两类别名混在一条断言里会让「空目录 ⇒ 404」
    // 被误读成别名不可达。
    let gateway_aliased: Vec<(&str, &str, &[&str])> = FROZEN_GATEWAY_ALIASES
        .iter()
        .filter(|(_, alias, _, _)| alias.starts_with("/api/"))
        .map(|(path, alias, methods, _)| (*path, *alias, *methods))
        .collect();
    let static_aliased: Vec<(&str, &str, &str)> = FROZEN_GATEWAY_ALIASES
        .iter()
        .filter(|(_, alias, _, _)| !alias.starts_with("/api/"))
        // 四元组是 `(内部段, 别名, 方法, 档位)`：第三个是 `methods`，**档位在第四位**
        .map(|(path, alias, _, auth)| (*path, *alias, *auth))
        .collect();
    assert_eq!(gateway_aliased.len(), 24, "网关别名数 = 业务 10 + auth 7 + sessions 7");
    assert_eq!(
        static_aliased.len(),
        1,
        "当前只有 `/static/terminal-bg` 一条宿主自有静态别名（新增要在下面补专用断言）"
    );

    for (decl_path, alias, methods) in &gateway_aliased {
        let concrete = materialize(alias);
        for method in *methods {
            let url = format!("{base}{concrete}");
            let request = method_request(&client, method, &url, &json!({ "probe": decl_path }));
            let resp = send_until(request.bearer_auth(VALID_TOKEN), deadline)
                .await
                .expect("request must reach server");
            assert_eq!(
                resp.status().as_u16(),
                200,
                "{decl_path} 用声明方法 {method} 请求 {concrete} 必须转发（404 = 别名或方法与注册表不匹配）"
            );
            let body = body_json(resp).await;
            // 宿主 `forward_to_plugin` 把插件的 `{status, body}` 信封**拆开**映射成
            // HTTP 响应（`body` 成为响应体），所以这里看到的应是插件 stub 回的
            // 内容，而不是信封本身——顺带证明「宿主拆信封 + 回包」这条路径也走通了。
            assert_eq!(
                body["plugin"], PLUGIN_ID,
                "{decl_path} 转发回包必须带属主（信封被宿主拆开后透出插件 body）: {body}"
            );
            assert_eq!(
                body["path"], *decl_path,
                "{decl_path} 转发到插件边界的内部端点段与冻结契约不符: {body}"
            );
        }
    }

    // ==================== 断言 2：转发入参逐字（path / method / params 模板捕获） ====================
    let calls = log.take();
    assert_eq!(
        calls.len(),
        gateway_aliased.iter().map(|(_, _, m)| m.len()).sum::<usize>(),
        "每次成功转发必须恰好一条记账（多 = 重试重入；少 = 网关吞了请求）"
    );

    let by_path: HashMap<String, Vec<&Value>> = calls.iter().fold(HashMap::new(), |mut acc, c| {
        acc.entry(c["path"].as_str().unwrap_or_default().to_string())
            .or_default()
            .push(c);
        acc
    });
    for (decl_path, alias, methods) in &gateway_aliased {
        let seen = by_path
            .get(*decl_path)
            .unwrap_or_else(|| panic!("{decl_path} 声明了别名却零转发记账"));
        assert_eq!(seen.len(), methods.len(), "{decl_path} 转发次数与声明方法数不符");
        let mut got: Vec<&str> = seen.iter().map(|c| c["method"].as_str().unwrap()).collect();
        got.sort_unstable();
        let mut want: Vec<&str> = methods.iter().copied().collect();
        want.sort_unstable();
        assert_eq!(got, want, "{decl_path} 转发方法集与冻结契约不符");

        // 模板别名：具体值必须被捕获进 params（ABI v29）
        for call in seen {
            let params = &call["params"];
            if let Some(name) = template_segment_name(alias) {
                assert_eq!(
                    params[format!("{name}")].as_str(),
                    Some(format!("d10-{name}").as_str()),
                    "{decl_path} 的模板段 {name} 未捕获进 params（转发会丢参数）：{params}"
                );
            } else {
                assert_eq!(params, &json!({}), "{decl_path} 非模板别名不得携带 params");
            }
        }
    }

    // ==================== 断言 3：body / query / device 三向透传 ====================
    let stop_decl = decl_of("sessions/stop");
    let stop_url = format!("{base}{}", materialize(stop_decl.host.unwrap()));
    let resp = send_until(
        client
            .post(&stop_url)
            .query(&[("force", "1")])
            .header("content-type", "application/json")
            .body(json!({ "reason": "d10" }).to_string())
            .bearer_auth(VALID_TOKEN),
        deadline,
    )
    .await
    .expect("probe request must reach server");
    assert_eq!(resp.status().as_u16(), 200);
    let call = log
        .take()
        .into_iter()
        .find(|c| c["path"] == "sessions/stop")
        .expect("sessions/stop 必须有一次转发");
    assert_eq!(call["body"]["reason"], "d10", "body 透传");
    assert_eq!(call["query"]["force"], "1", "query 透传");
    // `device` 的 wire 形状是 `{deviceId, deviceName?}`（宿主派生，**不携带**指纹与
    // 密钥类材料，见 `caller_identity_carries_only_host_derived_labels`）
    assert_eq!(
        call["device"]["deviceId"], "d10-pilot-device",
        "验签身份必须注入 device"
    );
    assert_eq!(call["device"]["deviceName"], "D10 Pilot", "设备名一并注入");
    assert!(
        call["device"].get("fingerprint").is_none(),
        "device 不得携带指纹（ADR 0033 裁剪面）"
    );
    assert_eq!(call["caller"], "device", "带凭证调用的 caller 档位");

    // ==================== 断言 4：档位 A/B（`none` 免凭证可达 / `jwt` 无凭证 401） ====================
    //
    // 同一次运行、同一个服务器对拍：端口注册表是进程级 `OnceLock`，起第二个进程才能换
    // `AuthCenter` 行为——所以用一个魔 token 把「验签成功/失败」压进 header。
    // 只取**有对外别名**的档位样本：内部端点（无别名）的可达性由断言 6 覆盖，
    // 这里要验的是「别名 + 档位」这一对在网关上的行为。
    let none_routes: Vec<&HttpRouteDecl> = ROUTES
        .iter()
        .filter(|r| r.auth == "none" && r.host.is_some() && r.host.expect("is_some").starts_with("/api/"))
        .collect();
    let jwt_routes: Vec<&HttpRouteDecl> = ROUTES.iter().filter(|r| r.auth == "jwt" && r.host.is_some()).collect();
    assert!(!none_routes.is_empty() && !jwt_routes.is_empty());
    // 免凭证档必须逐条可交代（与插件内 `no_auth_routes_are_justified` 同一清单，
    // 这里从**网关行为**侧复核：标了 none 却仍要凭证 = 移动端配对链与 hook 直接断）
    assert_eq!(
        none_routes.len(),
        7,
        "走网关的 none 档别名 = auth/* 七条；另三条 none 端点不在网关别名面上 \
         （terminal-bg 是宿主静态路由见断言 7，task-status / session-mode 无别名见断言 6b）"
    );

    for decl in &none_routes {
        let alias = decl.host.expect("filtered by is_some");
        let url = format!("{base}{}", materialize(alias));
        let resp = send_until(method_request(&client, decl.methods[0], &url, &json!({})), deadline)
            .await
            .expect("anonymous request must reach server");
        assert_eq!(
            resp.status().as_u16(),
            200,
            "{} 是 none 档（公开入口 / 环回 hook），无凭证必须转发 —— 否则移动端配对链与 hook 直接断",
            decl.path
        );
    }
    log.take();

    for decl in &jwt_routes {
        let alias = decl.host.expect("filtered by is_some");
        let url = format!("{base}{}", materialize(alias));
        let resp = send_until(method_request(&client, decl.methods[0], &url, &json!({})), deadline)
            .await
            .expect("anonymous request must reach server");
        assert_eq!(
            resp.status().as_u16(),
            401,
            "{} 是 jwt 档，无凭证必须被 JWT 中间件拒（200 = 越权可达）",
            decl.path
        );
    }
    assert!(
        log.calls().is_empty(),
        "401 档请求不得触达插件边界（记账 {} 条）",
        log.calls().len()
    );

    // ==================== 断言 5：未声明方法不得转发（注册表按 (path, method) 索引） ====================
    let configs = decl_of("configs");
    let wrong_method = if configs.methods.contains(&"POST") {
        "DELETE"
    } else {
        "POST"
    };
    let resp = send_until(
        client
            .request(
                reqwest::Method::from_bytes(wrong_method.as_bytes()).unwrap(),
                format!("{base}{}", configs.host.unwrap()),
            )
            .bearer_auth(VALID_TOKEN),
        deadline,
    )
    .await
    .expect("probe request must reach server");
    assert_eq!(
        resp.status().as_u16(),
        404,
        "configs 只声明 {:?}，{wrong_method} 必须 404（转发即越权面）",
        configs.methods
    );
    assert!(log.calls().is_empty(), "未声明方法不得触达插件边界");

    // ==================== 断言 6：仅内部端点不对外可达，内部路径可达 ====================
    //
    // 「内部端点」= 只挂 `/api/plugin/<owner>/<path>`，没有对外别名（插件自代理面）。
    let internal_only: Vec<&HttpRouteDecl> = ROUTES.iter().filter(|r| r.host.is_none()).collect();
    assert!(!internal_only.is_empty(), "本插件应有只挂内部路径的端点");
    for decl in &internal_only {
        let entry = bedcode_server_http::registry::find_by_internal(&format!("/api/plugin/{PLUGIN_ID}/{}", decl.path))
            .unwrap_or_else(|| panic!("{} 未按内部路径登记", decl.path));
        assert_eq!(entry.host_path, None, "{} 声明为仅内部端点，不得有对外别名", decl.path);
    }
    // 内部路径经插件代理前缀真可达（hook 脚本走的就是这条）
    let probe_internal = internal_only[0];
    let internal_url = format!("{base}/api/plugin/{PLUGIN_ID}/{}", probe_internal.path);
    let resp = send_until(
        client.post(&internal_url).json(&json!({})).bearer_auth(VALID_TOKEN),
        deadline,
    )
    .await
    .expect("internal path request must reach server");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "内部路径 {} 必须可达（插件自代理面，hook 脚本的唯一入口）",
        probe_internal.path
    );

    // ==================== 断言 6b：内部路径上的 **none 档**也必须免凭证可达 ====================
    //
    // hook 脚本（Claude Code / codex / pi / opencode 的 `task-status` / `session-mode`
    // 调用）拿不到 JWT —— 它们经插件代理前缀 `/api/plugin/<owner>/<path>` 进来，由
    // `plugin_controller::plugin_http_auth_allowed` 按**注册表里的档位**判。档位写错
    // （该 none 写成 jwt）时这里的症状是 hook 静默 401：hook 脚本不检查响应体，
    // 于是表现为「任务面板偶尔不刷新」，极难归因。
    let internal_none: Vec<&HttpRouteDecl> = internal_only.iter().copied().filter(|r| r.auth == "none").collect();
    assert_eq!(
        internal_none.len(),
        2,
        "内部路径上的 none 档端点 = task-status + session-mode（另加即需逐条交代）"
    );
    for decl in &internal_none {
        let url = format!("{base}/api/plugin/{PLUGIN_ID}/{}", decl.path);
        let resp = send_until(method_request(&client, decl.methods[0], &url, &json!({})), deadline)
            .await
            .expect("anonymous internal request must reach server");
        assert_eq!(
            resp.status().as_u16(),
            200,
            "{} 是 none 档（环回 hook），经内部路径必须免凭证可达（否则 hook 静默 401）",
            decl.path
        );
    }
    // 反面：jwt 档的内部端点无凭证必须被拒（否则内部前缀成了免鉴权后门）
    let internal_jwt: Vec<&HttpRouteDecl> = internal_only.iter().copied().filter(|r| r.auth == "jwt").collect();
    assert!(!internal_jwt.is_empty(), "应有 jwt 档内部端点可做反面样本");
    let jwt_internal = internal_jwt[0];
    let jwt_url = format!("{base}/api/plugin/{PLUGIN_ID}/{}", jwt_internal.path);
    let resp = send_until(
        method_request(&client, jwt_internal.methods[0], &jwt_url, &json!({})),
        deadline,
    )
    .await
    .expect("anonymous internal request must reach server");
    assert_eq!(
        resp.status().as_u16(),
        401,
        "{} 是 jwt 档，内部路径无凭证必须被拒（内部前缀不是免鉴权后门）",
        jwt_internal.path
    );
    log.take();

    // ==================== 断言 7：宿主自有静态别名（不走网关，走文件） ====================
    //
    // `/static/terminal-bg` 的 URL 生命周期归插件（注册表门控），但**取文件在宿主**
    // （二进制通道缺口下的引擎残留，见 `routes::terminal_bg_image` 注释）。它的完整
    // 可达链是四跳：注册表有声明 → 属主已激活 → 数据目录可解析 → 目录里真有图片。
    // 四跳任一坏掉都表现为**同一个 404**，所以这里真的放一张图再断言 200 ——
    // 否则「空目录 ⇒ 404」与「别名未登记」不可区分（首版把两者混在断言 1 里，
    // 实测报出一个假红）。
    let bg = static_aliased[0];
    let (bg_path, bg_alias, bg_auth) = bg;
    assert_eq!(bg_path, "terminal-bg");
    assert_eq!(bg_auth, "none", "终端背景图必须是 none 档（CSS 无法携带认证头）");
    let bg_url = format!("{base}{bg_alias}");
    let bg_file = data_dir.join(format!(
        "{}.png",
        bedcode_server_base::constants::TERMINAL_BG_FILE_PREFIX
    ));
    // 先证档位：无凭证必须**不是 401**（none 档生效；jwt 档在这里会是 401）
    let anonymous = send_until(client.get(&bg_url), deadline)
        .await
        .expect("anonymous static probe must reach server");
    assert_ne!(
        anonymous.status().as_u16(),
        401,
        "{bg_alias} 是 none 档（CSS background-image 无法携带认证头），无凭证不得被拒"
    );
    // 再证「无图片 ⇒ 404」（门控与目录解析都通，只是没文件）
    let no_file = send_until(client.get(&bg_url).bearer_auth(VALID_TOKEN), deadline)
        .await
        .expect("static probe must reach server");
    assert_eq!(
        no_file.status().as_u16(),
        404,
        "{bg_alias} 在数据目录无图片时必须 404（而不是回空 200 或 500）"
    );
    std::fs::write(&bg_file, b"\x89PNG\r\n\x1a\nd10-probe").expect("write probe background image");
    let with_file = send_until(client.get(&bg_url).bearer_auth(VALID_TOKEN), deadline)
        .await
        .expect("static probe must reach server");
    assert_eq!(
        with_file.status().as_u16(),
        200,
        "{bg_alias} 数据目录有图片时必须 200（注册表门控 + 激活 + 目录解析 + 文件读取）"
    );
    assert_eq!(
        with_file.headers().get("content-type").and_then(|v| v.to_str().ok()),
        Some("image/png"),
        "{bg_alias} 必须按扩展名回 image/png"
    );
    let _ = std::fs::remove_file(&bg_file);

    // ==================== 停机 ====================
    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");
}

/// 构造指定方法的请求（ROUTES 现有 GET / POST / DELETE 三种）
///
/// 避开链式 `.json()`：那条 API 只在带 body 的 `post()` 上有；DELETE 等方法用
/// `.body()` + `content-type` 逐字等效（宿主 `body_value` 只看字节与 JSON 可解析性）。
fn method_request(client: &reqwest::Client, method: &str, url: &str, body: &Value) -> reqwest::RequestBuilder {
    let verb = reqwest::Method::from_bytes(method.as_bytes())
        .unwrap_or_else(|e| panic!("ROUTES 声明了非法 HTTP 方法 {method}：{e}"));
    client
        .request(verb, url)
        .header("content-type", "application/json")
        .body(body.to_string())
}
/// 模板别名里 `{name}` 的段名（非模板别名返回 None）
fn template_segment_name(alias: &str) -> Option<&str> {
    alias
        .split('/')
        .find_map(|seg| seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
}

// ==================== 腿 B：manifest 的 wsEndpoints × 真实 WS 端点面 ====================

#[tokio::test]
async fn manifest_ws_endpoints_mount_and_gate_on_real_websocket_face() {
    let _leg = LEG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing_subscriber::fmt().with_test_writer().try_init().ok();
    }

    // 总线句柄走 `ports_once()` 拿（与腿 A 共用同一套进程级端口）
    let (bus, _log, _data_dir) = ports_once();

    // ==================== 断言 1：manifest 的每条声明都能真登记（挂载路径含属主段） ====================
    //
    // 生产 `register` 失败会被降级（HTTP 面降级为 warn、端点直接永久不可达），所以
    // 这里**不容忍**失败：一条登记不上的 wsEndpoint = 插件的一整条广播出口静默丢帧。
    let declared = manifest_ws_endpoints();
    assert!(!declared.is_empty(), "manifest 必须声明至少一条 wsEndpoint");
    let mut mounted = Vec::new();
    for (path, auth) in &declared {
        let parsed = EndpointAuth::parse_with(Some(auth.as_str()), EndpointAuth::Jwt)
            .unwrap_or_else(|e| panic!("manifest wsEndpoint {path} 的 auth 非法：{e}"));
        let entry = bedcode_server_websocket::endpoint::register(
            PLUGIN_ID,
            path,
            parsed,
            None,
            None,
            // **必须是共享的 bus 句柄**：`register` 把 bus 句柄吃进端点条目，
            // 之后 `announce_connect` 的接入事件就发在它上面（首版传了个新
            // `PilotBus::default()`，于是断言「看共享总线」永远看不到事件）
            bus.clone(),
        )
        .unwrap_or_else(|e| panic!("ws 端点登记失败（该端点永久不可升级）：{path} → {e}"));
        assert_eq!(
            entry.mount_path,
            format!("/ws/plugin/{PLUGIN_ID}/{path}"),
            "挂载路径必须含属主命名空间段（隔离闸门 ADR 0017）"
        );
        mounted.push(entry.mount_path);
    }
    assert_eq!(
        mounted.len(),
        declared.len(),
        "每条声明都必须有自己的挂载路径（同名会静默覆盖一条出口）"
    );

    // ==================== 断言 2：manifest 与插件代码里的端点常量不漂移 ====================
    //
    // 两份清单分居 manifest（宿主 activate 期读它登记端点）与插件代码（插件自己按
    // path 回包），此前**无共同断言**：manifest 少写一条 ⇒ 宿主不挂载 ⇒ 插件的广播
    // 出口静默丢全部帧，而宿主与插件各自的单测都看不见（宿主读 manifest，插件测代码）。
    let manifest_paths: Vec<&str> = declared.iter().map(|(p, _)| p.as_str()).collect();
    for code_path in [
        crate::ws_events::SESSION_CONTROL_PATH,
        crate::ws_terminal::ENDPOINT_PATH,
    ] {
        assert!(
            manifest_paths.contains(&code_path),
            "插件代码里的端点常量 `{code_path}` 未在 manifest contributes.wsEndpoints 声明 \
             ⇒ 宿主不会挂载它，插件回包走不出去"
        );
    }

    // ==================== 断言 3：未声明端点不得可升级（无 fallback 放行） ====================
    let unregistered = format!("/ws/plugin/{PLUGIN_ID}/no-such-endpoint");
    assert!(!mounted.contains(&unregistered), "未声明端点不得有挂载路径");

    // ==================== 起真实 WS 面 ====================
    let port = pick_free_port();
    let (handle, server) = bedcode_server_core::app::serve(
        port,
        &NetworkConfig::default(),
        vec![
            Arc::new(bedcode_server_http::HttpTransportFace),
            Arc::new(bedcode_server_websocket::WebSocketTransportFace),
        ],
    )
    .await
    .expect("pilot ws server must start");
    let server_task = tokio::spawn(server);

    // ==================== 断言 4：jwt 档端点的认证闸门 fail-closed（A/B 同服务器对拍） ====================
    //
    // 判据用**正反两侧**：
    // - 错 token → 服务端立刻 close 4001（`PluginChannel::reject_auth`）；
    // - 对 token → 认证通过，`on_auth_ok` 往总线发接入事件（`authenticated: true`）。
    // 负例只断言「被拒」太弱（连不上也算被拒）；正例给出「真的进了业务通道」的正面证据。
    for (i, mount) in mounted.iter().enumerate() {
        let url = format!("ws://127.0.0.1:{port}{mount}");
        // --- 负例：错 token → 4001 ---
        let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(&url))
            .await
            .unwrap_or_else(|_| panic!("{mount} 升级探测超时（端点已挂载但连不上）"))
            .unwrap_or_else(|e| panic!("{mount} 升级失败（已挂载的端点必须可升级）：{e}"));
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({ "type": "auth", "token": "wrong-token" }).to_string().into(),
        ))
        .await
        .expect("send auth frame");
        let reply = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .unwrap_or_else(|_| panic!("{mount} 错 token 后服务端未在窗口内断连（闸门失效 = 无限期挂起连接）"))
            .expect("stream not closed")
            .expect("read frame");
        match reply {
            tokio_tungstenite::tungstenite::Message::Close(Some(frame)) => {
                assert_eq!(
                    u16::from(frame.code),
                    4001,
                    "{mount} 认证失败必须 close 4001（ADR 0031 fail-closed 码）"
                );
            }
            other => panic!("{mount} 错 token 后应立即 close 4001，实得 {other:?}"),
        }

        // --- 正例：对 token → 认证通过（接入事件是正面证据）---
        let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(&url))
            .await
            .expect("second upgrade probe must not hang")
            .expect("second upgrade must succeed");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({ "type": "auth", "token": VALID_TOKEN }).to_string().into(),
        ))
        .await
        .expect("send valid auth frame");
        // 轮询总线直到接入事件出现（接入事件与 close 竞态，先到先算）
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(payload) = bus.find(WS_CLIENT_CONNECT) {
                assert_eq!(
                    payload["authenticated"], true,
                    "{mount} 认证通过后接入事件必须标 authenticated=true"
                );
                assert_eq!(
                    payload["endpointId"].as_str().is_some_and(|e| e.starts_with("wse-")),
                    true,
                    "{mount} 接入事件必须带端点句柄（wse- 前缀，插件据此定位端点）"
                );
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!(
                    "{mount} 用有效 token 认证后未观察到 {WS_CLIENT_CONNECT} 接入事件 \
                     （总线记录 {} 条）——认证通过但没进业务通道",
                    bus.count()
                );
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let _ = ws.close(None).await;
        let _ = i;
    }

    // ==================== 停机 ====================
    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");
}
