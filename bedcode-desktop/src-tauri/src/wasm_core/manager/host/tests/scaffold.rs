//! host 测试共享脚手架（自 `manager/host.rs` 内联 `mod tests` 拆出，票 11 第 7 项）。
//!
//! 跨文件复用，故顶层项标 `pub(super)`（可见于 `host::tests` 及其后代）。

use super::*;

/// 本模块测试的不可测面说明：
///
/// - `PluginHost::new`：依赖 inventory 静态注册表与真实插件目录；app_handle 已
///   Option 化（None = 无头测试上下文），但集成测试在 crate 外无法访问私有字段,
///   仍需通过结构体字面量直接构造（tests 位于 host.rs 内部，可访问私有字段），
///   覆盖 new() 之后的全部宿主行为。
/// - `notify_startup` / `notify_shutdown` / `PluginServices::mark_plugin_error`：
///   无头测试上下文降级为纯日志跳过前端 emit（try_global None 分支），
///   静态插件回调链路由下方合成 inventory 条目测试覆盖。
/// - `dispatch_*_to_plugin` 的错误分支（WASM 实例缺失/调用失败）：仅有日志
///   副作用，无返回值可断言；成功路径由 `test_dispatch_lifecycle_and_input_to_wasm_plugin`
///   以「分发后 store 未被污染」间接验证。
/// - `invoke_wasm_command` 的非法 JSON 返回分支：需要构造返回坏 JSON 的恶意
///   WASM 插件，超出测试组件能力范围。

/// 构造无头测试宿主（app_handle = None）
///
/// 结构体字面量构造 PluginHost：字段私有但 tests 模块与 host.rs 同属一个
/// 模块树，可访问。所有子系统均用真实实现 + 内存 SQLite，仅 Tauri 相关
/// 能力降级（与 wasm_runtime.rs 测试同一策略）。
pub(super) async fn setup_host() -> PluginHost {
    // AppConfig 全局初始化（与 wasm_runtime 测试同策略；重复 init 幂等）
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = AppConfig::default();
        config.network.port = 8765;
        AppConfig::init(config);
    });

    let db = Arc::new(Mutex::new(Database::new(&PathBuf::from(":memory:")).unwrap()));
    db.lock().await.init_schema().unwrap();
    let storage = Arc::new(PluginStorage::new(db.clone()));
    let session_manager = Arc::new(SessionManager::default());
    let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
        Database::new(&PathBuf::from(":memory:")).unwrap(),
    ))));

    let permission = Arc::new(PermissionManager::new());
    let registry = Arc::new(PluginRegistry::new());
    let message_bus = Arc::new(MessageBus::new());

    let wasm_runtime = Arc::new(WasmRuntime::new(storage.clone(), None).unwrap());

    let wasm_host_ctx = Arc::new(WasmHostContext::new(
        db,
        Arc::new(Mutex::new(HashMap::new())),
        storage.clone(),
        session_manager,
        config_manager,
        None,
        permission.clone(),
        wasm_runtime.fs_auth().clone(),
        message_bus.clone(),
    ));

    wasm_host_ctx.security().set_monitor(wasm_runtime.monitor());
    // v24 认证记录下沉：认证中心（terminal-session）配对/历史真源在插件私有库。
    // 无头测试无 AppHandle，注入进程级临时根使激活建表/播种/断言可达私有库
    // （auth-policy 闭环等依赖）
    wasm_host_ctx.set_plugin_db_root(Some(
        std::env::temp_dir().join(format!("bedcode-hosttest-pluginroot-{}", std::process::id())),
    ));

    PluginHost {
        plugins: Arc::new(RwLock::new(HashMap::new())),
        registry,
        permission,
        storage,
        rust_command_handlers: Arc::new(RwLock::new(HashMap::new())),
        rust_terminal_handlers: Arc::new(RwLock::new(Vec::new())),
        wasm_runtime,
        wasm_plugins: Arc::new(RwLock::new(HashMap::new())),
        wasm_host_ctx,
        message_bus,
        plugin_timers: Arc::new(std::sync::Mutex::new(HashMap::new())),
        wasm_reload_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
        runtime_error_notify_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
        shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        user_plugins_dir: std::env::temp_dir().join("bedcode-test-user-plugins"),
        frontend_channel: Arc::new(crate::wasm_core::security::frontend_channel::FrontendChannelRegistry::new()),
    }
}

/// 构造一个最小 LoadedPlugin（manifest 含 storage + terminal:input 权限）
pub(super) fn make_plugin(id: &str, source: PluginSource, state: PluginState) -> LoadedPlugin {
    LoadedPlugin {
        manifest: PluginManifest {
            id: id.to_string(),
            name: format!("Test {}", id),
            version: "1.0.0".to_string(),
            main: "index.ts".to_string(),
            permissions: vec!["storage".to_string(), "terminal:input".to_string()],
            plugin_type: PluginType::TsOnly,
            // 余下字段取 Default，避免 SDK 追加可选字段时本夹具编译红（票 14）
            ..Default::default()
        },
        state,
        extension_path: String::new(),
        activated_at: None,
        source,
    }
}

// ==================== 静态注册插件（builtin 常驻语义） ====================

/// 合成静态插件 ID（inventory 条目仅编译进测试二进制，不影响产物）
pub(super) const SYNTHETIC_STATIC_ID: &str = "com.bedcode.test-static";

/// 记录 on_startup 是否被宿主回调（inventory 条目进程级唯一，标志跨测试共享）
pub(super) static SYNTHETIC_ON_STARTUP_CALLED: AtomicBool = AtomicBool::new(false);

fn synthetic_manifest() -> PluginManifest {
    PluginManifest {
        id: SYNTHETIC_STATIC_ID.to_string(),
        name: "Synthetic Static".to_string(),
        version: "0.1.0".to_string(),
        plugin_type: PluginType::Rust,
        // 余下字段取 Default，避免 SDK 追加可选字段时本夹具编译红（票 14）
        ..Default::default()
    }
}

fn synthetic_on_startup() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
    Box::pin(async {
        SYNTHETIC_ON_STARTUP_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    })
}

fn synthetic_on_shutdown() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
    Box::pin(async { Ok(()) })
}

fn synthetic_noop_lifecycle(_ctx: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
    Box::pin(async { Ok(()) })
}

fn synthetic_commands() -> Vec<PluginCommand> {
    vec![PluginCommand::new("ping", |_args| async move {
        Ok(json!({ "pong": true }))
    })]
}

fn synthetic_terminal_handlers() -> Vec<Box<dyn TerminalHandler>> {
    Vec::new()
}

// 合成静态注册条目：等价 submit_plugin! 的 inventory 注册链路
inventory::submit! {
    bedcode_plugin_api::BedcodePluginEntry {
        id: SYNTHETIC_STATIC_ID,
        create_manifest: synthetic_manifest,
        activate: synthetic_noop_lifecycle,
        deactivate: synthetic_noop_lifecycle,
        register_commands: synthetic_commands,
        terminal_handlers: synthetic_terminal_handlers,
        on_startup: synthetic_on_startup,
        on_shutdown: synthetic_on_shutdown,
    }
}
