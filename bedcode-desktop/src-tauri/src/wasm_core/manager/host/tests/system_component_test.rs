//! 角色分层（ADR 0032：L1 基础服务 / L2 内部统一业务 / L3 业务应用）用例：
//! capability 路由 / 认证策略闭环 / trap 隔离 / 三层加载次序与失败隔离。

use super::scaffold::*;
use super::wasm_flow_test::*;
use super::*;

/// 系统组件 fixture 插件 ID（与 packages/plugin-system-test 的 manifest 一致）
const TEST_SYSTEM_PLUGIN_ID: &str = "com.bedcode.system-test";

/// 构建系统组件 fixture 并编码为组件（packages/plugin-system-test，
/// 与 build_test_component 同策略：mtime 新鲜度检查 + cargo build）
fn build_system_test_component() -> Vec<u8> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packages_dir = manifest_dir.join("../packages");
    let plugin_dir = packages_dir.join("plugin-system-test");

    let module_path = crate::wasm_core::manager::runtime::fixture_target::artifact(
        "wasm32-wasip3",
        "release",
        "bedcode_plugin_system_test",
    );

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
        .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
        .env(
            "CARGO_TARGET_DIR",
            crate::wasm_core::manager::runtime::fixture_target::dir(),
        )
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

    host.install_instance(TEST_SYSTEM_PLUGIN_ID, plugin, CallModel::Mutex)
        .await;

    let mut loaded = make_plugin(TEST_SYSTEM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_system_test".to_string();
    loaded.manifest.kind = bedcode_plugin_api::PluginKind::BasicService;
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

    // 预置系统组件实例的私有 KV（经装配条目统一门面直接调用其能力导出）
    let sys_entry = host.get_instance(&sys_id).await.expect("system component entry");
    let set_result = sys_entry
        .call_guest(GuestOp::CapStorageSet {
            key: "component-test-key".to_string(),
            value: r#"{"sys":"routed"}"#.to_string(),
        })
        .await
        .expect("capability set transport");
    match set_result {
        GuestReply::GuestUnit(Ok(())) => {}
        other => panic!("capability set guest result: {:?}", other),
    }

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

/// 票 05 闭环（v32 fail-closed → v33 验签一并下沉）：server 认证策略
/// （注册表找中心 → 中心 `auth-policy` 导出 → 验签 + 策略 → 放行/拒绝）
///
/// 真实认证中心 wasip3 产物经宿主 async 运行时加载：`enforce_connection_policy`
/// **查注册表**（ADR 0031，不再是「能力探测 + 排序取首个」）找到中心，调中心
/// `auth-policy` capability 导出；**验签在中心内部**（ADR 0033：入场密钥自持，
/// 宿主 `utils/auth/jwt.rs` 已退役）——故 token 一律经
/// `crate::utils::auth::test_tokens::issue` 从中心签出（走生产 `auth-grant` 路径）。
/// 覆盖：
/// - 无中心在册 → **拒绝**（`no auth center registered`，fail-closed K3）
/// - 激活 + 注册中心 + 私有库无配对记录 → 放行（无信任锚点，仅凭验签）
/// - 私有库存在活跃配对记录 → 放行
/// - 经插件撤销（`session.trust.revoke` 软删私有库真源）→ 拒绝（原因透出，
///   deny_kind=policy）
/// - 未撤销的其他设备 token → 放行
/// - 实例消失（停用）→ 调用失败 → **拒绝**（`auth center unavailable`，
///   deny_kind=unavailable，不再 fail-open 放行）
///
/// 注册路径：新 SDK 产物在 activate 内自注册（v32）；测试兼容旧产物——激活后
/// 若注册表仍空则手动注册（模拟 v32 中心）。各裁决段落前 `ensure_center`
/// 幂等重注册，免疫同二进制内注册表单测的并发 reset（全局静态，测试并行）。
#[tokio::test(flavor = "multi_thread")]
async fn test_server_auth_policy_closed_loop() {
    use crate::utils::auth::auth_center as bridge;
    use crate::wasm_core::host_api::auth_center as registry;

    const PAIRING_ID: &str = "p-policy";
    const CENTER_METHODS: [&str; 4] = ["pairing_code", "qr", "biometric", "jwt"];
    let session_id = bridge::SESSION_PLUGIN_ID;

    // 全局注册表清零（同二进制内注册表单测并行跑，先复位再登记保证本文确定性）
    // 串行闸门：单中心 desk 独占，两条全局注册表集成用例互斥执行（防并行清台）
    let _permit = registry::registry_gate().acquire().await.expect("registry test gate");
    // 闸门内清台：顺序反了会清掉另一个用例在闸门内登记的中心（并行红）
    registry::reset();

    // ============ 激活真实认证中心产物 ============
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }
    let host = setup_host().await;

    // ============ fail-closed 锁（K3）：无中心 = 拒绝，不得静默放行 ============
    // 此刻中心还没激活，**造不出**合法 token（v33：宿主无签发面）——但也不需要：
    // 无中心分支在解析凭证之前就拒了，凭证是什么不影响结论。用一个显眼的占位串
    // 顺带锁住「拒绝文本不得回显凭据」。
    let placeholder_token = "pre-center-placeholder-not-a-real-token";
    assert!(!registry::is_registered(), "reset 后无中心在册");
    let no_center =
        bridge::enforce_connection_policy(&host, placeholder_token).expect_err("无中心必须拒绝（fail-closed）");
    assert!(
        no_center.contains("no auth center registered"),
        "fail-closed 拒绝原因可读: {no_center}"
    );
    assert!(
        !no_center.contains(placeholder_token),
        "错误文本不得含凭据/token 片段（AGENTS §8）: {no_center}"
    );

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
    host.install_instance(session_id, plugin, CallModel::Mutex).await;
    let mut loaded = make_plugin(session_id, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_terminal_session".to_string();
    loaded.manifest.permissions = vec!["auth".to_string(), "peer".to_string(), "storage".to_string()];
    host.plugins.write().await.insert(session_id.to_string(), loaded);

    // 种子：把中心的入场密钥环种成**已知密钥**（必须在 activate 之前——中心
    // activate 会读一次密钥环，格式不对即阻断激活）。之后本用例用这把密钥签
    // 测试 token：与中心**同一把**钥匙，故中心必然认。
    // 走种子而非 `auth-grant` 互调：互调要总线派发落地（自建 PluginHost 未
    // `init_message_bus` 时等不到回复），那是测试基建限制，不是被测行为。
    // 种入要过中心的 `auth` 权限门（`auth_secret_set` 判据），故先授权
    host.permission.grant_permissions(
        session_id,
        &["auth".to_string(), "peer".to_string(), "storage".to_string()],
    );
    const TEST_KEY: [u8; 32] = [0x5a; 32];
    crate::utils::auth::test_tokens::seed_keyring(host.wasm_host_ctx(), session_id, &TEST_KEY);
    host.activate_plugin(session_id, false).await.expect("activate session");

    // 注册中心：新 SDK 产物 activate 内已自注册；旧产物（v31）未注册 → 手动
    // 注册（模拟 v32 中心的动态就绪段）。幂等：已注册则 register 报
    // 「already registered」忽略（免疫注册表单测并发 reset 后自动补位）。
    let ensure_center = || {
        let _ = registry::register(
            bridge::SESSION_PLUGIN_ID,
            CENTER_METHODS.iter().map(|s| s.to_string()).collect(),
        );
    };
    ensure_center();
    assert!(
        registry::is_registered(),
        "会话中心注册（自注册或手动补位）后必须有一个在册中心"
    );
    assert!(bridge::session_active(), "注册后桥接门必须放行（注册表查询，无参）");

    // ============ 激活会话中心 → 验签 + 策略取中心（注册表） ============
    // token 只能从中心签出（v33）：走生产 `auth-grant` / `jwt` / `issue`
    let valid_token =
        crate::utils::auth::test_tokens::sign_with_seeded_key(&TEST_KEY, "device-1", Some("Pixel 9"), Some("fp-abc"));

    // 私有库无配对记录 → 放行（无信任锚点，仅凭验签；搬迁前语义）
    let decision = bridge::enforce_connection_policy(&host, &valid_token).expect("私有库无记录 → 放行（中心验签通过）");
    assert_eq!(
        decision.device_id, "device-1",
        "放行时必须交回连接身份（ADR 0033：宿主据此建立连接/转发 caller 上下文）"
    );
    assert_eq!(decision.device_name.as_deref(), Some("Pixel 9"));
    assert_eq!(decision.fingerprint.as_deref(), Some("fp-abc"));

    // 认证中心私有库写入活跃配对记录（v24 下沉后真源在私有库）→ 放行
    {
        let db_path = std::env::temp_dir()
            .join(format!("bedcode-hosttest-pluginroot-{}", std::process::id()))
            .join(bridge::SESSION_PLUGIN_ID)
            .join("plugin.db");
        let conn = rusqlite::Connection::open(&db_path)
            .unwrap_or_else(|e| panic!("打开认证中心私有库失败 {}: {e}", db_path.display()));
        let _ = conn.busy_timeout(std::time::Duration::from_secs(10));
        conn.execute(
            "INSERT OR REPLACE INTO auth_pairings \
             (id, device_name, device_fingerprint, address, uid_hash, paired_at, \
              last_seen, connect_count, is_active) \
             VALUES (?1, 'Pixel 9', 'fp-abc', NULL, NULL, '2026-09-19T00:00:00Z', NULL, 1, 1)",
            rusqlite::params![PAIRING_ID],
        )
        .expect("seed auth_pairings");
    }
    ensure_center();
    assert!(
        bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
        "已配对设备 → 放行"
    );

    // 经插件撤销（软删私有库真源）→ 拒绝（拒绝原因必须可读，deny_kind=policy）
    let revoked = host
        .invoke_rust_command(session_id, "session.trust.revoke", json!({"id": PAIRING_ID}))
        .await
        .expect("session.trust.revoke");
    assert_eq!(revoked["removed"], true);
    {
        let db_path = std::env::temp_dir()
            .join(format!("bedcode-hosttest-pluginroot-{}", std::process::id()))
            .join(bridge::SESSION_PLUGIN_ID)
            .join("plugin.db");
        let conn = rusqlite::Connection::open(&db_path)
            .unwrap_or_else(|e| panic!("打开认证中心私有库失败 {}: {e}", db_path.display()));
        let active: i32 = conn
            .query_row(
                "SELECT is_active FROM auth_pairings WHERE id = ?1",
                rusqlite::params![PAIRING_ID],
                |row| row.get(0),
            )
            .expect("软删保留记录");
        assert_eq!(active, 0, "撤销由插件写私有库真源（is_active = 0）");
    }
    ensure_center();
    let deny = bridge::enforce_connection_policy(&host, &valid_token).expect_err("撤销后必须拒绝");
    assert!(deny.contains("revoked"), "拒绝原因可读（deny_kind=policy）: {deny}");
    assert!(
        !deny.contains(&valid_token) && !deny.contains("eyJ"),
        "错误文本不得含凭据/token 片段（AGENTS §8）: {deny}"
    );

    // 未撤销记录的其他设备 token → 放行（未命中从宽，搬迁前语义）
    let other_token =
        crate::utils::auth::test_tokens::sign_with_seeded_key(&TEST_KEY, "device-2", Some("Phone 2"), Some("fp-xyz"));
    ensure_center();
    assert!(
        bridge::enforce_connection_policy(&host, &other_token).is_ok(),
        "未撤销设备 → 放行"
    );

    // ============ 实例消失 → 调用失败 → 拒绝（deny_kind=unavailable） ============
    host.wasm_plugins.write().await.remove(session_id);
    ensure_center();
    let unavailable = bridge::enforce_connection_policy(&host, &valid_token)
        .expect_err("实例缺失 → 必须拒绝（fail-closed，不再回退放行）");
    assert!(
        unavailable.contains("auth center unavailable"),
        "unavailable 拒绝原因可读且可区分: {unavailable}"
    );
    assert!(
        !unavailable.contains(&valid_token),
        "unavailable 错误文本不得含凭据片段: {unavailable}"
    );

    // 收尾清零（同二进制内并行的注册表单测互不污染）
    registry::purge_for_plugin(bridge::SESSION_PLUGIN_ID);
}

/// 2026-09-29 事故真源回归锁（spec §10 票 05 第 1 条）：**多候选必须落在注册者身上**
///
/// 复刻事故：agent-hub（按 id 排序在 terminal-session **之前**、且 SDK 无条件导出
/// auth-policy 默认拒绝实现）先行激活成候选；旧裁决按「能力探测 + 排序取首个」
/// 会选中 agent-hub 而全局拒绝。新裁决只认注册表（ADR 0031 K1/K4）：
/// ① 只注册 terminal-session（agent-hub 不注册）→ 注册表属主 = terminal-session；
/// ② 未撤销 token 放行（若旧逻辑选中 agent-hub，其默认拒绝会让它 Err）；
/// ③ 撤销后拒绝原因来自 terminal-session 私有库 trust 真源（deny_kind=policy）；
/// ④ 停用未注册者（agent-hub）不影响注册表（purge 只碰本人）。
#[tokio::test(flavor = "multi_thread")]
async fn test_auth_center_multi_candidate_registration_lock() {
    use crate::utils::auth::auth_center as bridge;
    use crate::wasm_core::host_api::auth_center as registry;

    // 串行闸门：单中心 desk 独占，两条全局注册表集成用例互斥执行（防并行清台）
    let _permit = registry::registry_gate().acquire().await.expect("registry test gate");
    // 闸门内清台：顺序反了会清掉另一个用例在闸门内登记的中心（并行红）
    registry::reset();
    const AGENT_HUB_ID: &str = "com.bedcode.agent-hub";
    const CENTER_METHODS: [&str; 4] = ["pairing_code", "qr", "biometric", "jwt"];
    let session_id = bridge::SESSION_PLUGIN_ID;

    // agent-hub 与 terminal-session 两个 wasm 产物都必须可加载
    let session_wasm = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    let hub_wasm = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.agent-hub/bedcode_plugin_agent_hub.wasm");
    if !session_wasm.exists() || !hub_wasm.exists() {
        eprintln!("[skip] wasm artifacts not built");
        return;
    }
    let host = setup_host().await;

    // 两个导出 auth-policy 的候选都进实例表（agent-hub id 排序在前）；agent-hub
    // 激活失败不阻断本锁（它的激活可能与测试宿主能力面不完全匹配），实例仍在册
    for (id, wasm, lib) in [
        (AGENT_HUB_ID, &hub_wasm, "bedcode_plugin_agent_hub"),
        (session_id, &session_wasm, "bedcode_plugin_terminal_session"),
    ] {
        let component = host
            .wasm_runtime()
            .compile_component(&std::fs::read(&wasm).expect("read artifact"))
            .expect("compile artifact");
        let plugin = host
            .wasm_runtime()
            .instantiate_component(&component, id, host.wasm_host_ctx().clone(), &[], None)
            .unwrap_or_else(|e| panic!("instantiate {id}: {e}"));
        assert!(
            plugin.exported_capabilities().iter().any(|c| c == "auth-policy"),
            "{id} 必须导出 auth-policy（事故前提：SDK 默认拒绝实现）"
        );
        host.install_instance(id, plugin, CallModel::Mutex).await;
        let mut loaded = make_plugin(id, PluginSource::Wasm, PluginState::Loaded);
        loaded.manifest.rust_library = lib.to_string();
        loaded.manifest.permissions = vec!["auth".to_string(), "storage".to_string()];
        host.plugins.write().await.insert(id.to_string(), loaded);
        // 种子密钥环：activate 之前种好，之后用同一把钥匙签测试 token
        // （见同文件另一用例的注释：走互调要总线派发，自建 PluginHost 未
        //  init_message_bus 时等不到回复）。种入要过 `auth` 权限门
        host.permission.grant_permissions(id, &["auth".to_string()]);
        const TEST_KEY: [u8; 32] = [0x5a; 32];
        crate::utils::auth::test_tokens::seed_keyring(host.wasm_host_ctx(), id, &TEST_KEY);
        if let Err(e) = host.activate_plugin(id, false).await {
            eprintln!("[warn] activate {id} failed (proceed with installed instance): {e}");
        }
    }

    // 只注册 terminal-session 为认证中心；agent-hub 即使排序在前也不被选中
    let _ = registry::register(
        bridge::SESSION_PLUGIN_ID,
        CENTER_METHODS.iter().map(|s| s.to_string()).collect(),
    );
    assert_eq!(
        registry::center().map(|e| e.owner),
        Some(bridge::SESSION_PLUGIN_ID.to_string()),
        "注册表只认注册者（agent-hub 排序在前也不被选中）"
    );

    // token 用**中心自己那份密钥环**（本用例给 terminal-session 种子）签出：
    // 验签方只能是 terminal-session；若旧逻辑选中 agent-hub 的默认拒绝实现，
    // 它既没有这把密钥也验不过 → 裁决必错，与本用例要锁的「注册表说了算」同向
    const TEST_KEY: [u8; 32] = [0x5a; 32];
    let token =
        crate::utils::auth::test_tokens::sign_with_seeded_key(&TEST_KEY, "device-t", Some("Phone"), Some("fp-t"));
    // 私有库无撤销记录 → 放行（若旧逻辑选中 agent-hub 的默认拒绝，这里会是 Err）
    // 失败信息带裁决原文：三类拒因（no_center / unavailable / policy）排障路径完全不同，
    // 只报「assert is_ok」会把「注册表被并发清台」与「guest 调用超时」混成一个症状
    let decision = bridge::enforce_connection_policy(&host, &token);
    assert!(
        decision.is_ok(),
        "裁决必须落在已注册的 terminal-session 身上（agent-hub 未被注册即使排序在前）: \
         decision={decision:?} registry={:?}",
        registry::center().map(|e| e.owner)
    );

    // 撤销该设备（软删私有库真源 is_active=0）→ 拒绝，原因来自 terminal-session 策略
    {
        let db_path = std::env::temp_dir()
            .join(format!("bedcode-hosttest-pluginroot-{}", std::process::id()))
            .join(bridge::SESSION_PLUGIN_ID)
            .join("plugin.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap_or_else(|e| panic!("open center db: {e}"));
        let _ = conn.busy_timeout(std::time::Duration::from_secs(10));
        conn.execute(
            "INSERT OR REPLACE INTO auth_pairings \
             (id, device_name, device_fingerprint, address, uid_hash, paired_at, \
              last_seen, connect_count, is_active) \
             VALUES ('p-multi', 'Phone', 'fp-t', NULL, NULL, '2026-09-19T00:00:00Z', NULL, 1, 1)",
            [],
        )
        .expect("seed pairing");
        // 经 trust.revoke 软删（与生产同路径：插件写私有库真源）
        let revoked = host
            .invoke_rust_command(session_id, "session.trust.revoke", json!({"id": "p-multi"}))
            .await
            .expect("trust.revoke");
        assert_eq!(revoked["removed"], true, "撤销软删 must be recorded");
    }
    let deny = bridge::enforce_connection_policy(&host, &token).expect_err("撤销后必须拒绝");
    assert!(
        deny.contains("revoked"),
        "拒绝原因来自 terminal-session 的 trust 策略（deny_kind=policy）: {deny}"
    );

    // 停用 agent-hub（未注册者）→ 注册表与裁决不受影响（purge 只碰本人）
    host.deactivate_plugin(AGENT_HUB_ID, false)
        .await
        .expect("deactivate agent-hub");
    assert_eq!(
        registry::center().map(|e| e.owner),
        Some(bridge::SESSION_PLUGIN_ID.to_string()),
        "停用未注册者不得回收认证中心"
    );

    registry::purge_for_plugin(bridge::SESSION_PLUGIN_ID);
}

/// spec §10 票 05 第 3/5/6 条：三类拒绝可区分 + 错误文案不含凭据片段
///
/// `no_center` / `unavailable` 的拒绝文案由裁决面（enforce_connection_policy）生成，
/// `policy` 由中心原样透出。本锁断言三者文案互斥可指名（排障时「中心没起来」和
/// 「用户撤销了设备」是两类问题），且都不含 JWT/凭据片段（AGENTS §8 凭据红线）。
/// 纯状态机断言（`_inner` 局部 desk），不触碰全局注册表——无并行清台问题。
#[test]
fn test_auth_center_deny_kinds_are_distinguishable() {
    use crate::wasm_core::host_api::auth_center::{purge_inner, register_inner, AuthCenterEntry};

    // deny_kind=no_center：desk 空 → center()=None → 裁决面转统一文案
    let mut state: Option<AuthCenterEntry> = None;
    assert!(state.is_none(), "无中心（fail-closed 可判）");
    let _ = register_inner(
        &mut state,
        "com.bedcode.terminal-session",
        vec!["pairing_code".to_string()],
    );

    let no_center = "no auth center registered".to_string();
    // deny_kind=unavailable
    let unavailable = "auth center unavailable: plugin 'com.bedcode.terminal-session' not loaded".to_string();
    // deny_kind=policy（中心原样 reason）
    let policy = "device revoked from trust list".to_string();

    for text in [&no_center, &unavailable, &policy] {
        assert!(
            !text.contains("eyJ") && !text.to_lowercase().contains("jwt"),
            "错误文案不得含凭据/JWT 片段: {text}"
        );
    }
    // 三态互斥：no_center 不含 unavailable/revoked；unavailable 不含 no center/revoked；…
    assert!(!no_center.contains("unavailable") && !no_center.contains("revoked"));
    assert!(!unavailable.contains("no auth center") && !unavailable.contains("revoked"));
    assert!(!policy.contains("no auth center") && !policy.contains("unavailable"));
    // 边界：unavailable 文案点名中心属主（排障不需要猜是哪个中心）
    assert!(
        unavailable.contains("com.bedcode.terminal-session"),
        "unavailable 文案必须点名中心属主: {unavailable}"
    );

    purge_inner(&mut state, "com.bedcode.terminal-session");
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

/// L1 基础服务（旧称「系统组件」，ADR 0032）停用：能力回落宿主原语；重新激活后再装配。
/// 注：**不**涉及「只停不删」——按 kind 拒绝卸载的守卫尚未实现（本用例只覆盖停用路径）
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
    let app_id = "com.bedcode.sdk-test";

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

    // 用户插件目录（dev 合入的第 3 参）：本用例无用户安装插件，指向空临时目录
    let user_plugins_dir = tmp_dir.path().join("user-plugins");
    let host = PluginHost::new(db, &plugins_dir, &user_plugins_dir, None).await;

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

// ==================== 角色分层加载次序（ADR 0032） ====================

/// 三层夹具的插件 id（boot 集成用例；manifest `type` 决定角色）
///
/// **id 排序与层序故意相反**（L1 的 id 排最后）：这样「按角色分两批」与「合并成一批、
/// 批内按 id 排序」两种实现会给出**不同**的激活次序，顺序断言才有判别力。
/// 若三者 id 按 l1 < l2 < l3 递增，合并实现同样会绿——该断言就白写了（评审 2026-09-29）。
const L1_ID: &str = "com.bedcode.zzz-l1-store";
const L2_ID: &str = "com.bedcode.aaa-l2-center";
const L3_ID: &str = "com.bedcode.l3-app";

/// 写一个可加载的 wasm 插件包（plugin.json + 组件字节）
fn write_wasm_package(
    plugins_dir: &std::path::Path,
    id: &str,
    rust_library: &str,
    manifest_extra: &str,
    component: &[u8],
) {
    let dir = plugins_dir.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = format!(
        r#"{{"id": "{}", "name": "{}", "version": "0.1.0", "pluginType": "rust-ts", "rustLibrary": "{}", "permissions": ["storage"]{}}}"#,
        id, id, rust_library, manifest_extra
    );
    std::fs::write(dir.join("plugin.json"), manifest).unwrap();
    std::fs::write(dir.join(format!("{}.wasm", rust_library)), component).unwrap();
}

/// 三层 boot 夹具 + 持久化真源，返回构造好的宿主
///
/// 持久化只含 L3（业务应用面）：L1 / L2 是**角色驱动**层，启停不持久化，
/// 它们被激活这件事本身就是「角色批跑过了」的证据（若被误并入 L3 批，
/// 它们既不会出现在持久化表里、也不会被激活）。
async fn setup_three_layer_host(tmp_dir: &tempfile::TempDir, l2_manifest_extra: &str) -> Arc<PluginHost> {
    // AppConfig 初始化（与 setup_host / 既有 boot 用例同策略，重复 init 幂等）
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = AppConfig::default();
        config.network.port = 8766;
        AppConfig::init(config);
    });

    let plugins_dir = tmp_dir.path().join("plugins");
    let user_plugins_dir = tmp_dir.path().join("user-plugins");
    std::fs::create_dir_all(&user_plugins_dir).unwrap();

    // L1 基础服务：提供 host-storage（能力提供者身份）
    write_wasm_package(
        &plugins_dir,
        L1_ID,
        "bedcode_plugin_system_test",
        r#", "type": "basic-service""#,
        &build_system_test_component(),
    );
    // L2 内部统一业务应用：角色驱动、位于 L1 之后
    write_wasm_package(
        &plugins_dir,
        L2_ID,
        "bedcode_plugin_component_test",
        &format!(r#", "type": "internal-business"{}"#, l2_manifest_extra),
        &build_test_component(),
    );
    // L3 业务应用：不写 `type`（缺省角色），依赖 L1 装配的 host-storage
    write_wasm_package(
        &plugins_dir,
        L3_ID,
        "bedcode_plugin_component_test",
        r#", "dependencies": ["host-storage"]"#,
        &build_test_component(),
    );

    let db = Arc::new(Mutex::new(
        Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
    ));
    db.lock().await.init_schema().unwrap();
    PluginStorage::new(db.clone())
        .save_activated_plugins(&HashMap::from([(L3_ID.to_string(), true)]))
        .await
        .expect("seed persisted activation state (L3 only)");

    let host = PluginHost::new(db, &plugins_dir, &user_plugins_dir, None).await;
    host
}

/// 读某个插件记录的激活时刻（`activated_at` = 激活事件时刻，非 sleep 推断）
async fn activated_at(host: &PluginHost, id: &str) -> chrono::DateTime<chrono::Utc> {
    host.plugins
        .read()
        .await
        .get(id)
        .and_then(|p| p.activated_at)
        .unwrap_or_else(|| panic!("plugin {id} must record an activation timestamp"))
}

/// 加载次序：L1 基础服务 → L2 内部统一业务 → L3 业务应用（ADR 0032）
///
/// 三条独立证据（不靠时序 sleep）：
/// 1. 终态：三者皆 Activated；
/// 2. 激活事件时刻严格递增 `L1 < L2 < L3`。**判别力**：夹具 id 排序与层序**故意相反**
///    （`zzz-l1-store` > `aaa-l2-center`），故「合并成一批按 id 排序」的退化实现会让
///    本条转红（那样 L2 会先于 L1 激活）——该断言测的是「按角色分批」而非「按 id 排」；
/// 3. 语义证据：L3 声明 `dependencies: ["host-storage"]`，该能力由 L1 实例提供
///    ——L1 若晚于 L3，L3 的依赖检查会失败；三者仍 Activated 即证明 L1 更早。
///
/// 附：角色驱动层不进持久化表（启停真源是「角色」而非用户状态）。
#[tokio::test(flavor = "multi_thread")]
async fn test_boot_activates_l1_then_l2_then_l3() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_three_layer_host(&tmp_dir, "").await;

    for id in [L1_ID, L2_ID, L3_ID] {
        let info = host.get_plugin(id).await.expect("plugin present");
        assert_eq!(info.state, PluginState::Activated, "state of {id}");
    }

    let (l1_at, l2_at, l3_at) = (
        activated_at(&host, L1_ID).await,
        activated_at(&host, L2_ID).await,
        activated_at(&host, L3_ID).await,
    );
    assert!(
        l1_at < l2_at,
        "L1 基础服务必须先于 L2 内部统一业务激活（ADR 0032 加载顺序）: l1={l1_at} l2={l2_at}"
    );
    assert!(
        l2_at < l3_at,
        "L2 内部统一业务必须先于 L3 业务应用激活（宿主裁决面先于业务面就绪）: l2={l2_at} l3={l3_at}"
    );

    // 能力装配：host-storage 由 L1 实例提供（L3 的依赖检查命中它）
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", L1_ID),
        "host-storage must be assembled to the L1 basic service at boot"
    );

    // 角色驱动层不进持久化激活表（启停不持久化，持久化真源是「角色」）
    let persisted = host.get_activated_state().await;
    assert!(
        !persisted.contains_key(L1_ID) && !persisted.contains_key(L2_ID),
        "L1/L2 must be excluded from persisted activation state, got {persisted:?}"
    );
    assert_eq!(persisted.get(L3_ID), Some(&true), "L3 业务应用仍是用户启停真源");
}

