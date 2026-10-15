//! 移动插件宿主侧闭环（真实产物）：ai-chatbox `activate` 路径 + 未激活命令面
//!
//! 本文件是 **2026-10-15 真机崩溃链**的回归锁（真机症状：打开 AI Chatbox 即
//! 进程 SIGILL 退出）。链路三层，各自在此被正向钉住：
//!
//! 1. **宿主装配面**（`WasmHostContext::new_with_ports` = 移动 `PluginManager`
//!    用的构造器）必须注册安全框架的 fs 仲裁器。漏注册时 `authorize_fs` 归到
//!    「资源 Fs 未注册仲裁器，默认拒绝」⇒ `host-fs` 全部原语恒拒 ⇒ 插件
//!    `activate()` 在 `store::init`（`fs_exists`）失败。真机当时的表现就是
//!    `WASM activate() failed: ... fs_exists: permission denied`。
//! 2. **插件 activate 必须真的走得通**（用真产物跑，而不是夹具）：夹具只 import
//!    core 域，装配漏注册这类端装配缺陷在夹具上不可见——必须真产物 + 真链路。
//! 3. **未激活时命令面不得 panic**：activate 失败后插件前端仍会发命令（宿主壳
//!    只按前端装载状态放行），guest 侧若用 `expect("...during plugin activate")`
//!    就是 panic → wasm `unreachable` → 真机 trap 未被处理 → SIGILL。现在应返回
//!    带上下文的错误。
//!
//! 产物依赖：`resources/plugins/mobile/**` 由 `pnpm run plugins:build` 生成
//! （CI 的 rust-mobile job 在 `cargo test` 前会跑），缺失即显性失败、不静默跳过。

use std::sync::Arc;

use bedcode_mobile_lib as _;
use bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents;
use bedcode_mobile_lib::plugin::manager::mobile_core_config;
use bedcode_wasm_core::manager::runtime::AbiPolicy;
use bedcode_wasm_core::permission::{
    PERMISSION_FS_READ, PERMISSION_FS_WRITE, PERMISSION_NETWORK_HTTP, PERMISSION_STORAGE,
};
use bedcode_wasm_core::test_support::{
    build_host_ctx_with, runtime_engine, setup_headless_runtime_with_config, MockPorts,
};
use wasmtime::component::Component;

/// 真机插件 id（与 manifest 逐字一致）
const PLUGIN_ID: &str = "com.bedcode.ai-chatbox";

/// 真机构建产物（`pnpm run plugins:build` 落点，与设备 plugins/ 目录内同一份）
const WASM_REL: &str = "resources/plugins/mobile/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm";

/// 移动运行时装配（与生产 `PluginManager::init_wasm_runtime_with` 同一条配置路径）
///
/// 用 `mobile_core_config` 而不是测试默认：生产引擎配置（含移动端固定项「关信号式
/// trap」）必须与用例一致，否则本条闭环测不到产品实际装配态。
fn mobile_runtime(aot_cache_dir: Option<std::path::PathBuf>) -> bedcode_wasm_core::manager::runtime::WasmRuntime {
    let runtime = setup_headless_runtime_with_config(aot_cache_dir, AbiPolicy { form: None }, mobile_core_config(None))
        .expect("create wasm runtime");
    runtime.set_guest_events(Arc::new(MobileGuestEvents));
    runtime
}

/// 装好下载目录端事实与权限的宿主上下文（真机同形：`app.downloads_dir` 已注入、
/// manifest 权限已按生效集授予）
struct Fixture {
    _tmp: tempfile::TempDir,
    data_dir: std::path::PathBuf,
    host_ctx: Arc<bedcode_wasm_core::host_api::WasmHostContext>,
    runtime: bedcode_wasm_core::manager::runtime::WasmRuntime,
}

