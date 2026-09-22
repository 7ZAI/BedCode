//! 系统组件（capability 路由 / 认证策略闭环 / trap 隔离 / boot 次序）用例。

use super::*;
use super::scaffold::*;
use super::wasm_flow_test::*;


/// 系统组件 fixture 插件 ID（与 packages/plugin-system-test 的 manifest 一致）
const TEST_SYSTEM_PLUGIN_ID: &str = "com.bedcode.system-test";

/// 构建系统组件 fixture 并编码为组件（packages/plugin-system-test，
/// 与 build_test_component 同策略：mtime 新鲜度检查 + cargo build）
fn build_system_test_component() -> Vec<u8> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packages_dir = manifest_dir.join("../packages");
    let plugin_dir = packages_dir.join("plugin-system-test");

    let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
    let module_path = output_dir.join("bedcode_plugin_system_test.wasm");

    if module_path.exists() {
        let src_files = [
            plugin_dir.join("src/lib.rs"),
            packages_dir.join("plugin-sdk-desktop/rust/wit/bedcode.wit"),
        ];
        let module_modified = std::fs::metadata(&module_path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let needs_rebuild = src_files.iter().any(|f| {
            std::fs::metadata(f)
                .and_then(|m| m.modified())
                .map(|t| t > module_modified)
                .unwrap_or(true)
        });
        if !needs_rebuild {
            return std::fs::read(&module_path).expect("Failed to read system test module");
        }
    }

    let status = std::process::Command::new("cargo")
        .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
        .args([
            "build",
            "--target",
            "wasm32-wasip3",
            "--release",
            "--manifest-path",
            plugin_dir.join("Cargo.toml").to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run cargo build for system test component");
    assert!(status.success(), "System test component WASM build failed");
    std::fs::read(&module_path).expect("Failed to read system test module after build")
}

/// 实例化系统组件 fixture 并注入宿主（kind=System，探测断言含 host-storage）
async fn setup_system_component(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
    let component = host
        .wasm_runtime()
        .compile_component(&build_system_test_component())
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

    host.wasm_plugins
        .write()
        .await
        .insert(TEST_SYSTEM_PLUGIN_ID.to_string(), Arc::new(Mutex::new(plugin)));

    let mut loaded = make_plugin(TEST_SYSTEM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_system_test".to_string();
    loaded.manifest.kind = bedcode_plugin_api::PluginKind::System;
    loaded.extension_path = tmp_dir.path().to_string_lossy().to_string();
    host.plugins
        .write()
        .await
        .insert(TEST_SYSTEM_PLUGIN_ID.to_string(), loaded);

    TEST_SYSTEM_PLUGIN_ID.to_string()
}

/// 装配闭环：系统组件注册能力 + 应用插件经 Linker 路由消费，
/// 读到的值来自系统组件实例私有 KV 而非宿主 SQLite（证明转发到达组件实例）
#[tokio::test(flavor = "multi_thread")]
async fn test_system_component_capability_routing_end_to_end() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let sys_id = setup_system_component(&host, &tmp_dir).await;
    let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

    // 应用插件声明能力依赖（激活时校验）
    host.plugins
        .write()
        .await
        .get_mut(&app_id)
        .unwrap()
        .manifest
        .dependencies = vec!["host-storage".to_string()];

    // 宿主 SQLite 预写对照值（若未路由，应用插件将读到它）
    host.storage()
        .set(&app_id, "component-test-key", json!({"k": "v"}))
        .await
        .expect("preset host storage key");

    // 系统组件先激活 → 能力注册表切换为系统组件提供者
    host.activate_plugin(&sys_id, false)
        .await
        .expect("activate system component");
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", sys_id),
        "host-storage must be provided by system component after activation"
    );

    // 应用插件后激活：依赖检查命中系统组件提供者
    host.activate_plugin(&app_id, false).await.expect("activate app plugin");

    // 预置系统组件实例的私有 KV（host-side 直接调用其能力导出）
    let sys_inst = host.get_wasm_plugin(&sys_id).await.unwrap();
    let set_result = sys_inst
        .lock()
        .await
        .call_capability_export::<(String, String), (Result<(), String>,)>(
            "bedcode:plugin/host-storage.set",
            ("component-test-key".to_string(), r#"{"sys":"routed"}"#.to_string()),
        )
        .expect("capability set transport");
    assert!(set_result.0.is_ok(), "capability set guest result: {:?}", set_result);

    // 应用插件消费：invoke 内 host_storage::get("component-test-key") 应经
    // Linker 路由转发到系统组件实例（读到系统组件私有值，而非宿主 SQLite）
    let result = host
        .invoke_rust_command(&app_id, "test.echo", json!({}))
        .await
        .expect("invoke app command");
    assert_eq!(
        result["stored"],
        json!({"sys": "routed"}),
        "routed read must return system component value, got: {}",
        result
    );

    // 显式按键读取同样路由
    let result = host
        .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "component-test-key"}))
        .await
        .expect("invoke storage-get");
    assert_eq!(result["value"], json!({"sys": "routed"}), "got: {}", result);
}

