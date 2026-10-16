//! 会话配置编排（票 08）：校验规则、业务排序、写入、一次性幂等迁移
//!
//! 业务规则权威在插件侧（本票从宿主下沉）：环境白名单、WSL 分支合法性、
//! 空命令兜底、业务排序。宿主 Rust 端仍保留 schema 级最终仲裁（主库投影的
//! NOT NULL / CHECK）——「双层校验」的第二层不因下沉而消失。
//!
//! **2026-09-22（v24）**：`LegacyConfigSource` / `migrate` 一次性迁移通道随
//! `session_configs` 表退役删除——宿主 host-session 配置读取面（config-list /
//! config-get）已删，主库不再持有配置历史；本插件私有库是会话配置唯一真源
//! （存量旧库遗留行按用户裁定直接丢弃，不等 legacy 观测归零）。

use super::model::{new_config_id, now_rfc3339, ConfigDraft, SessionConfig, VALID_ENVIRONMENTS};
use super::store::ConfigStore;

/// 业务排序：name 升序（大小写不敏感），同名按 id 稳定
pub fn sort_configs(configs: &mut [SessionConfig]) {
    configs.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// 全量列表（真源 + 业务排序；排序不在 SQL 侧做）
pub fn list(store: &impl ConfigStore) -> Result<Vec<SessionConfig>, String> {
    let mut configs = store.all()?;
    sort_configs(&mut configs);
    Ok(configs)
}

/// 读单条（空 id 显性报错；缺记录返回 None）
pub fn get(store: &impl ConfigStore, id: &str) -> Result<Option<SessionConfig>, String> {
    if id.trim().is_empty() {
        return Err("配置 id 不能为空".to_string());
    }
    store.get(id)
}

/// 写入：`id` 缺省/空 → 新建；命中 → 覆盖；非空未命中 → 显性报错
///
/// 覆盖时不重写 `created_at`（保留既有值），`updated_at` 刷新为当前时刻。
pub fn upsert(store: &impl ConfigStore, draft: &ConfigDraft) -> Result<SessionConfig, String> {
    let existing = match draft.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => Some(
            store
                .get(id)?
                .ok_or_else(|| format!("配置不存在：{}", id))?,
        ),
        None => None,
    };
    let now = now_rfc3339();
    let mut config = normalize(existing.as_ref(), draft, &now)?;
    if config.id.is_empty() {
        config.id = new_config_id()?;
    }
    store.put(&config)?;
    Ok(config)
}

/// 删除（返回是否命中；未知 id 幂等 false）
pub fn delete(store: &impl ConfigStore, id: &str) -> Result<bool, String> {
    if id.trim().is_empty() {
        return Err("配置 id 不能为空".to_string());
    }
    store.remove(id)
}

