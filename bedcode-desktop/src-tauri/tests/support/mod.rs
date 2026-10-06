//! 宿主侧集成测试共享基建（wasm-core 纯净性收口票 05b，2026-10-06）
//!
//! **为什么存在**：`bedcode-wasm-core` 内的跨 crate 集成测试（加载真实产品产物
//! `resources/plugins/desktop/*.wasm`）用户裁定全部迁宿主侧 `src-tauri/tests/`。
//! wasm-core 侧同名脚手架（`manager/host/tests/scaffold.rs` / `wasm_flow_test.rs` /
//! `test_support.rs`）随迁移分化：
//! - 机制本体脚手架（无头运行时装配 / 认证注册表闸门 / 夹具读取）留在
//!   `bedcode_wasm_core::test_support`（常编译 pub，本模块经其消费）；
//! - 宿主装配脚手架（`setup_host`）按**方案 B**（用户 2026-10-06 裁定）改为
//!   `PluginHost::new` **真实装配路径**——wasm-core 内 scaffold 的结构体字面量
//!   构造依赖私有字段访问，lib 侧无法等价；`PluginHost::new` 走 inventory +
//!   真实插件目录扫描，与生产同构。
//!
//! 本模块是 05c 批量迁移（ws_e2e / auth_center_perf / ws_output_perf 等）的共享
//! 底座：`setup_host` / `make_plugin` / `setup_wasm_plugin` 对所有迁出的真实产物
//! 测试二进制可见。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

// 夹具读取 re-export：所有经 `mod support; use support::*;` 的测试文件直接用
pub use bedcode_desktop_lib::wasm_core::test_support::{
    sdk_fixture_artifact_bytes, system_test_artifact_bytes,
};
// 常用类型 re-export（05c 批量迁移文件的公共面）
pub use bedcode_desktop_lib::db::Database;
pub use bedcode_desktop_lib::wasm_core::config::CallModel;
pub use bedcode_desktop_lib::wasm_core::manager::host::PluginHost;
pub use bedcode_desktop_lib::wasm_core::manager::host::{GuestOp, GuestReply};
pub use bedcode_desktop_lib::wasm_core::manager::runtime::LoadedWasmPlugin;
pub use bedcode_desktop_lib::wasm_core::manager::types::{LoadedPlugin, PluginSource};
pub use bedcode_desktop_lib::wasm_core::storage::PluginStorage;
pub use bedcode_desktop_lib::wasm_core::system::config::AppConfig;
pub use bedcode_plugin_api::{PluginKind, PluginManifest, PluginState, PluginType};

/// 随包内置插件目录（与 pty_session_chain 的 `bundled_plugins_dir` 同规则）
pub fn bundled_plugins_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/plugins/desktop")
}

/// 系统组件 fixture 插件 ID（与 packages/plugin-system-test 的 manifest 一致）
pub const TEST_SYSTEM_PLUGIN_ID: &str = "com.bedcode.system-test";

/// WASM 测试组件插件 ID（SDK fixture 合集 `plugin-sdk-fixtures`，与 wasm-core
/// host.rs mod tests 的 `TEST_WASM_PLUGIN_ID` 同值）
pub const TEST_WASM_PLUGIN_ID: &str = "com.bedcode.sdk-test";

// ==================== 宿主装配（方案 B：PluginHost::new 真实路径） ====================

/// 构造无头测试宿主（app_handle = None；**真实装配路径**）
///
/// 与 wasm-core 内 scaffold 的 `setup_host`（结构体字面量 + 私有字段）不同：
/// 本实现走 `PluginHost::new`——inventory 静态注册 + 真实插件目录扫描 +
/// 生产装配链（monitor / owner_sink / 能力域端口 / task engine / 单元执行器）。
/// 随包插件目录指向真实产物 `resources/plugins/desktop`（不建临时副本）。
///
/// 无头差异：app_handle / peer_ctx_provider 传 None（依赖前端事件的宿主能力
/// 降级）；插件私有库根注入进程级临时目录（认证中心私有库真源可达——
/// `PluginHost::new` 不设，生产由 app_handle 派生）。
pub async fn setup_host() -> Arc<PluginHost> {
    // AppConfig 初始化（与 wasm-core 测试同策略；重复 init 幂等）
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = bedcode_desktop_lib::wasm_core::system::config::AppConfig::default();
        config.network.port = 8765;
        bedcode_desktop_lib::wasm_core::system::config::AppConfig::init(config);
    });

    let db = Arc::new(Mutex::new(
        Database::new(&PathBuf::from(":memory:")).expect("memory db"),
    ));
    db.lock().await.init_schema().expect("init schema");

    let plugins_dir = bundled_plugins_dir();
    let user_plugins_dir =
        std::env::temp_dir().join(format!("bedcode-hosttest-userplugins-{}", std::process::id()));
    std::fs::create_dir_all(&user_plugins_dir).expect("user plugins dir");

    let host = PluginHost::new(db, &plugins_dir, &user_plugins_dir, None, None, Vec::new()).await;

    // 无头私有库根注入（认证中心配对/历史真源在插件私有库，无私有库则无法驱动）
    host.wasm_host_ctx().set_plugin_db_root(Some(
        std::env::temp_dir().join(format!("bedcode-hosttest-pluginroot-{}", std::process::id())),
    ));

    host
}

