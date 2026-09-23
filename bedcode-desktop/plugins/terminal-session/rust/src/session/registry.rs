//! 会话登记注册表：内存态 + 私有库写穿（会话引擎整体下沉 P1）
//!
//! 不变量（单一真源 = 私有库，内存态是它的镜像）：
//! 1. **写库先行**：任何变更先落私有库再改内存态——写库失败时缓存保持原样，
//!    不会出现「内存说有、库里没有」的幽灵条目；
//! 2. **懒加载**：首次读时从私有库整表载入一次（`None` = 未载入），之后的读走内存态；
//! 3. **读序稳定**：内存态是 `HashMap`（迭代序不确定），对外一律按
//!    `created_at, id` 排序——宿主 `SessionManager` 的迭代序不确定，本域不复制该缺陷；
//! 4. **非法迁移显性报错**：状态机拒绝的迁移返回 `Err`（调用方 warn 留痕），
//!    绝不静默改成「就近合法值」。
//!
//! 本类型不持全局单例：wasm 门面持一份进程静态实例（[`super::REGISTRY`]），
//! native 单测各自新建实例，互不串状态。

use super::model::{SessionRecord, SessionStatus};
use super::ops;
use super::store::SessionStore;
use crate::actions::RendererSource;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

/// 会话登记注册表（内存态 = 私有库的镜像）
pub struct Registry {
    /// `None` = 尚未从私有库载入；`Some` = 已载入的镜像
    cache: Mutex<Option<HashMap<String, SessionRecord>>>,
}

impl Registry {
    pub const fn new() -> Self {
        Self {
            cache: Mutex::new(None),
        }
    }

    /// 锁获取（毒化即取回内层数据：本域状态是「可重建的镜像」，不因一次 panic 作废）
    fn cache(&self) -> std::sync::MutexGuard<'_, Option<HashMap<String, SessionRecord>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 懒加载镜像（幂等；载入失败不缓存半成品）
    fn load(&self, store: &impl SessionStore) -> Result<(), String> {
        let mut guard = self.cache();
        if guard.is_some() {
            return Ok(());
        }
        let mut map = HashMap::new();
        for record in store.all()? {
            map.insert(record.id.clone(), record);
        }
        *guard = Some(map);
        Ok(())
    }