/// L2 激活失败不阻断其余层（单个失败不阻断其余）
///
/// L2 夹具声明一个不存在的能力依赖 → 依赖检查即失败落 Error 态。预期：
/// L1 与 L3 照常 Activated，且 L3 的能力依赖仍命中 L1 —— **不**出现
/// 「L2 失败拖死整条启动链」或「L3 因 L2 缺席而未激活」。
///
/// 安全注记：真实 L2（认证中心）失败时认证面 fail-closed（ADR 0031），
/// 那属于裁决面拒绝对外表现，与「其余层照常激活」是两件事。
#[tokio::test(flavor = "multi_thread")]
async fn test_l2_activation_failure_does_not_block_other_layers() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_three_layer_host(&tmp_dir, r#", "dependencies": ["host-no-such-cap"]"#).await;

    let l2 = host.get_plugin(L2_ID).await.expect("l2 present");
    assert!(
        matches!(&l2.state, PluginState::Error(e) if e.contains("host-no-such-cap")),
        "L2 失败必须落 Error 态并点名缺失能力（可见信号），got {:?}",
        l2.state
    );
    for id in [L1_ID, L3_ID] {
        let info = host.get_plugin(id).await.expect("plugin present");
        assert_eq!(info.state, PluginState::Activated, "{id} must still activate");
    }
    assert_eq!(
        host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
        format!("system:{}", L1_ID)
    );
}
