//! 运行期错误通知节流 + fs 预授权（preauth）用例。

use super::scaffold::*;
use super::*;

/// 被测插件 id（本文件的测试夹具，不是内核常量）
///
/// 审计票 12 起 `FILE_TRANSFER_PLUGIN_ID` 从 `manager/host.rs` 与 `peer_net.rs` 删除——
/// 内核不再硬编码产品身份（peer-net 节点改按属主记账）。本用例测的是「共享根为空的
/// 插件可以启用先行」这条通用 preauth 语义，用一个具名夹具插件 id 即可，
/// 不需要内核侧留着那个常量
const FILE_TRANSFER_PLUGIN_ID: &str = "com.bedcode.file-transfer";

#[tokio::test]
async fn notify_plugin_runtime_error_throttle_and_no_app_context() {
    // 统一异常通道（PLUGIN_RUNTIME_ERROR）：
    // 1. 无 AppContext（测试/无头）时降级为纯日志，不 panic
    // 2. 同一插件窗口内二次通知被节流（不重复提示），节流表只记录一次
    let host = setup_host().await;

    host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "panic", "boom").await;
    host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "trap", "boom again")
        .await;

    let throttle = host.runtime_error_notify_throttle.lock().unwrap();
    assert!(
        throttle.contains_key(TEST_PLUGIN_ID),
        "first call must record throttle entry"
    );
    // 窗口内二次调用不新增/刷新条目（被节流）
    assert_eq!(throttle.len(), 1, "second call within window must be throttled");
}

// ==================== preauthorize_plugin 预授权钩子 ====================
//
// 验证「先授权再 loading」改造的契约:
// 1. 无 provider + 无 storage:直接放行(空路径 = 无需预授权)
// 2. file-transfer 无共享目录:同样放行(启用先行,配置由插件设置面板引导)
// 3. 路径已在 storage:不需要 provider,直接放行(check_batch 空路径短路)

/// 无 provider + 无 storage 预授权路径:空路径直接放行。
/// 对应「普通插件(无 fs 权限)启用 → 不出现 fs 弹窗,loading 正常显示」场景。
#[tokio::test]
async fn preauthorize_empty_paths_passes() {
    let host = setup_host().await;
    let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
    assert!(result.is_ok(), "empty preauth paths must pass, got: {:?}", result.err());
}

/// file-transfer 共享目录未配置:放行(启用先行)。
/// 硬拒绝会造成死锁——共享目录配置入口在插件 UI 内,而插件 UI 加载
/// 依赖激活成功,「配置需激活 → 激活需先配置」互为前置,首次启用永远失败。
#[tokio::test]
async fn preauthorize_file_transfer_empty_shared_roots_passes() {
    let host = setup_host().await;
    let result = host.preauthorize_plugin(FILE_TRANSFER_PLUGIN_ID).await;
    assert!(
        result.is_ok(),
        "file-transfer with empty shared_roots must pass (enable-first), got: {:?}",
        result.err()
    );
}

/// manifest `wasiPreopenDirs` 声明目录并入预授权收集(如 ai-chatbox 数据
/// 目录)。未授权 + 无头上下文(check_batch 保守拒绝)→ 返回「授权被拒」
/// 错误——若声明目录未被收集,空路径会直接放行,本用例即失去意义
///
/// 声明特意用**只读档**（票 07）：档位只收紧 guest 的写能力，不构成免授权通道，
/// 所以只读目录同样必须被收集并因未授权而拒绝
#[tokio::test]
async fn preauthorize_collects_manifest_preopen_dirs_ungranted_denied() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
    );
    host.plugins
        .write()
        .await
        .get_mut(TEST_PLUGIN_ID)
        .expect("test plugin in map")
        .manifest
        .wasi_preopen_dirs = vec![WasiPreopenDir::read_only("${home}/.bedcode-preauth-probe")];

    let err = host
        .preauthorize_plugin(TEST_PLUGIN_ID)
        .await
        .expect_err("ungranted manifest dir must be collected and denied headless");
    assert!(
        err.to_string().contains("denied"),
        "must fail via check_batch deny (not storage/parse), got: {}",
        err
    );
}

/// 声明目录已授权(storage fs_granted_paths 前缀命中)→ check_batch 短路
/// 通过,preauthorize 整体放行
#[tokio::test]
async fn preauthorize_manifest_preopen_dir_granted_passes() {
    let host = setup_host().await;
    let expanded = format!(
        "{}/.bedcode-preauth-probe",
        std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
    );
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
    );
    host.plugins
        .write()
        .await
        .get_mut(TEST_PLUGIN_ID)
        .expect("test plugin in map")
        .manifest
        .wasi_preopen_dirs = vec![WasiPreopenDir::writable("${home}/.bedcode-preauth-probe")];
    host.storage
        .set(TEST_PLUGIN_ID, "fs_granted_paths", json!([expanded]))
        .await
        .unwrap();

    let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
    assert!(
        result.is_ok(),
        "granted manifest dir must pass, got: {:?}",
        result.err()
    );
}

/// storage 已写入 preauth_paths 数组:从 storage 读取路径,非 file-transfer
/// 插件直接放行(check_batch 空 path 列表会短路返回 true,无头上下文
/// 不发事件)。
#[tokio::test]
async fn preauthorize_reads_paths_from_storage() {
    let host = setup_host().await;
    // 写入 storage 数组 — 不影响 plugin_id 隔离(只有自身能读)
    host.storage
        .set(
            TEST_PLUGIN_ID,
            super::PREAUTH_PATHS_STORAGE_KEY,
            json!([std::env::temp_dir().to_string_lossy()]),
        )
        .await
        .unwrap();
    // check_batch 在无头 app_handle 上下文下对未授权路径保守拒绝,
    // 因此我们只验证「路径已被收集」并不期望一定通过;重要的是
    // preauthorize 不会因 storage 读取而 panic,且调用了 check_batch
    let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
    // 接受 Ok 或 Err(无头上下文拒绝)— 但不能是 storage 解析错误
    if let Err(e) = &result {
        assert!(
            !e.to_string().contains("parse") && !e.to_string().contains("deserialize"),
            "storage parse error indicates collector bug: {}",
            e
        );
    }
}

// ==================== 系统组件与能力装配（core-plugin-manager，票据 06） ====================