/// 构造一个最小 LoadedPlugin（manifest 含 storage + terminal:input 权限）
pub fn make_plugin(id: &str, source: PluginSource, state: PluginState) -> LoadedPlugin {
    LoadedPlugin {
        manifest: PluginManifest {
            id: id.to_string(),
            name: format!("Test {}", id),
            version: "1.0.0".to_string(),
            main: "index.ts".to_string(),
            permissions: vec!["storage".to_string(), "terminal:input".to_string()],
            plugin_type: PluginType::TsOnly,
            // 余下字段取 Default，避免 SDK 追加可选字段时本夹具编译红
            ..Default::default()
        },
        state,
        extension_path: String::new(),
        activated_at: None,
        source,
    }
}

// ==================== WASM 测试组件装配 ====================

/// 将 SDK fixture 组件实例化并注入宿主（plugins + 装配条目双表）
///
/// 返回插件 ID；组件 invoke 内 host_storage 读回的 key 预写入
/// `component-test-key`。extension_path 指向临时目录（invoke 的
/// resource_dir 注入断言用）。
pub async fn setup_wasm_plugin(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
    setup_wasm_plugin_with_model(host, tmp_dir, CallModel::Mutex).await
}

/// 同上，但显式指定调用模型（两模型对照用例：不必改全局配置，可并行跑）
pub async fn setup_wasm_plugin_with_model(
    host: &PluginHost,
    tmp_dir: &tempfile::TempDir,
    call_model: CallModel,
) -> String {
    let component = host
        .wasm_runtime()
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let plugin = host
        .wasm_runtime()
        .instantiate_component(&component, TEST_WASM_PLUGIN_ID, host.wasm_host_ctx().clone(), &[], None)
        .expect("instantiate test component");

    host.storage()
        .set(TEST_WASM_PLUGIN_ID, "component-test-key", serde_json::json!({"k": "v"}))
        .await
        .expect("preset storage key");

    let extension_path = tmp_dir.path().to_string_lossy().to_string();
    host.install_instance(TEST_WASM_PLUGIN_ID, plugin, call_model).await;

    let mut loaded = make_plugin(TEST_WASM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_component_test".to_string();
    loaded.extension_path = extension_path;
    host.plugins()
        .write()
        .await
        .insert(TEST_WASM_PLUGIN_ID.to_string(), loaded);

    TEST_WASM_PLUGIN_ID.to_string()
}

/// 实例化系统组件 fixture 并注入宿主（kind=System，探测断言含 host-storage）
pub async fn setup_system_component(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
    let component = host
        .wasm_runtime()
        .compile_component(&system_test_artifact_bytes())
        .expect("compile system test component");
    let plugin = host
        .wasm_runtime()
        .instantiate_component(
            &component,
            TEST_SYSTEM_PLUGIN_ID,
            host.wasm_host_ctx().clone(),
            &[],
            None,
        )
        .expect("instantiate system test component");
    // 实例化探测：plugin-system world 的 host-storage 导出应被识别为可路由能力
    assert_eq!(
        plugin.exported_capabilities(),
        &["host-storage".to_string()],
        "system component must export host-storage capability"
    );

    host.install_instance(TEST_SYSTEM_PLUGIN_ID, plugin, CallModel::Mutex)
        .await;

    let mut loaded = make_plugin(TEST_SYSTEM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_system_test".to_string();
    loaded.manifest.kind = PluginKind::BasicService;
    loaded.extension_path = tmp_dir.path().to_string_lossy().to_string();
    host.plugins()
        .write()
        .await
        .insert(TEST_SYSTEM_PLUGIN_ID.to_string(), loaded);

    TEST_SYSTEM_PLUGIN_ID.to_string()
}

// ==================== host-websocket 域测试基建（票 05c 自 runtime.rs mod tests 迁入） ====================

/// WS 客户端类型（tokio-tungstenite 直连 ws://，与集成测试同构）
pub type WsTestClient = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// 探测空闲端口（OS 分配后立即释放，交给宿主服务器绑定）
pub fn ws_pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port");
    listener.local_addr().expect("probed port").port()
}

/// 读客户端下一条业务帧（跳过心跳帧），超时返回 `None`
pub async fn ws_client_recv(
    client: &mut WsTestClient,
    timeout: std::time::Duration,
) -> Option<tokio_tungstenite::tungstenite::Message> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, client.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                other => return Some(other),
            },
            _ => return None,
        }
    }
}

