//! 快捷指令编排（票 02）：业务排序、一次性幂等迁移
//!
//! 业务规则权威在插件侧（本票从宿主下沉）：`sort_order` 升序（与 legacy 主库
//! `ORDER BY sort_order` 同语义，同名稳定）、迁移 marker 一次性语义。
//!
//! 迁移通道说明（spec 决策 6「不新增 host 原语、不触发 ABI bump」）：快捷指令
//! 没有像会话配置那样的 legacy 读取面（host-session 配置面只覆盖配置表），故迁移
//! 由**宿主侧 handoff**（`src-tauri/src/plugin/quick_actions_migration.rs`）读取
//! legacy 主库 `quick_actions` 行、经互调 api `quick-actions-import` 推送进来。
//! 本层只负责「收下并幂等落库」：marker 已存在 → 整体跳过（否则「插件侧删除 →
//! 下次 handoff 被 legacy 行复活」会让删除失效，与配置域迁移同一次护栏）。

use super::model::{QuickAction, QuickActionRow};
use super::store::{MIGRATION_MARKER, QuickActionStore};
use crate::config::model::now_rfc3339;

use serde::Serialize;

/// 迁移结果（宿主日志与闭环断言用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// marker 已在 → 本次未执行导入（一次性语义）
    pub already_migrated: bool,
    /// 实际新插入行数
    pub imported: usize,
    /// 已存在而跳过的行数（同 id）
    pub skipped_existing: usize,
}

/// 业务排序：`sort_order` 升序（与 legacy 主库 `ORDER BY sort_order` 同语义），
/// 同权重按 id 稳定（legacy 无二级序，SQLite 行序不保证，补 id 保证可断言）
pub fn sort_actions(actions: &mut [QuickAction]) {
    actions.sort_by_key(|a| (a.sort_order, a.id.clone()));
}

/// 全量列表（真源 + 业务排序；排序不在 SQL 侧做，与配置域同模式）
pub fn list(store: &impl QuickActionStore) -> Result<Vec<QuickAction>, String> {
    let mut actions = store.all()?;
    sort_actions(&mut actions);
    Ok(actions)
}

