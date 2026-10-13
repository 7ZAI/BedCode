//! 测试支持面（票 17 批次 2）
//!
//! 编译门控：`#[cfg(any(test, feature = "test-support"))]`
//! - crate 内部 `#[cfg(test)]`（组件闭环测试）与本 crate 测试夹具共用；
//! - 宿主 src-tauri 在 `[dev-dependencies]` 开启 `test-support` feature
//!   （manager / loader 集成测试消费 `build_host_ctx` / `build_test_component` /
//!   `build_terminal_session_component`）。
//!
//! 生产构建零影响（两个 cfg 都不生效时本模块整体编译为空）。

pub mod mock_plugin_ws;

/// 夹具共享 target 目录（`bedcode-mobile/target/fixtures`）
///
/// 与桌面端同构：terminal-session 插件与 plugin-component-test 夹具原本各写
/// 各自 `target/`，把 SDK / wit-bindgen / serde 这份相同依赖图编译两遍
/// （2026-09-26 实测各 ~170M）。共享后只编译一遍。
/// 路径基准：本 crate 根 = 仓库根 `packages/bedcode-wasm-core`（票 06 批次 03 迁根），
/// 上跳两级到仓库根再进 `bedcode-mobile/target/fixtures`。
pub fn fixture_target_dir() -> std::path::PathBuf {
    // 票 06 批次 03：crate 迁根后路径改写（移动夹具落点跟随移动端 target）
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bedcode-mobile/target/fixtures")
}

/// 测试用插件 ID（宿主主库表前缀校验依赖它）
pub const TEST_PLUGIN_ID: &str = "com.bedcode.test";

/// 构建测试引擎：燃料看门狗必须与生产配置一致（WasmRuntime::new）
pub fn test_engine() -> wasmtime::Engine {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    wasmtime::Engine::new(&config).expect("create test engine")
}

/// 测试组件字节缓存（按 features key），跨用例复用避免重复 cargo build
#[cfg(feature = "test-support")]
static COMPONENT_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
> = std::sync::OnceLock::new();

#[cfg(feature = "test-support")] // 需 wit-component（宿主 dev-deps / crate test --features test-support）
/// 构建真实插件组件：SDK `wasm_entry!` 宏产物（票 16 起样本 = 合并后的
/// `com.bedcode.terminal-session`（终端域 + 任务域），迁移 ticket 04 验收沿用）
///
/// 与 `build_test_component` 同链路：cargo build（wasm32，wasm feature）→
/// wit-component 编码。被测对象是 SDK 宏生成的组件（区别于手写 Guest impl
/// 的 plugin-component-test）——宏展开错误 / export! 接线错误在此暴露。
pub fn build_terminal_session_component() -> Vec<u8> {
    let cache =
        COMPONENT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    const KEY: &str = "terminal-session";
    if let Some(bytes) = cache.lock().unwrap().get(KEY) {
        return bytes.clone();
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 票 06 批次 03：crate 迁根后路径改写（fork 时代 ../../wasm-apps = bedcode-mobile/
    // wasm-apps；根 crate 在 packages/ ⇒ 跨端相对路径 ../../bedcode-mobile/wasm-apps）
    let plugin_dir = manifest_dir.join("../../bedcode-mobile/wasm-apps/terminal-session");
    let target_dir = fixture_target_dir().to_str().unwrap().to_string();
    let manifest_path = plugin_dir
        .join("rust/Cargo.toml")
        .to_str()
        .unwrap()
        .to_string();
    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
            &target_dir,
            "--no-default-features",
            "--features",
            "wasm",
            "--manifest-path",
            &manifest_path,
        ])
        .status()
        .expect("Failed to run cargo build for terminal-session component");
    assert!(status.success(), "terminal-session WASM build failed");

    let core = std::fs::read(
        fixture_target_dir()
            .join("wasm32-unknown-unknown/release/bedcode_plugin_terminal_session.wasm"),
    )
    .expect("Failed to read terminal-session module after build");
    // 宏产物必经 componentize（等效本函数内编码）；SDK 构建链已内置该步骤。
    // 此处直接编码 core module（若传入已组件化产物，编码器会拒绝）
    let component = wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&core)
        .expect("component encoder module")
        .encode()
        .expect("component encoder encode");
    assert_eq!(
        &component[..4],
        [0x00, 0x61, 0x73, 0x6d],
        "terminal-session 组件应以 core module 段起始"
    );
    assert_eq!(
        &component[4..8],
        [0x0d, 0x00, 0x01, 0x00],
        "terminal-session 组件头应为 0d 00 01 00"
    );

    cache
        .lock()
        .unwrap()
        .insert(KEY.to_string(), component.clone());
    component
}

