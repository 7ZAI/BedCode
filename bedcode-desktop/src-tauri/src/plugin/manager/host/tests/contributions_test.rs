//! manifest contributes 注册用例（commands / views / terminal / http endpoints / file handlers）。

use super::*;
use super::scaffold::*;


// ==================== Manifest Contributions ====================

#[tokio::test(flavor = "multi_thread")]
async fn test_register_manifest_contributions() {
    let host = setup_host().await;
    let mut plugin = make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded);
    plugin.manifest.contributes = PluginContributes {
        commands: vec![bedcode_plugin_api::CommandContribution {
            id: "test.hello".into(),
            title: "Hello".into(),
            icon: None,
        }],
        views: vec![bedcode_plugin_api::ViewContribution {
            id: "test.view".into(),
            view_type: "sidebar".into(),
            title: "V".into(),
            component: "View.vue".into(),
        }],
        ..Default::default()
    };
    host.plugins.write().await.insert(TEST_PLUGIN_ID.to_string(), plugin);

    host.register_manifest_contributions().await;

    let commands = host.registry().list_commands().await;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].plugin_id, TEST_PLUGIN_ID);
    let views = host.registry().list_views().await;
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].plugin_id, TEST_PLUGIN_ID);
    assert_eq!(views[0].view_type, "sidebar");
}

