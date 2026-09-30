//! 桌面端无头装配：真实 AppContext + 真实认证中心插件 + 真实 HTTP/WS 服务器
//!
//! 复刻 `bedcode-desktop/src-tauri/tests/pty_session_chain.rs`（`app_handle(None)`
//! 无头模式）。与桌面端单端测试的差别只有一处、也是本工程的全部意义：
//! **对面是移动端真实客户端代码**，不是通用 reqwest / tokio-tungstenite。
//!
//! **装配面必须与 GUI bootstrap 完全同款**（server-lib-split 票 07）：端口注册表
//! 经宿主组合根的单一装配点 [`bedcode_desktop_lib::server::composition`]
//! `install_server_ports` 装。历史上本 rig 只建 `AppContext` 不装端口，症状是
//! HTTP 网关 `ports::get() == None` 判 `PassThrough` → 插件在激活期登记的全部宿主
//! 别名（`/api/auth/*` / `/api/sessions*` / `/api/configs`）一律 404，而端口占用、
//! WS 升级等不依赖端口的路径照常工作——七个场景同时红、且看起来像协议问题。
//! 自检用例 `rig_assembles_the_same_server_ports_face_as_gui_boot` 钉住这条不变量。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bedcode_desktop_lib::db::Database;
use bedcode_desktop_lib::mdns::advertiser::MdnsAdvertiser;
// server-lib-split 票 03：内核组合入口 `server::core::app` 已下沉为
// `bedcode_server_core::app::serve`，宿主侧兼容面在 `server::composition`
// （签名与拆分前一致，调用点不必改）
use bedcode_desktop_lib::server::composition::{install_server_ports, start_http_server};
use bedcode_desktop_lib::system::app_context::{AppContext, AppContextBuilder};
use bedcode_desktop_lib::system::info::SystemInfo;
use bedcode_desktop_lib::wasm_core::PluginHost;
use bedcode_desktop_lib::AppConfig;

/// 认证中心 / 会话真源插件 id（`com.bedcode.terminal-session`：配对、JWT 签发、
/// 会话登记域、WS 端点编排全在它自己进程内）
pub const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 随包 wasm 产物目录（桌面端 `pnpm run tauri:build` / `plugin-build.js` 产出）
///
/// 产物缺失时**显性失败**而非跳过：跨端互连的唯一驱动方式就是真实产物，
/// 静默 skip 会把「未验证」伪装成「通过」（与 `pty_session_chain` 同口径）。
pub fn bundled_plugins_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bedcode-desktop/src-tauri/resources/plugins/desktop");
    let artifact = dir.join(format!("{SESSION_PLUGIN_ID}/bedcode_plugin_terminal_session.wasm"));
    assert!(
        artifact.exists(),
        "认证中心插件产物缺失：先在 bedcode-desktop/ 跑 `pnpm run tauri:build` \
         （或 node scripts/plugin-build.js --plugin {SESSION_PLUGIN_ID}），\
         期望产物：{}",
        artifact.display()
    );
    dir
}

/// 无头上下文的插件私有库根（配对 / 会话 / 认证记录真源在插件私有库，
/// 无头模式无 AppHandle，经 `set_plugin_db_root` 注入）
fn plugin_db_root() -> &'static PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bedcode-crossend-pluginroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    })
}

/// 用户插件目录（独立空目录；复用随包目录会把随包插件标成 UserInstalled 来源，
/// 激活时撞审批门禁）
fn user_plugins_dir() -> PathBuf {
    std::env::temp_dir().join(format!("bedcode-crossend-userplugins-{}", std::process::id()))
}

/// 组装真实服务 AppContext（`app_handle=None` 无头模式）+ 激活认证中心插件
///
/// 每个测试进程只 init 一次（`AppContext` 是进程级 `OnceLock` 单例）。
pub async fn init_app_context() {
    init_app_context_inner(true).await
}

/// 同上，但**不激活**认证中心（fail-closed 场景：先观察「无中心在册」的桌面
/// 端真实行为，再经 [`activate_session_center`] 激活取正例对照）
pub async fn init_app_context_without_center() {
    init_app_context_inner(false).await
}

/// 激活认证中心插件（`init_app_context_without_center` 之后用；可重复调用）
pub async fn activate_session_center() {
    AppContext::global()
        .plugin_host()
        .activate_plugin(SESSION_PLUGIN_ID, false)
        .await
        .expect("activate com.bedcode.terminal-session (bundled artifact)");
}

async fn init_app_context_inner(activate_center: bool) {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if INIT.get().is_some() {
        return;
    }
    {
        let db = Arc::new(tokio::sync::Mutex::new(
            Database::new(Path::new(":memory:")).expect("create in-memory db failed"),
        ));
        db.lock().await.init_schema().expect("init db schema failed");

        let plugins_dir = bundled_plugins_dir();
        let user_dir = user_plugins_dir();
        std::fs::create_dir_all(&user_dir).expect("create temp user plugins dir failed");

        let plugin_host = PluginHost::new(db.clone(), &plugins_dir, &user_dir, None).await;
        plugin_host.init_message_bus().await;
        plugin_host
            .wasm_host_ctx()
            .set_plugin_db_root(Some(plugin_db_root().clone()));

        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let system_info = Arc::new(SystemInfo::collect());

        // 顺序 = GUI bootstrap 的同一顺序（src-tauri/src/lib.rs setup）：
        // AppContext 先注册 → 端口装配（`assemble()` 取总线走 `AppContext::try_global`，
        // 早一步会装上占位空总线，插件的 `bus-subscribe` 便永收不到互调请求）→
        // 最后才激活插件（激活期宿主据 manifest 自动登记端点，端点表与
        // `PluginInvoker` 端口此刻都必须已就位）。
        AppContextBuilder::new()
            .db(db.clone())
            .plugin_host(plugin_host.clone())
            .mdns_advertiser(mdns_advertiser.clone())
            .app_handle(None)
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();
        install_server_ports();

        if activate_center {
            // 激活认证中心：/api/auth/*、/api/sessions*、/api/configs 与两条 WS
            // 插件端点全在插件侧；激活期宿主据 manifest 自动登记端点
            plugin_host
                .activate_plugin(SESSION_PLUGIN_ID, false)
                .await
                .expect("activate com.bedcode.terminal-session (bundled artifact)");
        }
        let _ = INIT.set(());
    }
}