/// 构建并编码测试组件：cargo build（指定 features）→ wit-component 编码
#[cfg(feature = "test-support")] // 需 wit-component
pub fn build_test_component(features: &[&str]) -> Vec<u8> {
    let cache =
        COMPONENT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let key = if features.is_empty() {
        "default".to_string()
    } else {
        features.join("+")
    };
    if let Some(bytes) = cache.lock().unwrap().get(&key) {
        return bytes.clone();
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 票 06 批次 03：crate 迁根后路径改写（plugin-component-test 在移动 packages/ 下）
    let plugin_dir = manifest_dir.join("../../bedcode-mobile/packages/plugin-component-test");
    let target_dir = fixture_target_dir().to_str().unwrap().to_string();
    let features_arg = features.join(",");
    let manifest_path = plugin_dir.join("Cargo.toml").to_str().unwrap().to_string();
    let mut args = vec![
        "build",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "--target-dir",
        &target_dir,
    ];
    if !features.is_empty() {
        args.push("--features");
        args.push(&features_arg);
    }
    args.push("--manifest-path");
    args.push(&manifest_path);

    let status = std::process::Command::new("cargo")
        .args(&args)
        .status()
        .expect("Failed to run cargo build for test component");
    assert!(status.success(), "Test component WASM build failed");

    let core = std::fs::read(
        fixture_target_dir()
            .join("wasm32-unknown-unknown/release/bedcode_plugin_component_test.wasm"),
    )
    .expect("Failed to read test component module after build");
    let component = wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&core)
        .expect("component encoder module")
        .encode()
        .expect("component encoder encode");
    // 产物形态：核心模块段在前、组件头随后（spike 实证 00 61 73 6d 0d 00 01 00）
    assert_eq!(
        &component[..4],
        [0x00, 0x61, 0x73, 0x6d],
        "编码后组件应以 core module 段起始"
    );
    assert_eq!(
        &component[4..8],
        [0x0d, 0x00, 0x01, 0x00],
        "编码后组件头应为 0d 00 01 00"
    );

    cache.lock().unwrap().insert(key, component.clone());
    component
}

/// 构造最小宿主上下文（db 内存库 + 内存 storage；app_handle=None 无头形态；
/// 端口 = UnimplementedPorts 占位——夹具用例不触引擎调用）
pub fn build_host_ctx() -> std::sync::Arc<crate::host_api::WasmHostContext> {
    use std::sync::{Arc, Mutex};
    let db = std::sync::Arc::new(std::sync::Mutex::new(crate::db::Database::from_connection(
        rusqlite::Connection::open_in_memory().expect("open in-memory db"),
    )));
    let storage = crate::storage::PluginStorage::test_storage();
    // 无头/测试上下文：fs_auth 的 app_handle 亦为 None（桌面端 build_host_ctx 同形态）
    // first_party_dirs 空 = 移动形态（桌面 first-party 概念不启用）
    let fs_auth = Arc::new(crate::security::fs_auth::FsAuthChecker::new(
        storage.clone(),
        None,
        Vec::new(),
    ));
    let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
    Arc::new(crate::host_api::WasmHostContext::new_headless(
        db,
        storage,
        None,
        fs_auth,
        Arc::new(crate::bus::MessageBus::new()),
        status_reporter,
    ))
}

/// 统一运行时壳的无头构造（单形态 2026-10-13）
///
/// 无 storage / app_handle / first_party_dirs，ABI 规格 [`AbiPolicy::UNCONSTRAINED`]
/// （无头装配不设上限——夹具是桌面 SDK 产物）。供移动宿主集成测试构造
/// `WasmRuntime`（loader / manager / 终端会话夹具均需真实 Engine）。
pub fn setup_headless_runtime(
    aot_cache_dir: Option<std::path::PathBuf>,
) -> crate::Result<crate::manager::runtime::WasmRuntime> {
    setup_headless_runtime_with_abi(aot_cache_dir, crate::manager::runtime::AbiPolicy::UNCONSTRAINED)
}

