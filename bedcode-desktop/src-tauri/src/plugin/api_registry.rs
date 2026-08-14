//! 插件互调 api 注册表（ADR-0017 层 1 门禁）
//!
//! 插件激活时登记 manifest `api` 声明的全限定 api 名（如
//! `com.bedcode.scheduler.add`），停用时注销。`bus_publish` 对
//! `bedcode.api.*` 请求 topic 做目标校验：api 名必须命中注册表，
//! 否则拒绝 —— 「注册即声明，未声明不可调」由宿主强制。
//!
//! 注册表只存「目标 api 是否存在」（层 1），不校验调用方身份、不做
//! 版本化（ADR-0017 已决）；激活态插件的 api 才在表中，因此「已注册」
//! 等价于「目标插件已激活」。

use std::collections::HashMap;
use std::sync::RwLock;

/// 插件互调 api 注册表：api 全限定名 → 声明它的插件 ID
pub struct ApiRegistry {
    apis: RwLock<HashMap<String, String>>,
}

impl ApiRegistry {
    pub fn new() -> Self {
        Self {
            apis: RwLock::new(HashMap::new()),
        }
    }

    /// 登记插件声明的 api 清单（激活时调用；重复登记幂等覆盖）
    ///
    /// 同步锁：临界区仅 map 操作（无 await），wasm host 调用栈内
    /// （bus_publish 门禁）与异步激活路径均可用
    pub fn register(&self, plugin_id: &str, apis: &[String]) {
        let mut map = self.apis.write().unwrap_or_else(|e| e.into_inner());
        for api in apis {
            // 同名 api 重复声明：后登记覆盖（同名 api 语义本就应唯一）
            map.insert(api.clone(), plugin_id.to_string());
        }
    }

    /// 注销插件的全部 api（停用时调用；未登记过则幂等无操作）
    pub fn unregister(&self, plugin_id: &str) {
        let mut map = self.apis.write().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, owner| owner != plugin_id);
    }

    /// 目标 api 是否已被某激活插件声明（门禁判定）
    pub fn contains(&self, api: &str) -> bool {
        let map = self.apis.read().unwrap_or_else(|e| e.into_inner());
        map.contains_key(api)
    }

    /// 已登记的 api 清单（诊断/测试用）
    pub fn list_apis(&self) -> Vec<String> {
        let map = self.apis.read().unwrap_or_else(|e| e.into_inner());
        map.keys().cloned().collect()
    }

    /// 登记项数（诊断/测试用）
    pub fn len(&self) -> usize {
        let map = self.apis.read().unwrap_or_else(|e| e.into_inner());
        map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 登记后可命中；未登记的 api 不命中（门禁核心判定）
    #[test]
    fn contains_after_register() {
        let reg = ApiRegistry::new();
        assert!(!reg.contains("com.bedcode.scheduler.add"));
        reg.register("com.bedcode.scheduler", &["com.bedcode.scheduler.add".to_string()]);
        assert!(reg.contains("com.bedcode.scheduler.add"));
        assert!(!reg.contains("com.bedcode.scheduler.none"));
    }

    /// 停用注销后目标不可调（「未激活插件目标调用被拒」验收）
    #[test]
    fn unregister_removes_plugin_apis() {
        let reg = ApiRegistry::new();
        reg.register(
            "com.bedcode.scheduler",
            &["com.bedcode.scheduler.add".to_string(), "com.bedcode.scheduler.list".to_string()],
        );
        reg.register("com.bedcode.other", &["com.bedcode.other.ping".to_string()]);
        reg.unregister("com.bedcode.scheduler");
        assert!(!reg.contains("com.bedcode.scheduler.add"));
        // 其他插件的 api 不受影响
        assert!(reg.contains("com.bedcode.other.ping"));
        assert_eq!(reg.len(), 1);
    }

    /// 重复登记幂等；不同插件声明同名 api 时后者覆盖
    #[test]
    fn register_idempotent_and_overwrite() {
        let reg = ApiRegistry::new();
        reg.register("p1", &["a.x".to_string()]);
        reg.register("p1", &["a.x".to_string()]);
        assert_eq!(reg.len(), 1);
        // 同名 api 后登记覆盖（语义冲突由插件生态避免，注册表仅记录）
        reg.register("p2", &["a.x".to_string()]);
        assert!(reg.contains("a.x"));
        reg.unregister("p1");
        assert!(reg.contains("a.x"), "p2 的登记应保留");
        reg.unregister("p2");
        assert!(!reg.contains("a.x"));
    }

    /// 空清单登记 / 未登记插件注销：幂等无操作
    #[test]
    fn empty_register_and_unknown_unregister_noop() {
        let reg = ApiRegistry::new();
        reg.register("p1", &[]);
        reg.unregister("ghost");
        assert_eq!(reg.len(), 0);
    }
}