/// 探测空闲端口：绑 127.0.0.1:0 由 OS 分配，立即释放后交给服务器绑定
pub fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 启动真实 HTTP/WS 服务器（Actix，`start_http_server` 纯函数、不依赖 AppHandle）
///
/// 返回 `(端口, ServerHandle, server 任务句柄)`。
pub async fn start_server() -> (
    u16,
    actix_web::dev::ServerHandle,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let config = AppConfig::default().network;
    let port = pick_free_port();
    let (handle, server) = start_http_server(port, &config).await.expect("test server must start");
    let task = tokio::spawn(server);
    (port, handle, task)
}

/// 优雅停机并等待服务器任务退出（超时 panic：静默超时会让端口被占用影响后续 target）
pub async fn stop_server(handle: actix_web::dev::ServerHandle, task: tokio::task::JoinHandle<std::io::Result<()>>) {
    tokio::time::timeout(std::time::Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    task.await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");
}

/// 宿主 → 插件互调（JSON-RPC 约定，与 `pty_session_chain::plugin_api_call` 同 wire）
///
/// **params = 该 api 声明的入参值本身**（不是带参数名的对象）：SDK 宏按参数个数
/// 生成反序列化——单参 `x: T` 直接 `from_value(params)`。所以
/// `config-upsert(draft: Value)` 的 params 是 draft 对象，
/// `qr-code-generate(ttl: u64)` 的 params 是裸 `300`；传 `{"ttl":300}` 会被
/// 插件侧报 `invalid params`（且回复走 error 通道，宿主侧表现为超时）。
///
/// 无头集成测试没有前端命令通道，播种插件侧数据（如会话配置 / QR token）的唯一路径。
pub async fn plugin_api_call(api: &str, params: serde_json::Value) -> serde_json::Value {
    let host_ctx = AppContext::global().plugin_host().wasm_host_ctx().clone();
    let topic = format!("bedcode.api.{api}");
    let method = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(api);
    let id = format!(
        "crossend-req-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let reply_json = host_ctx
        .call_plugin_api_host(&topic, &payload.to_string(), 5000)
        .unwrap_or_else(|e| panic!("plugin api '{api}' failed: {e}"));
    let reply: serde_json::Value = serde_json::from_str(&reply_json).expect("reply json");
    if let Some(err) = reply.get("error") {
        panic!("plugin api '{api}' error: {err}");
    }
    reply.get("result").cloned().expect("result")
}

/// 驱动插件 **Rust 命令面**（= 桌面宿主前端那条路，与 `plugin_invoke` 同一下游）
///
/// 走 `PluginHost::invoke_rust_command`（`api_bridge::plugin_invoke` 的下游，
/// 去掉的是 webview + 凭证层——那是身份管道，不是输出管道）。因此本函数能
/// 在无头场景里扮演**桌面终端预览消费者**（`session.output.pull` /
/// `session.output.ack`），与移动端 WS 消费者**共享同一个 PTY 输出环**。
///
/// 这是「桌面端 + 移动端同时看同一个终端」唯一能真实构造的位置：两个消费者
/// 的帧面根本不同（命令面 JSON vs WS 二进制帧），任何单端 mock 都拼不出来。
pub async fn plugin_command(plugin_id: &str, command: &str, args: serde_json::Value) -> serde_json::Value {
    try_plugin_command(plugin_id, command, args)
        .await
        .unwrap_or_else(|e| panic!("plugin command '{plugin_id}::{command}' failed: {e}"))
}

/// [`plugin_command`] 的可失败版（长循环里用：错误要能被断言到，不得靠 panic 传递）
pub async fn try_plugin_command(
    plugin_id: &str,
    command: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    AppContext::global()
        .plugin_host()
        .invoke_rust_command(plugin_id, command, args)
        .await
        .map_err(|e| e.to_string())
}

/// 播种一条可直接启动的会话配置（bash，工作目录临时目录），返回 config id
pub async fn seed_shell_config(name: &str) -> String {
    let config = plugin_api_call(
        "com.bedcode.terminal-session.config-upsert",
        serde_json::json!({
            "name": name,
            "environment": "linux",
            "workingDir": std::env::temp_dir().to_string_lossy(),
            "command": "bash",
        }),
    )
    .await;
    config["id"].as_str().expect("config-upsert must return id").to_string()
}

/// 场景收尾：清理临时目录（随包产物目录 `resources/plugins/desktop` 不得触碰）
pub fn cleanup_temp_dirs() {
    let _ = std::fs::remove_dir_all(user_plugins_dir());
    let _ = std::fs::remove_dir_all(plugin_db_root());
}
