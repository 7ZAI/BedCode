//! 插件权限审批与内容钉扎
//!
//! 插件权限审批与内容钉扎（content pinning）：
//!
//! - 权限门禁：生效权限 = 用户批准的权限 ∩ manifest 请求的权限
//!   （`effective_permissions`），杜绝「manifest 声明即信任」的自动全量授权
//! - 哈希钉扎：批准时对插件目录全部文件计算 SHA-256，激活时重算校验。
//!   插件文件在批准后被替换（冒名顶替的在位攻击）→ 哈希不匹配 →
//!   批准自动撤销，必须重新人工审批
//!
//! 信任边界：内置插件（桌面 resources 随包 / 移动 APK assets）属于
//! 应用构建信任域，视为已批准（`trusted=true` 直接放行）；
//! 用户安装的插件必须经过本模块审批后才能激活。
//!
//! 持久化：`plugin_storage` 表 `__system__` 空间 `plugin_approvals` key，
//! 与激活状态持久化（storage.rs ACTIVATION_STATE_KEY）同一模式。

use crate::plugin::storage::PluginStorage;
use crate::AppError;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// 审批记录存储 key（系统级 plugin_id 空间下）
pub const APPROVAL_STORAGE_KEY: &str = "plugin_approvals";

/// 系统级 plugin_id（与 storage.rs SYSTEM_PLUGIN_ID 同值，避免数据混入插件空间）
const SYSTEM_PLUGIN_ID: &str = "__system__";

/// 单条插件审批记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginApproval {
    /// 用户批准时同意的权限列表（生效权限 = 该列表 ∩ manifest 请求）
    pub approved_permissions: Vec<String>,
    /// 批准时插件目录内容 SHA-256（激活时校验，防批准后替换）
    pub content_hash: String,
    /// 批准时的插件版本
    pub version: String,
    /// 批准时间（RFC3339）
    pub approved_at: String,
}

/// 审批校验结果
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalStatus {
    /// 已批准且内容哈希一致
    Approved,
    /// 无审批记录（或权限集合为空）
    Pending,
    /// 有审批记录但内容哈希不匹配（文件被替换，需重新批准）
    HashMismatch,
}

/// 审批存储：基于 PluginStorage 的 JSON map（plugin_id → PluginApproval）
pub struct PluginApprovalStore {
    storage: Arc<PluginStorage>,
}

impl PluginApprovalStore {
    pub fn new(storage: Arc<PluginStorage>) -> Self {
        Self { storage }
    }

    /// 加载全部审批记录（无记录/损坏时返回空 map）
    pub async fn load_all(&self) -> crate::Result<HashMap<String, PluginApproval>> {
        match self.storage.get(SYSTEM_PLUGIN_ID, APPROVAL_STORAGE_KEY).await? {
            Some(value) => serde_json::from_value(value).map_err(|e| {
                tracing::warn!("Failed to parse plugin approvals, resetting: {}", e);
                AppError::Plugin(format!("Invalid plugin approvals: {}", e))
            }),
            None => Ok(HashMap::new()),
        }
    }

    /// 保存全部审批记录
    pub async fn save_all(&self, map: &HashMap<String, PluginApproval>) -> crate::Result<()> {
        let value = serde_json::to_value(map)?;
        self.storage.set(SYSTEM_PLUGIN_ID, APPROVAL_STORAGE_KEY, value).await
    }

    /// 读取单个插件审批记录
    pub async fn get(&self, plugin_id: &str) -> crate::Result<Option<PluginApproval>> {
        Ok(self.load_all().await?.remove(plugin_id))
    }

