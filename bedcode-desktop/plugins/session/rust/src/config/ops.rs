//! 会话配置编排（票 08）：校验规则、业务排序、写入、一次性幂等迁移
//!
//! 业务规则权威在插件侧（本票从宿主下沉）：环境白名单、WSL 分支合法性、
//! 空命令兜底、业务排序。宿主 Rust 端仍保留 schema 级最终仲裁（主库投影的
//! NOT NULL / CHECK）——「双层校验」的第二层不因下沉而消失。
//!
//! 迁移的两个性质都要成立（票面要求）：
//! - **一次性**：私有库 marker（`plugin_meta` 的 [`MIGRATION_MARKER`]）存在即不重跑。
//!   否则「插件侧删掉某配置 → 下次激活被 legacy 行重新导入」会让删除失效。
//! - **幂等**：marker 之外仍逐条按 id 判存在（`INSERT OR REPLACE` 不会产生重复行），
//!   marker 写入失败/进程崩溃后重跑同样收敛。

use super::model::{new_config_id, now_rfc3339, ConfigDraft, SessionConfig, VALID_ENVIRONMENTS};
use super::store::{ConfigStore, MIGRATION_MARKER};

/// 旧数据来源（迁移用）：wasm = host-session 配置面（票 07）；native 单测注入 mock
///
/// **为什么经宿主原语读**：插件无法直接 SQL 主库（主库表名强制 `plugin_<id>_`
/// 前缀，`host_impl/database.rs` 的前缀校验会拒），`host-session.config-list` /
/// `config-get` 是插件访问 legacy 配置的唯一合法通道——这也是票 07 配置面在票 08
/// 之后仍然有消费者的原因（迁移期读取 + 宿主降级轨）。
pub trait LegacyConfigSource {
    /// legacy 主库配置 id 清单
    fn legacy_ids(&self) -> Result<Vec<String>, String>;
    /// 单条 legacy 配置（全量字段；不存在返回 None）
    fn legacy_get(&self, id: &str) -> Result<Option<SessionConfig>, String>;
}

// ==================== wasm：legacy 来源实现（host-session 配置面） ====================

#[cfg(target_arch = "wasm32")]
mod wasm_legacy {
    use super::*;
    use bedcode_plugin_api::host::HostSession;
    use bedcode_plugin_api::wasm_host::WasmHost;

    impl LegacyConfigSource for WasmHost {
        fn legacy_ids(&self) -> Result<Vec<String>, String> {
            // config-list 是精简列表（id/name/workingDir/command）——迁移只需 id
            let list = self
                .session_config_list()
                .map_err(|e| format!("host-session config-list failed: {}", e.message))?;
            let rows = list.unwrap_or_else(|| serde_json::json!([]));
            Ok(rows
                .as_array()
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|row| {
                            row.get("id").and_then(|v| v.as_str()).map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default())
        }

        fn legacy_get(&self, id: &str) -> Result<Option<SessionConfig>, String> {
            let row = self
                .session_config_get(id)
                .map_err(|e| format!("host-session config-get failed: {}", e.message))?;
            match row {
                Some(value) => serde_json::from_value(value)
                    .map(Some)
                    .map_err(|e| format!("legacy config decode failed: {}", e)),
                None => Ok(None),
            }
        }
    }
}

/// 迁移结果（宿主日志与闭环断言用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// marker 已在 → 本次未执行迁移（一次性语义）
    pub already_migrated: bool,
    pub imported: usize,
    /// 目标私有库已有同 id 行 → 跳过（幂等）
    pub skipped_existing: usize,
}

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