/// 票 05 闭环：server 认证策略（验签 → 会话中心 `auth-policy` 导出 → 放行/拒绝）
///
/// 真实会话中心 wasip3 产物经宿主 async 运行时加载：验签由宿主 `JwtService`
/// 执行（中间件路径，密码学引擎不移动），验签后经
/// `auth_center::enforce_connection_policy` 取会话中心 `auth-policy` capability
/// 导出做策略裁决（结构 / claims / 时效 + **信任撤销检查**）。覆盖：
/// - 未激活（api 注册表无标记）→ 宿主策略回退（Ok，无单点）
/// - 激活 + 内核 `pairings` 空 → 放行（无信任锚点只凭验签，搬迁前语义）
/// - 内核存在活跃配对记录 → 放行
/// - 经插件撤销（`session.trust.revoke` 软删内核真源）→ 拒绝（原因透出）
/// - 未撤销的其他设备 token → 放行
/// - 实例消失（停用）→ 能力调用失败 → 宿主策略回退（Ok）
///
/// 票 05 落地：策略目标自旧认证中心插件改指会话中心；信任判据自插件私有镜像
/// 改为内核 `pairings` 表（host-auth 记录面），测试直查内核真源断言软删。
#[tokio::test(flavor = "multi_thread")]
async fn test_server_auth_policy_closed_loop() {
    use crate::utils::auth::auth_center as bridge;

    const PAIRING_ID: &str = "p-policy";
    let session_id = bridge::SESSION_PLUGIN_ID;
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let host = setup_host().await;
    let component = host
        .wasm_runtime()
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");
    let plugin = host
        .wasm_runtime()
        .instantiate_component(&component, session_id, host.wasm_host_ctx().clone(), &[], None)
        .expect("instantiate session");
    if !plugin.exported_capabilities().iter().any(|c| c == "auth-policy") {
        eprintln!("[skip] session artifact lacks auth-policy export (rebuild with current SDK)");
        return;
    }
    host.wasm_plugins
        .write()
        .await
        .insert(session_id.to_string(), Arc::new(Mutex::new(plugin)));
    let mut loaded = make_plugin(session_id, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_terminal_session".to_string();
    // 权限经 manifest 声明在 activate 时授予（生产装配路径；手工 grant 会被
    // activate 的 manifest 重新授权覆盖）
    loaded.manifest.permissions = vec!["auth".to_string(), "peer".to_string()];
    host.plugins.write().await.insert(session_id.to_string(), loaded);

    host.wasm_host_ctx().api_registry().register(session_id, &[]);
    host.activate_plugin(session_id, false).await.expect("activate session");

    // 验签在宿主执行（中间件路径）：无效 token 连策略都到不了
    let bad = "not-a-jwt";
    assert!(crate::utils::auth::JwtService::new()
        .verify_token_with_expiry(bad)
        .is_err());

    // ============ 未激活 → 宿主策略回退（无单点） ============
    assert!(
        !bridge::session_active(host.wasm_host_ctx()),
        "未注册标记 api → 视为未激活"
    );
    let valid_token = crate::utils::auth::JwtService::new()
        .generate_token(
            "device-1".to_string(),
            Some("Pixel 9".to_string()),
            Some("fp-abc".to_string()),
        )
        .expect("issue host token");
    crate::utils::auth::JwtService::new()
        .verify_token_with_expiry(&valid_token)
        .expect("host verifies signature");
    assert!(
        bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
        "会话中心未激活 → 宿主策略放行"
    );

    // ============ 激活 → 策略取会话中心（锚点 = trust-list） ============
    host.wasm_host_ctx()
        .api_registry()
        .register(session_id, &[bridge::SESSION_MARKER_API.to_string()]);
    assert!(bridge::session_active(host.wasm_host_ctx()));

    // 内核无配对记录 → 放行（无信任锚点，仅凭验签；搬迁前语义）
    assert!(
        bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
        "内核无记录 → 放行"
    );

    // 内核写入活跃配对记录（配对完成流的宿主写入路径）→ 放行
    {
        let db = host.wasm_host_ctx().database().lock().await;
        db.conn()
            .execute(
                "INSERT INTO pairings (id, device_name, device_fingerprint, public_key, address, \
                 paired_at, connect_count, is_active) VALUES (?1, 'Pixel 9', 'fp-abc', 'pk', NULL, \
                 '2026-09-19T00:00:00Z', 1, 1)",
                rusqlite::params![PAIRING_ID],
            )
            .expect("seed kernel pairing");
    }
    assert!(
        bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
        "已配对设备 → 放行"
    );

    // 经插件撤销（软删内核真源）→ 拒绝（拒绝原因必须可读）
    let revoked = host
        .invoke_rust_command(session_id, "session.trust.revoke", json!({"id": PAIRING_ID}))
        .await
        .expect("session.trust.revoke");
    assert_eq!(revoked["removed"], true);
    {
        let db = host.wasm_host_ctx().database().lock().await;
        let active: i32 = db
            .conn()
            .query_row(
                "SELECT is_active FROM pairings WHERE id = ?1",
                rusqlite::params![PAIRING_ID],
                |row| row.get(0),
            )
            .expect("软删保留记录");
        assert_eq!(active, 0, "撤销由插件写内核真源（host-auth 记录面）");
    }
    let deny = bridge::enforce_connection_policy(&host, &valid_token).expect_err("撤销后必须拒绝");
    assert!(deny.contains("revoked"), "拒绝原因可读: {}", deny);

    // 未撤销记录的其他设备 token → 放行（内核未命中从宽，搬迁前语义）
    let other_token = crate::utils::auth::JwtService::new()
        .generate_token(
            "device-2".to_string(),
            Some("Phone 2".to_string()),
            Some("fp-xyz".to_string()),
        )
        .expect("issue other token");
    assert!(
        bridge::enforce_connection_policy(&host, &other_token).is_ok(),
        "未撤销设备 → 放行"
    );

    // ============ 实例消失 → 能力调用失败 → 宿主策略回退 ============
    host.wasm_plugins.write().await.remove(session_id);
    assert!(
        bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
        "实例缺失 → 宿主策略回退（会话中心故障不误杀全部连接）"
    );
}

/// 依赖缺失：应用插件声明未知能力名 → 激活失败，错误信息指明能力名
#[tokio::test(flavor = "multi_thread")]
async fn test_activation_fails_with_missing_dependency_named() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

    host.plugins
        .write()
        .await
        .get_mut(&app_id)
        .unwrap()
        .manifest
        .dependencies = vec!["host-no-such-cap".to_string()];

    let err = host
        .activate_plugin(&app_id, false)
        .await
        .expect_err("activation must fail on missing capability");
    let msg = err.to_string();
    assert!(
        msg.contains("host-no-such-cap"),
        "error must name the missing capability, got: {}",
        msg
    );
    // 失败落 Error 终态（不留悬挂 Activating）
    let info = host.get_plugin(&app_id).await.unwrap();
    assert!(
        matches!(&info.state, PluginState::Error(e) if e.contains("host-no-such-cap")),
        "expected Error state naming missing capability, got {:?}",
        info.state
    );

    // 依赖宿主原语能力则放行（host-storage 恒由宿主原语/系统组件提供）
    host.plugins
        .write()
        .await
        .get_mut(&app_id)
        .unwrap()
        .manifest
        .dependencies = vec!["host-storage".to_string()];
    host.activate_plugin(&app_id, false)
        .await
        .expect("host primitive capability dependency must be satisfiable");
}