/// 同 [`setup_headless_runtime`]，但显式注入 ABI 规格（端事实）
///
/// 单形态（2026-10-13）：ABI 版本上限检查是宿主注入的策略（wasm-core 零端 ABI
/// 知识）——端宿主测试要断言「高版本组件被拒绝」（如移动 `high-abi` 用例），
/// 必须注入端真实规格（移动 21 / None），不能全用 UNCONSTRAINED。
pub fn setup_headless_runtime_with_abi(
    aot_cache_dir: Option<std::path::PathBuf>,
    abi: crate::manager::runtime::AbiPolicy,
) -> crate::Result<crate::manager::runtime::WasmRuntime> {
    let storage = crate::storage::PluginStorage::test_storage();
    let mut runtime = crate::manager::runtime::WasmRuntime::with_setup(
        storage,
        None,
        crate::manager::runtime::EngineSetup::new(crate::config::CoreConfig::default())
            .with_abi(abi),
    )?;
    // AOT 缓存目录：无头装配注入（生产路径经 app_handle 取；测试用例断言
    // 缓存产物落点需要显式注入——aot_cache_dir 是 pub(crate)，crate 内可达）
    runtime.aot_cache_dir = aot_cache_dir;
    Ok(runtime)
}

/// 构造插件实例 state（统一 host-kit 形态，wasm-core-mobile-purity 票 04）
///
/// 取代旧自持 `WasmPluginState { plugin_id, host_ctx, runtime_handle,
/// granted_permissions, on_message_binary }` 字面量：权限集经 `for_plugin`
/// 注入 per-plugin 上下文；`host` 槽是 host-kit state 的 `Arc<dyn HostPorts>`。
/// StoreSpec 取默认资源上限 + 关燃料（测试不设限、不注入燃料预算）。
pub fn build_component_state(plugin_id: &str, granted: &[&str]) -> crate::manager::runtime::WasmPluginState {
    let ctx = build_host_ctx().for_plugin(granted.iter().map(|p| p.to_string()).collect());
    build_component_state_with_ctx(plugin_id, std::sync::Arc::new(ctx))
}

/// 同上，允许注入自定义宿主上下文（域测试需覆盖端口 / 构造无头上下文时用）
pub fn build_component_state_with_ctx(
    plugin_id: &str,
    ctx: std::sync::Arc<crate::host_api::WasmHostContext>,
) -> crate::manager::runtime::WasmPluginState {
    crate::manager::runtime::WasmPluginState::new(
        plugin_id.to_string(),
        ctx,
        // 移动测试夹具不 import WASI（wasm32-unknown-unknown）⇒ 空上下文即可
        crate::manager::runtime::wasi::empty_ctx(),
        crate::manager::runtime::StoreSpec {
            limits: bedcode_host_kit::limits::StoreLimits::default(),
            fuel_enabled: false,
            metrics: std::sync::Arc::new(bedcode_host_kit::metrics::PluginMetrics::default()),
        },
    )
}

// ==================== 组件实例观测面（宿主集成测试转发，票 05） ====================
//
// 宿主集成测试链的是**不带 cfg(test)** 的 lib（`#[cfg(test)]` 项等价于不可达），
// 而 fixture e2e 随绑定归位迁到移动宿主 `src-tauri/tests/`——它们需要的壳内私有面
// 经这两个转发函数消费（生产跨 crate 仍不可达：`raw_store` 是 `pub(crate)`、
// `core_binding` 是 `pub(crate) mod`）。

/// 系统存储空间属主 ID（`__system__`）
///
/// 内核 `storage::SYSTEM_PLUGIN_ID` 是 `pub(crate)`（属主分区细节不对外）；
/// 宿主域测试断言「系统空间纵深守卫对移动端同样生效」需要这个常量，经本转发面给出。
pub const SYSTEM_PLUGIN_ID: &str = crate::storage::SYSTEM_PLUGIN_ID;

/// 实例 ABI 版本探测（原生 `TypedFunc` 探测 + 调用，与生产协商同一实现）
///
/// 宿主 e2e 用它断言「产物 `abi.version()` == SDK `ABI_VERSION`」——不另拼探测逻辑，
/// 避免测试与生产各钉一份（漂移时测试反而会掩盖问题）。
pub fn plugin_abi_version(
    plugin: &mut crate::manager::runtime::LoadedWasmPlugin,
) -> crate::Result<Option<u32>> {
    let (store, instance) = plugin.raw_store();
    crate::manager::runtime::core_binding::abi_version(store, instance)
}