/// 一次性幂等导入：legacy 主库行（宿主 handoff 推送）→ 插件私有库
///
/// - marker 已存在 → 直接返回 `already_migrated`（不触碰任何行）
/// - 逐行 `INSERT OR REPLACE`（同 id 已存在 → 覆盖并计 `skipped_existing`，
///   与配置域迁移的「已存在跳过」同口径：handoff 幂等重试不重复计数）
/// - 校验：`id` / `name` / `content` 非空（legacy 主库 NOT NULL 约束的镜像；
///   迁移行缺字段即显性报错，不静默造空数据）
/// - 全部成功后再落 marker（任一行失败不落 marker → 下次 handoff 重试）
pub fn import(
    store: &impl QuickActionStore,
    rows: &[QuickActionRow],
) -> Result<ImportReport, String> {
    if store.marker(MIGRATION_MARKER)?.is_some() {
        return Ok(ImportReport {
            already_migrated: true,
            imported: 0,
            skipped_existing: 0,
        });
    }

    let mut imported = 0usize;
    let mut skipped_existing = 0usize;
    for row in rows {
        if row.id.trim().is_empty() {
            return Err("快捷指令迁移行 id 不能为空".to_string());
        }
        if row.name.trim().is_empty() {
            return Err(format!("快捷指令迁移行 name 不能为空: {}", row.id));
        }
        if row.content.trim().is_empty() {
            return Err(format!("快捷指令迁移行 content 不能为空: {}", row.id));
        }
        let action: QuickAction = row.clone().into();
        let exists = store
            .all()?
            .iter()
            .any(|a| a.id == action.id);
        store.put(&action)?;
        if exists {
            skipped_existing += 1;
        } else {
            imported += 1;
        }
    }
    store.set_marker(MIGRATION_MARKER, &now_rfc3339())?;
    Ok(ImportReport {
        already_migrated: false,
        imported,
        skipped_existing,
    })
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_actions::store::tests::MockQuickActionStore;

    fn row(id: &str, name: &str, sort_order: i64) -> QuickActionRow {
        QuickActionRow {
            id: id.to_string(),
            name: name.to_string(),
            content: format!("cmd-{id}"),
            icon: None,
            color: None,
            category: None,
            sort_order,
            created_at: "2026-09-20T00:00:00Z".to_string(),
        }
    }

    /// 排序与 legacy 主库 `ORDER BY sort_order` 同语义（含同权重稳定序）
    #[test]
    fn list_orders_by_sort_order_then_id() {
        let store = MockQuickActionStore::new(vec![
            QuickAction {
                id: "c".into(),
                name: "C".into(),
                content: "c".into(),
                icon: None,
                color: None,
                category: None,
                sort_order: 1,
                created_at: "2026-09-20T00:00:00Z".into(),
            },
            QuickAction {
                id: "a".into(),
                name: "A".into(),
                content: "a".into(),
                icon: None,
                color: None,
                category: None,
                sort_order: 0,
                created_at: "2026-09-20T00:00:00Z".into(),
            },
            QuickAction {
                id: "b".into(),
                name: "B".into(),
                content: "b".into(),
                icon: None,
                color: None,
                category: None,
                sort_order: 0,
                created_at: "2026-09-20T00:00:00Z".into(),
            },
        ]);
        let actions = list(&store).expect("list");
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"], "sort_order 升序 + 同权重 id 稳定");
    }

    /// 一次性导入：全部落库 + 字段零丢失 + marker 落库
    #[test]
    fn import_persists_all_rows_and_sets_marker() {
        let store = MockQuickActionStore::new(vec![]);
        let report = import(
            &store,
            &[
                row("qa-1", "部署", 1),
                QuickActionRow {
                    icon: Some("rocket".into()),
                    color: Some("#f00".into()),
                    category: Some("dev".into()),
                    ..row("qa-2", "构建", 0)
                },
            ],
        )
        .expect("import");
        assert_eq!(report.imported, 2);
        assert_eq!(report.skipped_existing, 0);
        assert!(!report.already_migrated);

        let actions = list(&store).expect("list after import");
        assert_eq!(actions.len(), 2);
        let qa2 = actions.iter().find(|a| a.id == "qa-2").expect("qa-2");
        assert_eq!(qa2.icon.as_deref(), Some("rocket"), "icon 零丢失");
        assert_eq!(qa2.color.as_deref(), Some("#f00"), "color 零丢失");
        assert_eq!(qa2.category.as_deref(), Some("dev"), "category 零丢失");
        assert_eq!(qa2.sort_order, 0);
        assert_eq!(qa2.created_at, "2026-09-20T00:00:00Z");

        assert!(
            store.marker(MIGRATION_MARKER).unwrap().is_some(),
            "marker 必须落库"
        );
    }

    /// 一次性护栏：marker 已在 → 再导入整体跳过（插件侧删除不被 legacy 复活）
    #[test]
    fn import_is_one_shot_when_marker_present() {
        let store = MockQuickActionStore::new(vec![]);
        import(&store, &[row("qa-1", "部署", 0)]).expect("first import");
        store.clear_rows(); // 模拟插件侧删除
        let report = import(&store, &[row("qa-1", "部署", 0)]).expect("second import");
        assert!(report.already_migrated, "marker 已在 → 不得再导入");
        assert_eq!(report.imported, 0);
        assert!(
            store.all_rows().is_empty(),
            "已迁移后重推不得复活被删行"
        );
    }

    /// 校验：缺 name / content 显性报错且不落 marker（下次 handoff 重试）
    #[test]
    fn import_rejects_invalid_rows_without_marker() {
        let store = MockQuickActionStore::new(vec![]);
        let err = import(&store, &[row("", "部署", 0)]).expect_err("空 id 必须报错");
        assert!(err.contains("id 不能为空"));
        let err = import(&store, &[row("qa-1", "  ", 0)]).expect_err("空 name 必须报错");
        assert!(err.contains("name 不能为空"));
        assert!(
            store.marker(MIGRATION_MARKER).unwrap().is_none(),
            "任一行失败不得落 marker（保证重试语义）"
        );
        assert!(store.all_rows().is_empty(), "失败路径不得留下半截数据");
    }

    /// 同 id 重复推送 → 覆盖计 skipped_existing（handoff 幂等重试不重复计数）
    #[test]
    fn import_overwrites_duplicate_ids_as_skipped() {
        let store = MockQuickActionStore::new(vec![]);
        import(&store, &[row("qa-1", "部署", 0)]).expect("first");
        // 清掉 marker 再推同 id 行（模拟「宿主未收到回执重试」的极端窗口）
        store.clear_markers();
        let report = import(&store, &[row("qa-1", "部署-改", 1)]).expect("retry");
        assert_eq!(report.imported, 0);
        assert_eq!(report.skipped_existing, 1, "同 id 覆盖计 skipped");
        let actions = list(&store).expect("list");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "部署-改", "同 id 覆盖以后写赢");
        assert_eq!(actions[0].sort_order, 1);
    }
}