    /// 记录/更新审批（覆盖式：以本次批准的权限集合为准）
    pub async fn approve(
        &self,
        plugin_id: &str,
        approved_permissions: &[String],
        content_hash: &str,
        version: &str,
    ) -> crate::Result<()> {
        let mut map = self.load_all().await?;
        map.insert(
            plugin_id.to_string(),
            PluginApproval {
                approved_permissions: approved_permissions.to_vec(),
                content_hash: content_hash.to_string(),
                version: version.to_string(),
                approved_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        self.save_all(&map).await
    }

    /// 撤销审批（哈希不匹配 / 卸载时调用）
    pub async fn revoke(&self, plugin_id: &str) -> crate::Result<()> {
        let mut map = self.load_all().await?;
        if map.remove(plugin_id).is_some() {
            self.save_all(&map).await?;
        }
        Ok(())
    }
}

/// 计算插件目录内容 SHA-256（相对路径排序 + 文件内容）
///
/// 覆盖目录下全部文件（含 plugin.json / wasm / js 产物），
/// 任一文件被替换都会导致哈希变化。
pub fn compute_dir_hash(dir: &Path) -> crate::Result<String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    fn collect(dir: &Path, base: &Path, files: &mut Vec<(String, Vec<u8>)>) -> crate::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .map_err(|e| AppError::Plugin(format!("Hash path strip failed: {}", e)))?
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                collect(&path, base, files)?;
            } else if path.is_file() {
                let content = std::fs::read(&path).map_err(|e| {
                    AppError::Plugin(format!("Failed to read '{}' for hashing: {}", rel, e))
                })?;
                files.push((rel, content));
            }
        }
        Ok(())
    }
    collect(dir, dir, &mut files)?;

    // 相对路径排序，保证遍历顺序稳定（文件系统枚举顺序不保证）
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, content) in &files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(content);
        hasher.update([0u8]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 计算生效权限集：用户批准 ∩ manifest 请求（storage 恒授予）
///
/// `trusted=true`（内置插件）时直接全量返回请求权限。
pub fn effective_permissions(
    requested: &[String],
    approval: Option<&PluginApproval>,
    trusted: bool,
) -> HashSet<String> {
    let mut effective: HashSet<String> = if trusted {
        requested.iter().cloned().collect()
    } else {
        match approval {
            Some(appr) => {
                let approved: HashSet<&str> =
                    appr.approved_permissions.iter().map(|s| s.as_str()).collect();
                requested
                    .iter()
                    .filter(|p| approved.contains(p.as_str()))
                    .cloned()
                    .collect()
            }
            None => HashSet::new(),
        }
    };
    // storage 恒授予：插件自身配置空间的读写（与 PermissionManager 语义一致）
    effective.insert(crate::plugin::permission::PERMISSION_STORAGE.to_string());
    effective
}

/// 校验审批状态：哈希钉扎检查
///
/// 返回 (status, 当前目录哈希)。Pending / HashMismatch 均表示
/// 插件不可按既有审批激活，调用方应要求重新人工批准。
pub fn verify_approval(
    approval: Option<&PluginApproval>,
    dir: &Path,
) -> crate::Result<(ApprovalStatus, String)> {
    let current_hash = compute_dir_hash(dir)?;
    let status = match approval {
        None => ApprovalStatus::Pending,
        Some(appr) => {
            if appr.content_hash == current_hash {
                ApprovalStatus::Approved
            } else {
                ApprovalStatus::HashMismatch
            }
        }
    };
    Ok((status, current_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    async fn test_store() -> PluginApprovalStore {
        let db = Database::new(&std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();
        PluginApprovalStore::new(Arc::new(PluginStorage::new(Arc::new(Mutex::new(db)))))
    }

    fn write_plugin_dir(dir: &Path, files: &[(&str, &str)]) {
        std::fs::create_dir_all(dir).unwrap();
        for (name, content) in files {
            let p = dir.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }
    }

    #[tokio::test]
    async fn test_approve_roundtrip_and_revoke() {
        let store = test_store().await;
        assert!(store.get("com.test.p").await.unwrap().is_none());

        store.approve("com.test.p", &["fs:read".to_string()], "abc123", "1.0.0").await.unwrap();
        let approval = store.get("com.test.p").await.unwrap().expect("approved");
        assert_eq!(approval.approved_permissions, vec!["fs:read"]);
        assert_eq!(approval.content_hash, "abc123");
        assert_eq!(approval.version, "1.0.0");

        store.revoke("com.test.p").await.unwrap();
        assert!(store.get("com.test.p").await.unwrap().is_none());
    }

    #[test]
    fn test_compute_dir_hash_stable_and_sensitive() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("p");
        write_plugin_dir(&dir, &[
            ("plugin.json", r#"{"id":"com.test.p"}"#),
            ("dist/main.js", "console.log(1)"),
            ("icon.png", "PNG-DATA"),
        ]);

        let h1 = compute_dir_hash(&dir).unwrap();
        let h2 = compute_dir_hash(&dir).unwrap();
        assert_eq!(h1, h2, "同内容哈希必须稳定");

        // 修改任意文件 → 哈希变化（钉扎生效）
        std::fs::write(dir.join("dist/main.js"), "console.log(2)").unwrap();
        let h3 = compute_dir_hash(&dir).unwrap();
        assert_ne!(h1, h3, "文件替换后哈希必须变化");

        // 新增文件 → 哈希变化
        std::fs::write(dir.join("extra.bin"), "x").unwrap();
        let h4 = compute_dir_hash(&dir).unwrap();
        assert_ne!(h3, h4, "新增文件后哈希必须变化");
    }

    #[test]
    fn test_effective_permissions_gating() {
        let requested = vec![
            "fs:read".to_string(),
            "process:run".to_string(),
            "storage".to_string(),
        ];

        // 未批准：仅 storage
        let eff = effective_permissions(&requested, None, false);
        assert!(eff.contains("storage"));
        assert!(!eff.contains("fs:read"));
        assert!(!eff.contains("process:run"));

        // 批准子集：批准 ∩ 请求，storage 恒有
        let approval = PluginApproval {
            approved_permissions: vec!["fs:read".to_string()],
            content_hash: "h".to_string(),
            version: "1.0.0".to_string(),
            approved_at: "now".to_string(),
        };
        let eff = effective_permissions(&requested, Some(&approval), false);
        assert!(eff.contains("storage"));
        assert!(eff.contains("fs:read"));
        assert!(!eff.contains("process:run"), "未批准的 process:run 不得授予");

        // 批准了请求里没有的权限：不生效（交集语义）
        let approval_extra = PluginApproval {
            approved_permissions: vec!["fs:write".to_string()],
            content_hash: "h".to_string(),
            version: "1.0.0".to_string(),
            approved_at: "now".to_string(),
        };
        let eff = effective_permissions(&requested, Some(&approval_extra), false);
        assert!(!eff.contains("fs:write"));

        // 内置可信：全量
        let eff = effective_permissions(&requested, None, true);
        assert!(eff.contains("process:run"));
        assert!(eff.contains("fs:read"));
    }

    #[test]
    fn test_verify_approval_status() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("p");
        write_plugin_dir(&dir, &[("plugin.json", r#"{"id":"com.test.p"}"#)]);

        // 无审批 → Pending
        let (status, hash) = verify_approval(None, &dir).unwrap();
        assert_eq!(status, ApprovalStatus::Pending);
        assert!(!hash.is_empty());

        // 审批哈希一致 → Approved
        let approval = PluginApproval {
            approved_permissions: vec![],
            content_hash: hash.clone(),
            version: "1.0.0".to_string(),
            approved_at: "now".to_string(),
        };
        let (status, _) = verify_approval(Some(&approval), &dir).unwrap();
        assert_eq!(status, ApprovalStatus::Approved);

        // 文件被替换 → HashMismatch
        std::fs::write(dir.join("plugin.json"), r#"{"id":"com.test.p","name":"evil"}"#).unwrap();
        let (status, _) = verify_approval(Some(&approval), &dir).unwrap();
        assert_eq!(status, ApprovalStatus::HashMismatch);
    }
}