/// 一次性幂等迁移：legacy 主库 → 插件私有库
pub fn migrate(
    store: &impl ConfigStore,
    legacy: &impl LegacyConfigSource,
) -> Result<MigrationReport, String> {
    if store.marker(MIGRATION_MARKER)?.is_some() {
        return Ok(MigrationReport {
            already_migrated: true,
            imported: 0,
            skipped_existing: 0,
        });
    }

    let ids = legacy.legacy_ids()?;
    let mut imported = 0usize;
    let mut skipped_existing = 0usize;
    for id in ids {
        if store.get(&id)?.is_some() {
            skipped_existing += 1;
            continue;
        }
        // 读出时为 None（迁移期间被删除）→ 跳过而非报错：迁移不该因竞态中断
        let Some(config) = legacy.legacy_get(&id)? else {
            continue;
        };
        store.put(&config)?;
        imported += 1;
    }
    store.set_marker(MIGRATION_MARKER, &now_rfc3339())?;
    Ok(MigrationReport {
        already_migrated: false,
        imported,
        skipped_existing,
    })
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

    /// legacy 主库 mock（native 单测注入）
    pub(crate) struct MockLegacy {
        rows: Mutex<Vec<SessionConfig>>,
        /// 模拟「id 清单里有、单行已消失」的竞态
        pub drop_ids: Mutex<Vec<String>>,
    }

    impl MockLegacy {
        pub(crate) fn new(rows: Vec<SessionConfig>) -> Self {
            Self {
                rows: Mutex::new(rows),
                drop_ids: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn push(&self, config: SessionConfig) {
            self.rows.lock().unwrap().push(config);
        }
    }

    impl LegacyConfigSource for MockLegacy {
        fn legacy_ids(&self) -> Result<Vec<String>, String> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .map(|c| c.id.clone())
                .collect())
        }

        fn legacy_get(&self, id: &str) -> Result<Option<SessionConfig>, String> {
            if self.drop_ids.lock().unwrap().iter().any(|d| d == id) {
                return Ok(None);
            }
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
    }

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

    /// 迁移幂等（票面硬要求）：同一条 legacy 只导入一次；重复执行不产生重复行；
    /// 已存在的同 id 行跳过而非覆盖
    #[test]
    fn migrate_is_idempotent_and_repeatable() {
        let store = MockConfigStore::new(vec![MockConfigStore::config("existing", "已存在")]);
        let legacy = MockLegacy::new(vec![
            MockConfigStore::config("existing", "legacy 名字（不得覆盖私有库）"),
            MockConfigStore::config("legacy-1", "从主库迁入"),
        ]);

        let first = migrate(&store, &legacy).expect("first migrate");
        assert_eq!(
            first,
            MigrationReport {
                already_migrated: false,
                imported: 1,
                skipped_existing: 1
            }
        );
        let rows = store.all().unwrap();
        assert_eq!(rows.len(), 2, "不产生重复行");
        assert_eq!(
            rows.iter().find(|c| c.id == "existing").unwrap().name,
            "已存在",
            "私有库既有行不被 legacy 覆盖（私有库才是真源）"
        );

        // 第二次执行：marker 已在 → 不写任何行
        let puts_before = store.put_count();
        let second = migrate(&store, &legacy).expect("second migrate");
        assert!(second.already_migrated, "marker 存在即不重跑");
        assert_eq!(store.put_count(), puts_before, "重复迁移不得写库");
        assert_eq!(store.all().unwrap().len(), 2);

        // 一次性语义的护栏：marker 之后 legacy 新增行也不被导入
        // （否则「插件侧删除配置」会被下次激活重新导入 → 删除失效）
        legacy.push(MockConfigStore::config("legacy-2", "迁移后新增"));
        let third = migrate(&store, &legacy).expect("third migrate");
        assert!(third.already_migrated);
        assert_eq!(
            store.all().unwrap().len(),
            2,
            "迁移后 legacy 新行不得被导入"
        );
    }

    /// 迁移逐字段搬运：legacy 的时间戳与 autoStart 原样保留（不刷新为迁移时刻）
    #[test]
    fn migrate_preserves_legacy_fields() {
        let store = MockConfigStore::new(vec![]);
        let mut legacy_row = MockConfigStore::config("legacy-1", "旧配置");
        legacy_row.auto_start = true;
        legacy_row.created_at = "2026-08-01T00:00:00Z".to_string();
        legacy_row.updated_at = "2026-08-02T00:00:00Z".to_string();
        legacy_row.environment = "wsl2".to_string();
        legacy_row.wsl_distro = Some("Ubuntu".to_string());
        let legacy = MockLegacy::new(vec![legacy_row]);

        migrate(&store, &legacy).expect("migrate");
        let imported = store.get("legacy-1").unwrap().expect("imported");
        assert_eq!(imported.created_at, "2026-08-01T00:00:00Z");
        assert_eq!(imported.updated_at, "2026-08-02T00:00:00Z");
        assert!(imported.auto_start);
        assert_eq!(imported.wsl_distro.as_deref(), Some("Ubuntu"));
    }

    /// 迁移竞态：id 清单里有、单行读取为空 → 跳过且不报错（迁移不因竞态中断）
    #[test]
    fn migrate_skips_rows_that_vanished() {
        let store = MockConfigStore::new(vec![]);
        let legacy = MockLegacy::new(vec![MockConfigStore::config("gone", "已消失")]);
        legacy.drop_ids.lock().unwrap().push("gone".to_string());
        let report = migrate(&store, &legacy).expect("migrate tolerates race");
        assert_eq!(report.imported, 0);
        assert!(store.all().unwrap().is_empty());
        assert!(
            store.marker(MIGRATION_MARKER).unwrap().is_some(),
            "marker 仍须落库"
        );
    }
}