/// 系统组件 trap 隔离：转发调用中系统组件 panic，错误隔离为应用插件的
/// Err 返回（应用插件实例不中毒、可继续调用），能力回落宿主原语
#[tokio::test(flavor = "multi_thread")]
async fn test_system_component_trap_isolated_and_reverts_to_host() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let sys_id = setup_system_component(&host, &tmp_dir).await;
    let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

    host.storage()
        .set(&app_id, "component-test-key", json!({"k": "v"}))
        .await
        .expect("preset host storage key");
    host.activate_plugin(&sys_id, false)
        .await
        .expect("activate system component");
    host.activate_plugin(&app_id, false).await.expect("activate app plugin");
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", sys_id)
    );

    // 触发系统组件 trap（sys-test.panic key）：应用插件的 invoke 不 trap，
    // guest 收到 Err 并序列化进 storageError 字段（trap 不跨实例扩散）
    let result = host
        .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "sys-test.panic"}))
        .await
        .expect("app plugin invoke must survive system component trap");
    let storage_error = result["storageError"].as_str().unwrap_or("");
    assert!(
        storage_error.contains("system component capability call failed"),
        "forwarded trap must surface as guest-visible error, got: {}",
        result
    );

    // 能力自愈：trap 后回落宿主原语，后续调用读到宿主 SQLite 对照值
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        "host",
        "capability must revert to host primitive after system component trap"
    );
    let result = host
        .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "component-test-key"}))
        .await
        .expect("app plugin must keep working after revert");
    assert_eq!(
        result["value"],
        json!({"k": "v"}),
        "host primitive fallback, got: {}",
        result
    );
}