/// 归一化 + 校验：缺省字段回落既有值，返回可写入的完整配置
///
/// 校验规则（自宿主 `SessionConfigManager::validate_config` 与前端约定下沉并收紧）：
/// - `name` 非空（trim 后）
/// - `environment` ∈ [`VALID_ENVIRONMENTS`]（大小写归一化：`Linux` → `linux`，
///   避免主库投影的 CHECK 约束在写入时才失败）
/// - `environment = wsl2` ⇒ `wslDistro` 非空；非 wsl2 ⇒ 清空发行版（不留残留）
/// - 空 `command` ⇒ 按环境分支兜底默认 shell
fn normalize(
    existing: Option<&SessionConfig>,
    draft: &ConfigDraft,
    now: &str,
) -> Result<SessionConfig, String> {
    let pick = |draft_value: Option<&str>, existing_value: Option<&str>| -> String {
        draft_value
            .map(str::to_string)
            .unwrap_or_else(|| existing_value.unwrap_or_default().to_string())
    };

    let name = pick(draft.name.as_deref(), existing.map(|c| c.name.as_str()))
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("会话配置名称不能为空".to_string());
    }

    let environment = pick(
        draft.environment.as_deref(),
        existing.map(|c| c.environment.as_str()),
    )
    .trim()
    .to_lowercase();
    if !VALID_ENVIRONMENTS.contains(&environment.as_str()) {
        return Err(format!(
            "环境取值非法：{}（合法：{}）",
            environment,
            VALID_ENVIRONMENTS.join(" / ")
        ));
    }

    // 缺省回落既有值；显式空串表示清空（前端清空 WSL 发行版输入框的场景）
    let mut wsl_distro = match draft.wsl_distro.as_deref().map(str::trim) {
        Some("") => None,
        Some(value) => Some(value.to_string()),
        None => existing.and_then(|c| c.wsl_distro.clone()),
    };
    if environment == "wsl2" {
        if wsl_distro.as_deref().unwrap_or("").trim().is_empty() {
            return Err("WSL2 配置必须指定发行版（wslDistro）".to_string());
        }
    } else {
        wsl_distro = None;
    }

    let working_dir = pick(
        draft.working_dir.as_deref(),
        existing.map(|c| c.working_dir.as_str()),
    );
    let command_raw = pick(
        draft.command.as_deref(),
        existing.map(|c| c.command.as_str()),
    );
    let command = if command_raw.trim().is_empty() {
        ConfigDraft::default_command_for(&environment).to_string()
    } else {
        command_raw
    };
    let auto_start = draft
        .auto_start
        .or_else(|| existing.map(|c| c.auto_start))
        .unwrap_or(false);

    Ok(SessionConfig {
        id: draft
            .id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string(),
        name,
        environment,
        wsl_distro,
        working_dir,
        command,
        auto_start,
        created_at: existing
            .map(|c| c.created_at.clone())
            .unwrap_or_else(|| now.to_string()),
        updated_at: now.to_string(),
    })
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::store::tests::MockConfigStore;
    use std::sync::Mutex;

    fn draft(json: serde_json::Value) -> ConfigDraft {
        serde_json::from_value(json).expect("draft decode")
    }

    /// 新建：宿主生成 id 由调用方补；createdAt/updatedAt 落当前时刻
    #[test]
    fn upsert_creates_with_timestamps() {
        let store = MockConfigStore::new(vec![]);
        let written = upsert(
            &store,
            &draft(serde_json::json!({"name":"项目 A","environment":"linux","workingDir":"/srv/a","command":"bash"})),
        )
        .expect("upsert create");
        assert_eq!(written.id.len(), 36, "新建必须生成 UUID v4 形态 id");
        assert_eq!(written.created_at, written.updated_at);
        assert_eq!(store.all().unwrap().len(), 1);
    }

    /// 覆盖：缺省字段回落既有值，createdAt 保留、updatedAt 刷新
    #[test]
    fn upsert_update_keeps_unspecified_fields_and_created_at() {
        let store = MockConfigStore::new(vec![MockConfigStore::config("c1", "旧名")]);
        let written =
            upsert(&store, &draft(serde_json::json!({"id":"c1","name":"新名"}))).expect("update");
        assert_eq!(written.id, "c1");
        assert_eq!(written.name, "新名");
        assert_eq!(written.working_dir, "/srv/proj", "未声明字段回落既有值");
        assert_eq!(written.command, "bash", "未声明字段回落既有值");
        assert_eq!(
            written.created_at, "2026-09-01T00:00:00Z",
            "createdAt 不随覆盖变化"
        );
        assert_ne!(
            written.updated_at, "2026-09-01T00:00:00Z",
            "updatedAt 必须刷新"
        );
    }

    /// 未知 id：显性报错（不静默新建）
    #[test]
    fn upsert_unknown_id_is_explicit_error() {
        let store = MockConfigStore::new(vec![]);
        let err = upsert(
            &store,
            &draft(serde_json::json!({"id":"ghost","name":"n","environment":"linux"})),
        )
        .unwrap_err();
        assert!(err.contains("配置不存在：ghost"), "got: {err}");
        assert!(store.all().unwrap().is_empty(), "报错不得留下副作用");
    }

    /// 校验矩阵：名称空 / 环境非法 / wsl2 缺发行版 / 大小写归一化 / 非 wsl2 清残留
    #[test]
    fn normalize_validation_matrix() {
        let store = MockConfigStore::new(vec![]);

        let err = upsert(&store, &draft(serde_json::json!({"environment":"linux"}))).unwrap_err();
        assert!(err.contains("名称不能为空"), "got: {err}");

        let err = upsert(
            &store,
            &draft(serde_json::json!({"name":"n","environment":"darwin"})),
        )
        .unwrap_err();
        assert!(err.contains("环境取值非法：darwin"), "got: {err}");

        let err = upsert(
            &store,
            &draft(serde_json::json!({"name":"n","environment":"wsl2"})),
        )
        .unwrap_err();
        assert!(err.contains("必须指定发行版"), "got: {err}");

        // 大小写归一化（主库 CHECK 只认小写，归一化避免写入时才失败）
        let created = upsert(
            &store,
            &draft(serde_json::json!({"name":"n","environment":"Linux"})),
        )
        .expect("case-normalized");
        assert_eq!(created.environment, "linux");
        assert!(created.wsl_distro.is_none(), "非 wsl2 不留发行版");

        // wsl2 + 发行版：保留
        let wsl = upsert(
            &store,
            &draft(serde_json::json!({"name":"w","environment":"wsl2","wslDistro":"Ubuntu"})),
        )
        .expect("wsl2");
        assert_eq!(wsl.wsl_distro.as_deref(), Some("Ubuntu"));

        // 显式清空发行版（wsl2 → 非 wsl2 的编辑路径）后不带残留
        let after = upsert(
            &store,
            &draft(serde_json::json!({"id":wsl.id,"environment":"linux","wslDistro":""})),
        )
        .expect("clear distro");
        assert!(after.wsl_distro.is_none());
    }

    /// 空命令兜底按环境分支（业务规则从宿主下沉）
    #[test]
    fn normalize_falls_back_for_empty_command() {
        let store = MockConfigStore::new(vec![]);
        let linux = upsert(
            &store,
            &draft(serde_json::json!({"name":"l","environment":"linux","command":"  "})),
        )
        .expect("linux default");
        assert_eq!(linux.command, "bash");

        let win = upsert(
            &store,
            &draft(serde_json::json!({"name":"w","environment":"windows","command":""})),
        )
        .expect("windows default");
        assert_eq!(win.command, "powershell");
    }

    /// 业务排序在插件侧：name 大小写不敏感升序，同名按 id 稳定
    #[test]
    fn list_sorts_by_name_case_insensitively() {
        let store = MockConfigStore::new(vec![
            MockConfigStore::config("c3", "beta"),
            MockConfigStore::config("c1", "Alpha"),
            MockConfigStore::config("c2", "alpha"),
        ]);
        let names: Vec<String> = list(&store).unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec!["Alpha", "alpha", "beta"],
            "大小写不敏感 + 同名稳定"
        );
    }

    /// 删除：命中 true / 未知 id 幂等 false / 空 id 显性报错
    #[test]
    fn delete_reports_hit_and_rejects_empty_id() {
        let store = MockConfigStore::new(vec![MockConfigStore::config("c1", "A")]);
        assert!(delete(&store, "c1").unwrap());
        assert!(!delete(&store, "c1").unwrap());
        assert!(delete(&store, "  ").unwrap_err().contains("不能为空"));
        assert!(get(&store, "").unwrap_err().contains("不能为空"));
    }
}