impl Fixture {
    fn new() -> Self {
        // 下载目录 = 真机 `/storage/emulated/0/Android/data/com.bedcode.mobile/files/Download`
        let tmp = tempfile::tempdir().expect("tempdir");
        let downloads = tmp.path().join("Download");
        let data_dir = downloads.join("ai-chatbox");
        std::fs::create_dir_all(&data_dir).expect("create data dir");

        let ports = MockPorts::new().with_app_downloads_dir(&downloads.to_string_lossy());
        let host_ctx = build_host_ctx_with(ports);
        host_ctx.grant_permissions(
            PLUGIN_ID,
            &[
                PERMISSION_FS_READ.to_string(),
                PERMISSION_FS_WRITE.to_string(),
                PERMISSION_NETWORK_HTTP.to_string(),
                PERMISSION_STORAGE.to_string(),
            ],
        );
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        Self {
            _tmp: tmp,
            data_dir,
            host_ctx,
            runtime,
        }
    }

    /// 实例化真产物（每次新建实例：插件实例不可跨用例复用，Store 状态会被前一个用例污染）
    fn instantiate(&self) -> bedcode_wasm_core::manager::runtime::LoadedWasmPlugin {
        let component = Component::from_file(runtime_engine(&self.runtime), WASM_REL)
            .expect("ai-chatbox 产物缺失或不可编译：先跑 `pnpm run plugins:build`");
        self.runtime
            .instantiate_component(&component, PLUGIN_ID, self.host_ctx.clone(), &[], None)
            .expect("instantiate ai-chatbox plugin")
    }

    /// 预授权数据目录（真机 manifest `preauthDirs: ${downloads}/ai-chatbox` 的等价动作：
    /// activate 之前宿主统一弹窗/授权）
    async fn preauthorize_data_dir(&self) {
        self.host_ctx
            .fs_auth_checker()
            .seed_legacy_granted_path(PLUGIN_ID, &self.data_dir.to_string_lossy())
            .await
            .expect("seed fs grant");
    }
}

/// 真产物 activate 全链：宿主 fs 闸门放行 → 数据目录落盘 → 返回成功码
#[test]
fn real_plugin_activate_initializes_data_dir() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let fixture = Fixture::new();
        // 引擎配置锁：本用例跑的就是生产移动端引擎（信号式 trap 关闭）
        assert!(
            !runtime_engine(&fixture.runtime).get_signals_based_traps(),
            "移动端引擎必须关掉信号式 trap（否则 guest trap = 宿主进程 SIGILL）"
        );
        fixture.preauthorize_data_dir().await;
        let mut plugin = fixture.instantiate();

        let code = plugin.activate().expect("activate 应成功（宿主 fs 闸门必须放行）");
        assert_eq!(code, 0, "activate 成功码");

        // store::init 的真实副作用：两个初始文件落盘
        let mut names: Vec<String> = std::fs::read_dir(&fixture.data_dir)
            .expect("data dir readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["index.jsonl".to_string(), "providers.json".to_string()]);
    });
}

/// 未激活（activate 失败）时命令面返回带上下文的错误，**不得 panic / trap**
///
/// 真机链路：数据目录未授权 ⇒ activate 失败 ⇒ 插件前端照常调 `list-conversations`
/// ⇒ 旧实现的 `expect("data_dir must be initialized during plugin activate")` panic
/// ⇒ wasm `unreachable` ⇒ 真机 trap 未被处理 ⇒ 进程 SIGILL。
#[test]
fn command_before_successful_activate_returns_error() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let fixture = Fixture::new();
        // 故意不预授权：activate 在 fs_request_auth 被拒（等价真机目录授权被拒路径）
        let mut plugin = fixture.instantiate();
        plugin.activate().expect_err("未授权时 activate 应失败");

        // SDK 契约：guest 命令的 Err 经 `{"error": …}` 载荷回带（不是宿主级 Err）
        let out = plugin
            .invoke_command("ai-chatbox.list-conversations", "{}")
            .expect("未激活时命令不得 trap（历史缺陷：expect panic → __rust_abort → 真机 SIGILL）");
        let payload: serde_json::Value = serde_json::from_str(&out).expect("命令回带 JSON 载荷");
        let msg = payload.get("error").and_then(|v| v.as_str()).unwrap_or_default();
        assert!(
            msg.contains("not activated"),
            "应是带上下文的未激活错误，实际载荷: {out}"
        );
        // 反向锁：不得退化为 wasm trap（trap 文案带 wasm backtrace / __rust_abort）
        assert!(
            !out.contains("wasm backtrace") && !out.contains("__rust_abort"),
            "不得退化为 guest trap: {out}"
        );
    });
}