/// linker 装配面（与生产同一函数：core 交集 + 宿主自报能力模块 + WASI 段）
///
/// 单形态（2026-10-13）起与桌面壳同一实现：`component::add_to_linker`——core
/// 交集 8 域（core_binding）+ `inventory` 自报能力模块 + WASI p2/p3。宿主集成
/// 测试据此断言 linker 装配契约（WASI 默认注册 / 配置关闭可重注册）。
pub fn build_component_linker(
    engine: &wasmtime::Engine,
    wasi_cfg: &crate::config::WasiConfig,
) -> crate::Result<wasmtime::component::Linker<crate::manager::runtime::WasmPluginState>> {
    let mut linker = wasmtime::component::Linker::new(engine);
    crate::manager::runtime::component::add_to_linker(&mut linker, wasi_cfg)?;
    Ok(linker)
}

/// 探针：对给定 linker 尝试再次注册 WASI p2
///
/// 返回 `Some(错误文本)` = 已注册过（重复注册被 wasmtime 以 "defined twice" 拒绝）；
/// `None` = 本次注册成功（即此前**未**注册）。让宿主测试不引入 `wasmtime-wasi` 依赖。
pub fn probe_wasi_p2_reregistration(
    linker: &mut wasmtime::component::Linker<crate::manager::runtime::WasmPluginState>,
) -> Option<String> {
    wasmtime_wasi::p2::add_to_linker_async(linker)
        .err()
        .map(|e| e.to_string())
}

/// 实例 Store / Instance 裸面（燃料、limiter、原生导出调用等机制断言用）
///
/// 形状与壳内 `raw_store` 逐字一致，只是把可达性从 `pub(crate)` 提到本转发面。
pub fn plugin_raw_store(
    plugin: &mut crate::manager::runtime::LoadedWasmPlugin,
) -> (
    &mut wasmtime::Store<crate::manager::runtime::WasmPluginState>,
    &wasmtime::component::Instance,
) {
    plugin.raw_store()
}

// ==================== MockPorts：端口测试替身（builder 式 override） ====================

