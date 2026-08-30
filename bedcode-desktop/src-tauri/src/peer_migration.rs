//! 旧版对等网络数据一次性迁移（issue 13 Phase 4 步骤 9）
//!
//! 引擎侧 `transfer_settings.json` / `shared_dirs.json` → file-transfer 插件
//! 存储键（`transfer_settings` / `shared_roots`）。幂等：插件侧键已存在即跳过
//! （键的存在性即版本戳，防重复导入）；插件历史不迁移（旧历史在引擎侧，
//! 按裁决 B 停写保留一版只读兼容回滚）。
//!
//! 兼容性约定：共享根 id 与插件的 FNV-1a 内容哈希算法一致（`root-<hex16>`），
//! 用户对同一目录的重复添加天然去重。本模块计划随引擎停写收尾一并删除。

use crate::plugin::storage::PluginStorage;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

/// file-transfer 插件 ID（双端一致）
const PLUGIN_ID: &str = "com.bedcode.file-transfer";

/// FNV-1a 64-bit —— 与插件 roots_registry::fnv1a 算法保持一致（勿单方面改动）
fn fnv1a(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// 迁移入口（setup 阶段调用一次；任何失败仅记日志不阻断启动——迁移是
/// best-effort 兼容路径，用户始终可在插件设置里手动重建）
pub(crate) fn migrate_legacy_peer_data(app: &tauri::AppHandle, db: &Arc<tauri::async_runtime::Mutex<crate::db::Database>>) {
    let storage = Arc::new(PluginStorage::new(db.clone()));
    let dir = match crate::peer_net::app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::debug!("peer data migration skipped (no app dir): {e}");
            return;
        }
    };

    if let Err(e) = migrate_settings(&storage, &dir) {
        tracing::warn!(error = %e, "transfer settings migration failed");
    }
    if let Err(e) = migrate_shared_roots(&storage, &dir) {
        tracing::warn!(error = %e, "shared roots migration failed");
    }
}

/// 接收策略/落点/加密：引擎 transfer_settings.json → 插件 `transfer_settings`
/// （wire 形状转换为插件 SettingsPanel 词表：always_accept → accept 等）
fn migrate_settings(storage: &PluginStorage, dir: &PathBuf) -> anyhow::Result<()> {
    const KEY: &str = "transfer_settings";
    if block_on(storage.get(PLUGIN_ID, KEY))?.is_some() {
        return Ok(());
    }
    let raw = match std::fs::read_to_string(dir.join("transfer_settings.json")) {
        Ok(raw) => raw,
        Err(_) => return Ok(()), // 无旧文件 = 首装，无需迁移
    };
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Legacy {
        #[serde(default)]
        settings: Option<LegacySettings>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacySettings {
        #[serde(default)]
        policy_mode: String,
        #[serde(default)]
        ask_timeout_secs: u64,
        #[serde(default)]
        download_dir: Option<String>,
        #[serde(default)]
        encryption_enabled: bool,
    }
    let file: Legacy = serde_json::from_str(&raw)?;
    let Some(s) = file.settings else {
        return Ok(());
    };
    let receiving_policy = match s.policy_mode.as_str() {
        "always_accept" => "accept",
        "always_deny" => "reject",
        _ => "ask",
    };
    let exported = json!({
        "receivingPolicy": receiving_policy,
        "approvalTimeoutSec": s.ask_timeout_secs.clamp(10, 600),
        "downloadDir": s.download_dir,
        "encryption": s.encryption_enabled,
    });
    block_on(storage.set(PLUGIN_ID, KEY, exported))?;
    tracing::info!("legacy transfer settings migrated to plugin storage");
    Ok(())
}

/// 共享根注册表：引擎 shared_dirs.json → 插件 `shared_roots`
/// （id 以内容哈希重算与插件算法对齐；内置 local-downloads 不迁移）
fn migrate_shared_roots(storage: &PluginStorage, dir: &PathBuf) -> anyhow::Result<()> {
    const KEY: &str = "shared_roots";
    if block_on(storage.get(PLUGIN_ID, KEY))?.is_some() {
        return Ok(());
    }
    let raw = match std::fs::read_to_string(dir.join("shared_dirs.json")) {
        Ok(raw) => raw,
        Err(_) => return Ok(()),
    };
    #[derive(serde::Deserialize)]
    struct LegacyFile {
        #[serde(default)]
        dirs: Vec<LegacyDir>,
    }
    #[derive(serde::Deserialize)]
    struct LegacyDir {
        name: String,
        root: SharedRootKind,
    }
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum SharedRootKind {
        Fs { path: PathBuf },
        Saf { tree_uri: String },
    }
    let file: LegacyFile = serde_json::from_str(&raw)?;
    let entries: Vec<serde_json::Value> = file
        .dirs
        .into_iter()
        .filter_map(|d| match d.root {
            SharedRootKind::Fs { path } => {
                let p = path.to_string_lossy().to_string();
                Some(json!({
                    "id": format!("root-{:016x}", fnv1a(&p)),
                    "name": d.name,
                    "path": p,
                }))
            }
            SharedRootKind::Saf { tree_uri } => {
                Some(json!({
                    "id": format!("root-{:016x}", fnv1a(&tree_uri)),
                    "name": d.name,
                    "path": tree_uri,
                }))
            }
        })
        .collect();
    if entries.is_empty() {
        return Ok(());
    }
    block_on(storage.set(PLUGIN_ID, KEY, json!(entries)))?;
    tracing::info!(count = entries.len(), "legacy shared roots migrated to plugin storage");
    Ok(())
}

/// 同步上下文里的异步等待辅助（迁移在 setup 同步段执行）
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tauri::async_runtime::block_on(fut)
}
