//! 持久化启用态（L3 业务应用）的激活时序契约。
//!
//! 现场（2026-10-18，Windows 正式版 v3.0.0 首次运行）：`PluginHost::new` 在宿主
//! setup 主线程上被 `tauri::async_runtime::block_on` 调用，而它内部同步执行
//! `auto_activate_from_persisted_state()`——首次运行没有授权记录，L3 插件激活要弹
//! 交互式目录授权，于是主线程被占满 `fs_auth::PROMPT_TIMEOUT`（30s）：
//!
//! - 窗口与宿主前端 30s 后才可用（日志 `后端初始化耗时: 32869ms`）；
//! - 更糟的是弹窗在宿主前端注册监听器之前发出，事件丢失、无人应答，
//!   30s 后按拒绝收敛（`file access authorization rejected`）——插件永远启用不上。
//!
//! 修复：构造器只装配机制，**不做**持久化激活；激活由宿主在启动钩子里异步触发
//! （`bedcode-desktop/src-tauri/src/system/lifecycle.rs`，本 crate 不持有端时序）。
//! 本用例锁住这条契约的两半：`new` 不激活 + 显式调用仍能激活。

use super::*;

/// 写一个最小 TS-only 业务应用插件包（无 `type` → 缺省角色 = L3 业务应用；
/// 无 `rustLibrary` → 不需要 wasm 产物，排除「实例化失败」这一无关变量）
fn write_business_app_plugin(plugins_dir: &std::path::Path, plugin_id: &str) {
    let dir = plugins_dir.join(plugin_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(PLUGIN_MANIFEST_FILE),
        format!(
            r#"{{"id": "{plugin_id}", "name": "Persisted Activation Test", "version": "1.0.0", "platform": "both", "main": "index.js", "permissions": ["storage"]}}"#
        ),
    )
    .unwrap();
}

/// 契约（正例 + 反例）：
///
/// 1. 反例——`PluginHost::new` 后插件仍是 `Loaded`（构造函数不激活持久化插件：
///    把 `auto_activate_from_persisted_state` 加回 `new` 本断言必红）；
/// 2. 正例——显式调用后变 `Activated`（拆出入口后宿主仍能恢复启用态，
///    否则「不阻塞」就变成了「永不启用」）。
#[tokio::test(flavor = "multi_thread")]
async fn new_host_defers_persisted_activation_to_explicit_call() {
    // AppConfig 全局初始化（与 setup_host 同策略，重复 init 幂等）
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = AppConfig::default();
        config.network.port = 8765;
        AppConfig::init(config);
    });

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = tmp_dir.path().join("builtin");
    let user_dir = tmp_dir.path().join("user");
    const PLUGIN_ID: &str = "com.bedcode.test-persisted-l3";
    write_business_app_plugin(&builtin_dir, PLUGIN_ID);

    // 预置持久化启用态（用户上次是启用着退出的）
    let db = Arc::new(std::sync::Mutex::new(
        Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
    ));
    db.lock().expect("db lock").init_schema().unwrap();
    PluginStorage::new(db.clone())
        .save_activated_plugins(&HashMap::from([(PLUGIN_ID.to_string(), true)]))
        .await
        .expect("seed persisted activation state");

    let host = PluginHost::new(
        db,
        &builtin_dir,
        &user_dir,
        None,
        None,
        Vec::new(),
        crate::manager::runtime::AbiPolicy::UNCONSTRAINED,
        std::sync::Arc::new(crate::host_api::ports::UnimplementedHostFacts),
        None,
    )
    .await;

    let state = host.get_plugin(PLUGIN_ID).await.expect("plugin present").state;
    assert_eq!(
        state,
        PluginState::Loaded,
        "`PluginHost::new` 不得激活持久化插件（激活时序归宿主启动钩子，构造函数只装配）"
    );

    host.auto_activate_from_persisted_state().await;
    let state = host.get_plugin(PLUGIN_ID).await.expect("plugin present").state;
    assert_eq!(
        state,
        PluginState::Activated,
        "显式触发的持久化激活必须生效（不阻塞启动 ≠ 永不恢复启用态）"
    );
}