    /// 全量记录（按 `created_at, id` 稳定序）
    pub fn all(&self, store: &impl SessionStore) -> Result<Vec<SessionRecord>, String> {
        self.load(store)?;
        let mut records: Vec<SessionRecord> = self
            .cache()
            .as_ref()
            .map(|m| m.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        records.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(records)
    }

    /// 单条记录
    pub fn get(&self, store: &impl SessionStore, id: &str) -> Result<Option<SessionRecord>, String> {
        self.load(store)?;
        Ok(self.cache().as_ref().and_then(|m| m.get(id).cloned()))
    }

    /// 写穿一条记录（新建或整体覆盖）
    pub fn record(&self, store: &impl SessionStore, record: &SessionRecord) -> Result<(), String> {
        store.put(record)?;
        self.load(store)?;
        self.cache()
            .as_mut()
            .expect("loaded above")
            .insert(record.id.clone(), record.clone());
        Ok(())
    }

    /// 状态迁移（走 [`ops::transition`] 的合法性表；同态不写库）
    ///
    /// 返回 `Ok(None)` = 会话不在册（不新增孤儿记录）
    pub fn set_status(
        &self,
        store: &impl SessionStore,
        id: &str,
        to: SessionStatus,
        now: &str,
    ) -> Result<Option<SessionRecord>, String> {
        self.update(store, id, |current| ops::transition(current, to, now))
    }

    /// 改名（只改展示名，与宿主 `rename_session` 同语义）
    pub fn set_name(
        &self,
        store: &impl SessionStore,
        id: &str,
        name: &str,
        now: &str,
    ) -> Result<Option<SessionRecord>, String> {
        self.update(store, id, |current| {
            let mut next = current.clone();
            next.name = name.to_string();
            next.updated_at = now.to_string();
            Ok(next)
        })
    }

    /// 登记正统渲染端归属（尺寸裁决的登记事实）
    pub fn set_canonical(
        &self,
        store: &impl SessionStore,
        id: &str,
        source: &RendererSource,
        now: &str,
    ) -> Result<Option<SessionRecord>, String> {
        self.update(store, id, |current| {
            let mut next = current.clone();
            next.canonical_renderer = Some(source.clone());
            next.updated_at = now.to_string();
            Ok(next)
        })
    }

    /// 通用「读—改—写」：会话不在册返回 `Ok(None)`；变更函数报错则不落库
    pub fn update(
        &self,
        store: &impl SessionStore,
        id: &str,
        apply: impl FnOnce(&SessionRecord) -> Result<SessionRecord, String>,
    ) -> Result<Option<SessionRecord>, String> {
        self.load(store)?;
        let current = self.cache().as_ref().and_then(|m| m.get(id).cloned());
        let Some(current) = current else {
            return Ok(None);
        };
        let next = apply(&current)?;
        if next == current {
            return Ok(Some(current));
        }
        store.put(&next)?;
        self.cache()
            .as_mut()
            .expect("loaded above")
            .insert(id.to_string(), next.clone());
        Ok(Some(next))
    }

    /// 移除会话（连带清注解槽）；返回是否命中（未知 id 幂等 false）
    pub fn remove(&self, store: &impl SessionStore, id: &str) -> Result<bool, String> {
        let hit = store.remove(id)?;
        store.clear_annotations(id)?;
        self.load(store)?;
        self.cache().as_mut().expect("loaded above").remove(id);
        Ok(hit)
    }

    /// 会话注解槽（会话不在册 → 空表；不读孤儿键）
    pub fn annotations(
        &self,
        store: &impl SessionStore,
        id: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        if self.get(store, id)?.is_none() {
            return Ok(BTreeMap::new());
        }
        Ok(store.all_annotations(id)?.into_iter().collect())
    }

    /// 写会话注解槽；返回 false = 会话不在册（与宿主 `annotate_session` 同语义，
    /// 不写孤儿键）
    pub fn annotate(
        &self,
        store: &impl SessionStore,
        id: &str,
        key: &str,
        value: &str,
    ) -> Result<bool, String> {
        if self.get(store, id)?.is_none() {
            return Ok(false);
        }
        store.put_annotation(id, key, value)?;
        Ok(true)
    }

    /// 清空会话域（进程启动对账，见 [`super::store`] 模块头）；返回删除的会话行数
    pub fn clear(&self, store: &impl SessionStore) -> Result<usize, String> {
        let removed = store.clear_all()?;
        *self.cache() = Some(HashMap::new());
        Ok(removed)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::tests::MockSessionStore;

    fn now() -> &'static str {
        "2026-09-23T10:00:00Z"
    }

    /// 懒加载：库里有行、缓存未载入 → 读得到（且第二次读不再回库，靠缓存命中）
    #[test]
    fn reads_are_lazily_loaded_from_store() {
        let store = MockSessionStore::new(vec![MockSessionStore::record(
            "s1",
            SessionStatus::Running,
        )]);
        let registry = Registry::new();
        let all = registry.all(&store).expect("all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "s1");
        assert_eq!(registry.get(&store, "s1").unwrap().unwrap().name, "会话-s1");
        assert!(registry.get(&store, "missing").unwrap().is_none());
    }

    /// 写穿：记录同时进库与缓存；同 id 覆盖不产生第二行
    #[test]
    fn record_writes_through_to_store_and_cache() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        let mut record = MockSessionStore::record("s1", SessionStatus::Starting);
        registry.record(&store, &record).expect("record");
        assert_eq!(store.all().unwrap().len(), 1, "落库");
        assert_eq!(registry.all(&store).unwrap().len(), 1, "入缓存");

        record.name = "改名后".to_string();
        registry.record(&store, &record).expect("record update");
        assert_eq!(store.all().unwrap().len(), 1, "同 id 覆盖");
        assert_eq!(registry.get(&store, "s1").unwrap().unwrap().name, "改名后");
    }

    /// 状态迁移：合法迁移落库 + 刷时间戳；非法迁移显性报错且**不改动任何状态**
    #[test]
    fn set_status_applies_legal_and_rejects_illegal_without_mutation() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        registry
            .record(&store, &MockSessionStore::record("s1", SessionStatus::Starting))
            .expect("record");

        let running = registry
            .set_status(&store, "s1", SessionStatus::Running, now())
            .expect("legal")
            .expect("in registry");
        assert_eq!(running.status, SessionStatus::Running);
        assert_eq!(running.started_at.as_deref(), Some(now()));
        assert_eq!(
            store.get("s1").unwrap().unwrap().status,
            SessionStatus::Running,
            "落库"
        );

        let err = registry
            .set_status(
                &store,
                "s1",
                SessionStatus::Stopped,
                "2026-09-23T11:00:00Z",
            )
            .expect("stop legal")
            .expect("in registry");
        assert_eq!(err.status, SessionStatus::Stopped);

        let failure = registry
            .set_status(&store, "s1", SessionStatus::Running, now())
            .expect_err("Stopped → Running 必须报错");
        assert!(failure.contains("illegal session status transition"), "got: {failure}");
        assert_eq!(
            store.get("s1").unwrap().unwrap().status,
            SessionStatus::Stopped,
            "非法迁移不得改动库中状态"
        );
        assert_eq!(
            registry.get(&store, "s1").unwrap().unwrap().status,
            SessionStatus::Stopped,
            "非法迁移不得改动缓存"
        );
    }