/// 系统组件停用（只停不删）：能力回落宿主原语；重新激活后再装配
#[tokio::test(flavor = "multi_thread")]
async fn test_deactivate_system_component_reverts_capability() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let sys_id = setup_system_component(&host, &tmp_dir).await;

    host.activate_plugin(&sys_id, false)
        .await
        .expect("activate system component");
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", sys_id)
    );

    host.deactivate_plugin(&sys_id, false)
        .await
        .expect("deactivate system component");
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        "host",
        "capability must revert to host primitive on system component deactivation"
    );

    // 系统组件启停不持久化（默认启用语义：持久化真源是「内置」而非用户状态）
    let persisted = host.get_activated_state().await;
    assert!(
        !persisted.contains_key(&sys_id),
        "system component must be excluded from persisted activation state"
    );

    // 重新激活 → 能力再装配
    host.activate_plugin(&sys_id, false)
        .await
        .expect("re-activate system component");
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", sys_id)
    );
}

/// 启动加载顺序（集成测试）：PluginHost::new 全路径——系统组件先于应用
/// 插件激活，应用插件（持久化启用 + 能力依赖）激活时注册表已含系统组件
/// 提供者；两者终态均 Activated
#[tokio::test(flavor = "multi_thread")]
async fn test_boot_activates_system_components_before_app_plugins() {
    // AppConfig 初始化（与 setup_host 同策略，重复 init 幂等）
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = AppConfig::default();
        config.network.port = 8765;
        AppConfig::init(config);
    });

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let plugins_dir = tmp_dir.path().join("plugins");
    let sys_id = "com.bedcode.system-test";
    let app_id = "com.bedcode.component-test";

    // 写两个插件包：系统组件（type=system）+ 应用插件（dependencies）
    for (id, rust_lib, manifest_extra) in [
        (sys_id, "bedcode_plugin_system_test", r#", "type": "system""#),
        (
            app_id,
            "bedcode_plugin_component_test",
            r#", "dependencies": ["host-storage"]"#,
        ),
    ] {
        let dir = plugins_dir.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            r#"{{"id": "{}", "name": "{}", "version": "0.1.0", "pluginType": "rust-ts", "rustLibrary": "{}", "permissions": ["storage"]{}}}"#,
            id, id, rust_lib, manifest_extra
        );
        std::fs::write(dir.join("plugin.json"), manifest).unwrap();
        let component = if id == sys_id {
            build_system_test_component()
        } else {
            build_test_component()
        };
        std::fs::write(dir.join(format!("{}.wasm", rust_lib)), component).unwrap();
    }

    // 预置持久化启用状态：仅应用插件（系统组件默认启用、无需持久化）
    let db = Arc::new(Mutex::new(
        Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
    ));
    db.lock().await.init_schema().unwrap();
    PluginStorage::new(db.clone())
        .save_activated_plugins(&HashMap::from([(app_id.to_string(), true)]))
        .await
        .expect("seed persisted activation state");

    let session_manager = Arc::new(SessionManager::default());
    let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
        Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
    ))));

    // 用户插件目录（dev 合入的第 3 参）：本用例无用户安装插件，指向空临时目录
    let user_plugins_dir = tmp_dir.path().join("user-plugins");
    let host = PluginHost::new(
        db,
        &plugins_dir,
        &user_plugins_dir,
        session_manager,
        config_manager,
        None,
    )
    .await;

    // 终态：两者均 Activated（应用插件的依赖检查在系统组件装配之后执行，
    // 激活成功即顺序成立的语义断言）
    let sys_info = host.get_plugin(sys_id).await.expect("system component present");
    let app_info = host.get_plugin(app_id).await.expect("app plugin present");
    assert_eq!(sys_info.state, PluginState::Activated, "system component state");
    assert_eq!(app_info.state, PluginState::Activated, "app plugin state");

    // 能力注册表：host-storage 由系统组件提供
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", sys_id),
        "host-storage must be assembled to system component at boot"
    );

    // 激活时序：系统组件不晚于应用插件（语义断言之上的时序佐证）
    let (sys_at, app_at) = {
        let plugins = host.plugins.read().await;
        (
            plugins.get(sys_id).and_then(|p| p.activated_at),
            plugins.get(app_id).and_then(|p| p.activated_at),
        )
    };
    match (sys_at, app_at) {
        (Some(sys_at), Some(app_at)) => assert!(sys_at <= app_at, "system component must activate first"),
        _ => panic!("both plugins must record activated_at"),
    }
}

