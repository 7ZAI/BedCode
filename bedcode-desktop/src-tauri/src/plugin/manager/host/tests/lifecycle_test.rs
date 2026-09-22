//! 非 WASM 插件的激活 / 停用 / 持久化 / 启动期自动激活用例。

use super::*;
use super::scaffold::*;

// ==================== 激活 / 停用（非 WASM 插件） ====================

#[tokio::test(flavor = "multi_thread")]
async fn test_activate_plugin_file_scan_flow() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
    );

    // 未注册插件 → Err
    let err = host.activate_plugin("com.missing", false).await.unwrap_err();
    assert!(err.to_string().contains("not found"));

    host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

    let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
    assert_eq!(info.state, PluginState::Activated);
    // 激活时间被记录
    {
        let plugins_guard = host.plugins.read().await;
        let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
        assert!(loaded.activated_at.is_some());
    }
    // 重新授权：manifest 声明的合法权限已授予（storage 恒默认授予）
    let granted = host.permission().get_granted(TEST_PLUGIN_ID);
    assert!(granted.contains("storage"));
    assert!(granted.contains("terminal:input"));
    assert!(host.permission().check(TEST_PLUGIN_ID, "terminal:input"));

    // 重复激活幂等（已激活 → Ok）
    host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
    assert_eq!(
        host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
        PluginState::Activated
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_activate_plugin_recovers_from_error_state() {
    let host = setup_host().await;
    // Error 态插件可重新激活（如 WASM 缺失被标记后修复文件再激活）
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(
            TEST_PLUGIN_ID,
            PluginSource::FileScan,
            PluginState::Error("wasm load failed".into()),
        ),
    );

    host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
    assert_eq!(
        host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
        PluginState::Activated
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_activate_plugin_persists_state() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
    );

    host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();

    let persisted = host.storage().load_activated_plugins().await.unwrap();
    assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_deactivate_plugin_flow() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
    );
    // 预注册一条命令贡献，验证停用时从 registry 摘除
    host.registry()
        .register_commands(
            TEST_PLUGIN_ID,
            &[bedcode_plugin_api::CommandContribution {
                id: "test.cmd".into(),
                title: "T".into(),
                icon: None,
            }],
        )
        .await;
    assert_eq!(host.registry().list_commands().await.len(), 1);

    host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

    let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
    assert_eq!(info.state, PluginState::Deactivated);
    assert!(!host.is_activated(TEST_PLUGIN_ID).await);
    // 激活时间被清除
    {
        let plugins_guard = host.plugins.read().await;
        let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
        assert!(loaded.activated_at.is_none());
    }
    // 权限被撤销（重新激活时重新授权）
    assert!(host.permission().get_granted(TEST_PLUGIN_ID).is_empty());
    // registry 贡献被摘除
    assert!(host.registry().list_commands().await.is_empty());

    // 未注册插件 → Err
    let err = host.deactivate_plugin("com.missing", false).await.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_deactivate_all() {
    let host = setup_host().await;
    for id in ["com.bedcode.a", "com.bedcode.b"] {
        host.plugins.write().await.insert(
            id.to_string(),
            make_plugin(id, PluginSource::FileScan, PluginState::Activated),
        );
    }

    host.deactivate_all().await.unwrap();

    for id in ["com.bedcode.a", "com.bedcode.b"] {
        assert_eq!(host.get_plugin(id).await.unwrap().state, PluginState::Deactivated);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_auto_activate_from_persisted_state() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
    );

    // 持久化激活 → 手动停用（不持久化）→ 从持久化状态恢复激活
    host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();
    host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
    assert!(!host.is_activated(TEST_PLUGIN_ID).await);

    host.auto_activate_from_persisted_state().await;
    assert!(host.is_activated(TEST_PLUGIN_ID).await);

    // 幽灵 ID 清理：持久化映射中不存在的插件被剔除（map 整体替换语义：
    // 重新写入时保留现有条目再插入幽灵 ID）
    let mut stale = HashMap::new();
    stale.insert("com.ghost".to_string(), true);
    stale.insert(TEST_PLUGIN_ID.to_string(), true);
    host.storage().save_activated_plugins(&stale).await.unwrap();
    host.auto_activate_from_persisted_state().await;
    let persisted = host.storage().load_activated_plugins().await.unwrap();
    assert!(!persisted.contains_key("com.ghost"));
    // 已存在的插件条目保留
    assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
}