    /// 未知会话：状态迁移 / 改名 / 归属写都返回 `Ok(None)`（不新增孤儿记录）
    #[test]
    fn unknown_session_is_none_and_never_creates_orphans() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        assert!(registry
            .set_status(&store, "ghost", SessionStatus::Running, now())
            .unwrap()
            .is_none());
        assert!(registry.set_name(&store, "ghost", "x", now()).unwrap().is_none());
        assert!(registry
            .set_canonical(&store, "ghost", &RendererSource::Desktop, now())
            .unwrap()
            .is_none());
        assert!(store.all().unwrap().is_empty(), "不得产生孤儿行");
    }

    /// 改名与正统端归属：只动目标字段，状态与时间戳语义保持
    #[test]
    fn rename_and_canonical_only_touch_their_fields() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        registry
            .record(&store, &MockSessionStore::record("s1", SessionStatus::Running))
            .expect("record");

        let renamed = registry
            .set_name(&store, "s1", "新名字", now())
            .unwrap()
            .unwrap();
        assert_eq!(renamed.name, "新名字");
        assert_eq!(renamed.status, SessionStatus::Running, "改名不动状态");
        assert_eq!(renamed.updated_at, now());

        let mobile = RendererSource::Mobile {
            device_name: "Pixel-9".to_string(),
        };
        let claimed = registry
            .set_canonical(&store, "s1", &mobile, now())
            .unwrap()
            .unwrap();
        assert_eq!(claimed.canonical_renderer, Some(mobile));
        assert_eq!(claimed.name, "新名字", "登记归属不动名字");
    }

    /// 移除：库、缓存、注解槽三处一并清理；重复移除幂等 false
    #[test]
    fn remove_clears_record_and_annotations_everywhere() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        registry
            .record(&store, &MockSessionStore::record("s1", SessionStatus::Running))
            .expect("record");
        assert!(registry.annotate(&store, "s1", "taskStatus", "asking").unwrap());

        assert!(registry.remove(&store, "s1").unwrap());
        assert!(registry.get(&store, "s1").unwrap().is_none(), "缓存已清");
        assert!(store.get("s1").unwrap().is_none(), "库中已清");
        assert!(
            store.all_annotations("s1").unwrap().is_empty(),
            "注解槽连带清理"
        );
        assert!(!registry.remove(&store, "s1").unwrap(), "重复移除幂等 false");
    }

    /// 注解槽：会话不在册 → false（不写孤儿键）；在册 → 覆盖写 + 稳定序读回
    #[test]
    fn annotations_require_existing_session_and_overwrite_same_key() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        assert!(
            !registry.annotate(&store, "ghost", "k", "v").unwrap(),
            "未知会话不得写孤儿键"
        );
        registry
            .record(&store, &MockSessionStore::record("s1", SessionStatus::Running))
            .expect("record");
        assert!(registry.annotate(&store, "s1", "taskStatus", "asking").unwrap());
        assert!(registry.annotate(&store, "s1", "taskStatus", "idle").unwrap());
        assert!(registry.annotate(&store, "s1", "taskReason", "等待").unwrap());

        let slots = registry.annotations(&store, "s1").unwrap();
        assert_eq!(slots.get("taskStatus").map(String::as_str), Some("idle"));
        assert_eq!(slots.get("taskReason").map(String::as_str), Some("等待"));
        assert_eq!(slots.len(), 2, "同键只留一行");
        assert!(
            registry.annotations(&store, "ghost").unwrap().is_empty(),
            "未知会话读空槽"
        );
    }

    /// 启动对账清表：库与缓存一并清空，返回删除行数
    #[test]
    fn clear_drops_store_and_cache() {
        let store = MockSessionStore::new(vec![
            MockSessionStore::record("s1", SessionStatus::Running),
            MockSessionStore::record("s2", SessionStatus::Stopped),
        ]);
        let registry = Registry::new();
        registry.load(&store).expect("load");
        assert_eq!(registry.all(&store).unwrap().len(), 2);

        assert_eq!(registry.clear(&store).unwrap(), 2);
        assert!(store.all().unwrap().is_empty(), "库已清");
        assert!(registry.all(&store).unwrap().is_empty(), "缓存已清");
    }

    /// 稳定读序：`created_at` 升序、同刻按 id（宿主 `HashMap` 迭代序不确定，本域不复制）
    #[test]
    fn all_is_sorted_by_created_at_then_id() {
        let store = MockSessionStore::new(vec![]);
        let registry = Registry::new();
        let mut late = MockSessionStore::record("s-b", SessionStatus::Running);
        late.created_at = "2026-09-23T12:00:00Z".to_string();
        let mut early_b = MockSessionStore::record("s-b2", SessionStatus::Running);
        early_b.created_at = "2026-09-23T08:00:00Z".to_string();
        let mut early_a = MockSessionStore::record("s-a", SessionStatus::Running);
        early_a.created_at = "2026-09-23T08:00:00Z".to_string();
        for record in [&late, &early_b, &early_a] {
            registry.record(&store, record).expect("record");
        }
        let ids: Vec<String> = registry
            .all(&store)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["s-a", "s-b2", "s-b"]);
    }
}