/// 轮询 `ws-list-clients` 直到在线客户端数达到期望（注册表登记为异步）
pub async fn ws_wait_clients(
    plugin: &Arc<Mutex<LoadedWasmPlugin>>,
    endpoint_id: &str,
    expected: usize,
) -> Vec<serde_json::Value> {
    let args = serde_json::json!({ "endpointId": endpoint_id }).to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let raw = {
            let mut guard = plugin.lock().await;
            guard.invoke_command("ws-list-clients", &args).expect("ws-list-clients")
        };
        let clients: Vec<serde_json::Value> = serde_json::from_str::<serde_json::Value>(&raw)
            .expect("ws-list-clients json")["clients"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if clients.len() == expected || std::time::Instant::now() >= deadline {
            return clients;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// 读 fixture 的 `ws-state` 快照（锁在返回前释放，避免阻塞帧投递）
pub async fn ws_fixture_state(plugin: &Arc<Mutex<LoadedWasmPlugin>>) -> serde_json::Value {
    let raw = {
        let mut guard = plugin.lock().await;
        guard.invoke_command("ws-state", "{}").expect("ws-state")
    };
    serde_json::from_str::<serde_json::Value>(&raw).expect("ws-state json")
}

/// host-websocket fixture e2e 串行锁
///
/// 三个用例共用 fixture 常量属主 id（`com.bedcode.ws-test`，宿主侧连接表 / 端点表 /
/// 事件 topic 均按属主**进程级全局**登记），彼此 `purge_for_plugin` 会清掉对方的
/// 连接与端点（并行时现象：握手 404、连接被回收）。libtest 并行执行下必须串行。
static WS_FIXTURE_E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 取得 fixture e2e 串行锁（跨用例共享全局表；中毒后取回内部值继续）
pub fn lock_ws_fixture_e2e() -> std::sync::MutexGuard<'static, ()> {
    WS_FIXTURE_E2E_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// fixture e2e 兜底超时：把「挂起（疑似死锁）」变成明确失败
///
/// 历史踩点：插件投递任务占住 actix arbiter → 用例无限挂起（无超时则整轮
/// `cargo test` 永不返回）。上限取 60s（正常用例秒级完成）。
pub const WS_E2E_TIMEOUT_SECS: u64 = 60;

/// 以兜底超时驱动 e2e 主体
pub async fn ws_e2e_guard<F>(label: &str, fut: F) -> F::Output
where
    F: std::future::Future,
{
    match tokio::time::timeout(std::time::Duration::from_secs(WS_E2E_TIMEOUT_SECS), fut).await {
        Ok(out) => out,
        Err(_) => {
            panic!("{label}: 超过 {WS_E2E_TIMEOUT_SECS}s 未完成（疑似死锁；检查投递任务是否占住 actix arbiter）")
        }
    }
}

/// 轮询快照直到谓词命中或超时（帧与事件均为异步投递，不能单次读取断言）
pub async fn ws_poll_state(
    plugin: &Arc<Mutex<LoadedWasmPlugin>>,
    pred: impl Fn(&serde_json::Value) -> bool,
    timeout: std::time::Duration,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let state = ws_fixture_state(plugin).await;
        if pred(&state) || std::time::Instant::now() >= deadline {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// 快照中是否含指定 kind 的帧（`text` 为 Some 时要求文本一致）
pub fn ws_has_frame(state: &serde_json::Value, kind: &str, text: Option<&str>) -> bool {
    state["frames"]
        .as_array()
        .map(|frames| {
            frames
                .iter()
                .any(|f| f["kind"] == kind && text.map(|t| f["text"] == t).unwrap_or(true))
        })
        .unwrap_or(false)
}

/// 快照中首个指定 kind 帧的载荷长度
pub fn ws_frame_len(state: &serde_json::Value, kind: &str) -> Option<u64> {
    state["frames"]
        .as_array()?
        .iter()
        .find(|f| f["kind"] == kind)?
        .get("len")?
        .as_u64()
}

/// 快照中指定 topic 的事件 payload
pub fn ws_event_payload(state: &serde_json::Value, topic: &str) -> Option<serde_json::Value> {
    state["events"]
        .as_array()?
        .iter()
        .find(|e| e["topic"] == topic)
        .map(|e| e["payload"].clone())
}

/// 正向认证用例的 skip 判定（v33 起认证判定在认证中心插件，无头 harness 装不出
/// 隔离的 AppContext；正向认证覆盖见 `ws_auth_rules.rs` / `http_auth_biometric.rs`）
pub fn positive_auth_needs_dedicated_binary(test: &str) -> bool {
    eprintln!(
        "[skip] {test}: 需要正向认证（中心签发 → 首消息认证 → 业务帧往返），\
         但 v33 之后认证判定在认证中心插件里，宿主侧调用需要 AppContext——\
         无头 harness 装不出隔离的 AppContext。正向认证覆盖见 \
         tests/ws_auth_rules.rs 与 tests/http_auth_biometric.rs。"
    );
    true
}