use crate::host_api::ports::{
    AuthEnginePort, ConnectionEnginePort, HostEnginePorts, PrimaryTarget, SafIoPort,
    UnimplementedPorts, WsReconnectPolicyPort,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// 端口测试替身：未 override 的方法复刻 UnimplementedPorts 的 fail-visible
/// 形态（Err/None/空值），override 过的方法返回测试注入值——域测试按需最小装配。
pub struct MockPorts {
    token: String,
    connection: Option<Arc<dyn ConnectionEnginePort>>,
    auth: Option<Arc<dyn AuthEnginePort>>,
    egress_allow: bool,
    reconnect: Option<Arc<dyn Fn(u32, u64, u64) -> Box<dyn WsReconnectPolicyPort> + Send + Sync>>,
    base: UnimplementedPorts,
}

impl Default for MockPorts {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPorts {
    pub fn new() -> Self {
        Self {
            token: String::new(),
            connection: None,
            auth: None,
            egress_allow: false,
            reconnect: None,
            base: UnimplementedPorts,
        }
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.to_string();
        self
    }

    pub fn with_connection(mut self, engine: Arc<dyn ConnectionEnginePort>) -> Self {
        self.connection = Some(engine);
        self
    }

    pub fn with_auth(mut self, engine: Arc<dyn AuthEnginePort>) -> Self {
        self.auth = Some(engine);
        self
    }

    /// egress 一律放行（跳过宿主安全闸门——真源测试在宿主侧）
    pub fn with_egress_allow(mut self) -> Self {
        self.egress_allow = true;
        self
    }

    pub fn with_reconnect_policy(
        mut self,
        f: impl Fn(u32, u64, u64) -> Box<dyn WsReconnectPolicyPort> + Send + Sync + 'static,
    ) -> Self {
        self.reconnect = Some(Arc::new(f));
        self
    }
}

#[async_trait::async_trait]
impl HostEnginePorts for MockPorts {
    async fn egress_check(
        &self,
        app: &tauri::AppHandle,
        url: &str,
        source: &str,
    ) -> Result<(), String> {
        if self.egress_allow {
            Ok(())
        } else {
            self.base.egress_check(app, url, source).await
        }
    }

    fn egress_redirect_policy(&self) -> reqwest::redirect::Policy {
        reqwest::redirect::Policy::none()
    }

    fn auth_engine(&self) -> Option<Arc<dyn AuthEnginePort>> {
        self.auth.clone()
    }

    fn global_token(&self) -> String {
        self.token.clone()
    }

    fn connection_engine(&self) -> Arc<dyn ConnectionEnginePort> {
        self.connection
            .clone()
            .unwrap_or_else(|| Arc::new(NoTargetConn))
    }

    fn reconnect_policy(
        &self,
        max_retries: u32,
        base_ms: u64,
        max_ms: u64,
    ) -> Box<dyn WsReconnectPolicyPort> {
        match &self.reconnect {
            Some(f) => f(max_retries, base_ms, max_ms),
            None => Box::new(StaticBackoff::new(base_ms)),
        }
    }

    fn reconnect_bounds(&self) -> (u64, u64) {
        (1000, 60_000)
    }

    fn current_node_id(&self, _app: &tauri::AppHandle) -> Option<String> {
        None
    }

    async fn peer_dial_endpoint(
        &self,
        app: &tauri::AppHandle,
        n: String,
        a: String,
        p: u16,
    ) -> crate::Result<String> {
        self.base.peer_dial_endpoint(app, n, a, p).await
    }
    async fn peer_disconnect(&self, app: &tauri::AppHandle, n: String) -> crate::Result<bool> {
        self.base.peer_disconnect(app, n).await
    }
    async fn peer_respond_consent(
        &self,
        app: &tauri::AppHandle,
        r: String,
        ok: bool,
    ) -> crate::Result<bool> {
        self.base.peer_respond_consent(app, r, ok).await
    }
    async fn peer_list_trusted(&self, app: &tauri::AppHandle) -> crate::Result<String> {
        self.base.peer_list_trusted(app).await
    }
    async fn peer_revoke_trusted(&self, app: &tauri::AppHandle, n: String) -> crate::Result<bool> {
        self.base.peer_revoke_trusted(app, n).await
    }
    async fn peer_set_shared_roots(&self, app: &tauri::AppHandle, j: String) -> crate::Result<()> {
        self.base.peer_set_shared_roots(app, j).await
    }
    async fn peer_start_node(&self, app: &tauri::AppHandle, c: &str) -> crate::Result<bool> {
        self.base.peer_start_node(app, c).await
    }
    async fn peer_stop_node(&self, app: &tauri::AppHandle, c: &str) -> crate::Result<bool> {
        self.base.peer_stop_node(app, c).await
    }
    async fn peer_cancel_transfer(&self, app: &tauri::AppHandle, b: String) -> crate::Result<bool> {
        self.base.peer_cancel_transfer(app, b).await
    }
    async fn peer_cancel_receiving(
        &self,
        app: &tauri::AppHandle,
        b: String,
    ) -> crate::Result<bool> {
        self.base.peer_cancel_receiving(app, b).await
    }
    async fn peer_send_files_with_policy(
        &self,
        app: &tauri::AppHandle,
        n: String,
        paths: Vec<String>,
        e: Option<bool>,
    ) -> crate::Result<String> {
        self.base
            .peer_send_files_with_policy(app, n, paths, e)
            .await
    }
    async fn peer_respond_transfer(
        &self,
        app: &tauri::AppHandle,
        b: String,
        ok: bool,
    ) -> crate::Result<()> {
        self.base.peer_respond_transfer(app, b, ok).await
    }
    async fn peer_set_receive_policy(
        &self,
        app: &tauri::AppHandle,
        m: String,
        t: u64,
    ) -> crate::Result<()> {
        self.base.peer_set_receive_policy(app, m, t).await
    }
    async fn peer_pause_transfer(&self, app: &tauri::AppHandle, b: String) -> crate::Result<bool> {
        self.base.peer_pause_transfer(app, b).await
    }
    async fn peer_resume_transfer(&self, app: &tauri::AppHandle, b: String) -> crate::Result<bool> {
        self.base.peer_resume_transfer(app, b).await
    }
    async fn peer_set_download_dir(
        &self,
        app: &tauri::AppHandle,
        p: Option<String>,
    ) -> crate::Result<()> {
        self.base.peer_set_download_dir(app, p).await
    }
    async fn peer_list_shared_roots(
        &self,
        app: &tauri::AppHandle,
        n: String,
    ) -> crate::Result<String> {
        self.base.peer_list_shared_roots(app, n).await
    }
    async fn peer_browse_directory(
        &self,
        app: &tauri::AppHandle,
        n: String,
        d: String,
        r: String,
    ) -> crate::Result<String> {
        self.base.peer_browse_directory(app, n, d, r).await
    }
    async fn peer_pull_files(
        &self,
        app: &tauri::AppHandle,
        n: String,
        d: String,
        f: String,
    ) -> crate::Result<u32> {
        self.base.peer_pull_files(app, n, d, f).await
    }
    async fn peer_active_transfers(&self, app: &tauri::AppHandle) -> crate::Result<String> {
        self.base.peer_active_transfers(app).await
    }
    async fn peer_collect_outgoing(&self, paths: Vec<String>) -> crate::Result<String> {
        self.base.peer_collect_outgoing(paths).await
    }
    async fn platform_pick_files(&self, app: &tauri::AppHandle) -> crate::Result<Vec<String>> {
        self.base.platform_pick_files(app).await
    }
    async fn platform_pick_shared_directory(
        &self,
    ) -> crate::Result<Option<(String, String, String)>> {
        self.base.platform_pick_shared_directory().await
    }
    async fn resolve_downloads_dir(&self, app: &tauri::AppHandle) -> crate::Result<String> {
        self.base.resolve_downloads_dir(app).await
    }
    async fn delete_file_android(&self, path: String) -> crate::Result<()> {
        self.base.delete_file_android(path).await
    }
    async fn is_within_app_downloads_dir(
        &self,
        app: &tauri::AppHandle,
        path: &str,
    ) -> crate::Result<bool> {
        self.base.is_within_app_downloads_dir(app, path).await
    }
    fn saf_io(&self, app: &tauri::AppHandle) -> Option<Arc<dyn SafIoPort>> {
        self.base.saf_io(app)
    }
    async fn app_data_dir(&self, app: &tauri::AppHandle) -> crate::Result<PathBuf> {
        self.base.app_data_dir(app).await
    }
    async fn notify_show(
        &self,
        plugin_id: &str,
        title: &str,
        body: &str,
        vibrate: bool,
        sound: bool,
    ) -> Result<(), String> {
        self.base
            .notify_show(plugin_id, title, body, vibrate, sound)
            .await
    }

    async fn notify_check_permission(&self) -> Result<bool, String> {
        self.base.notify_check_permission().await
    }

    async fn notify_request_permission(&self) -> Result<bool, String> {
        self.base.notify_request_permission().await
    }

    async fn notify_vibrate(&self, duration_ms: u32) -> Result<(), String> {
        self.base.notify_vibrate(duration_ms).await
    }

    async fn notify_play_sound(&self) -> Result<(), String> {
        self.base.notify_play_sound().await
    }
}

/// MockPorts 兜底连接引擎：无目标（Ok(None)）
struct NoTargetConn;

#[async_trait::async_trait]
impl ConnectionEnginePort for NoTargetConn {
    async fn primary_target(&self) -> crate::Result<Option<(PrimaryTarget, bool)>> {
        Ok(None)
    }
}

/// 固定退避策略（重连测试用：单轮固定延迟）
pub struct StaticBackoff {
    delay_ms: u64,
}

impl StaticBackoff {
    pub fn new(delay_ms: u64) -> Self {
        Self { delay_ms }
    }
}

#[async_trait::async_trait]
impl WsReconnectPolicyPort for StaticBackoff {
    async fn start(&self) -> Option<()> {
        Some(())
    }
    async fn get_delay(&self) -> Duration {
        Duration::from_millis(self.delay_ms)
    }
    async fn on_success(&self) {}
}

/// 带自定义端口的宿主上下文构造（测试夹具）
pub fn build_host_ctx_with(
    ports: impl crate::host_api::HostEnginePorts + 'static,
) -> Arc<crate::host_api::WasmHostContext> {
    build_host_ctx_with_arc(Arc::new(ports))
}

/// [`build_host_ctx_with`] 的 Arc 形态（跨域测试共享同一端口替身实例）
pub fn build_host_ctx_with_arc(
    ports: Arc<dyn crate::host_api::HostEnginePorts>,
) -> Arc<crate::host_api::WasmHostContext> {
    use std::sync::Mutex;
    let db = Arc::new(Mutex::new(crate::db::Database::from_connection(
        rusqlite::Connection::open_in_memory().expect("open in-memory db"),
    )));
    let storage = crate::storage::PluginStorage::test_storage();
    let fs_auth = Arc::new(crate::security::fs_auth::FsAuthChecker::new(
        storage.clone(),
        None,
        Vec::new(),
    ));
    let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
    Arc::new(crate::host_api::WasmHostContext::new_with_ports(
        db,
        storage,
        None,
        fs_auth,
        Arc::new(crate::bus::MessageBus::new()),
        status_reporter,
        ports,
    ))
}
