//! Mobile Plugin Storage
//!
//! 每个插件独立的键值存储，数据持久化到 app_data_dir/plugins/{plugin_id}.json

use crate::Result;
use crate::system::constants::plugin::{PLUGIN_STORAGE_DIR, PLUGIN_STORAGE_EXT};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 插件键值存储管理器
pub struct PluginStorage {
    /// 存储文件根目录
    base_dir: PathBuf,
    /// 内存缓存：plugin_id → (key → value)
    caches: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
}

impl PluginStorage {
    /// 创建存储管理器
    pub fn new(app_data_dir: &PathBuf) -> Self {
        let base_dir = app_data_dir.join(PLUGIN_STORAGE_DIR);
        let _ = fs::create_dir_all(&base_dir);

        Self {
            base_dir,
            caches: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取值
    pub async fn get(&self, plugin_id: &str, key: &str) -> Result<Option<Value>> {
        let caches = self.caches.read().await;
        if let Some(store) = caches.get(plugin_id) {
            return Ok(store.get(key).cloned());
        }
        drop(caches);

        // 缓存未命中，从磁盘加载
        let store = self.load_from_disk(plugin_id)?;
        let value = store.get(key).cloned();

        let mut caches = self.caches.write().await;
        caches.insert(plugin_id.to_string(), store);

        Ok(value)
    }

    /// 设置值
    pub async fn set(&self, plugin_id: &str, key: &str, value: Value) -> Result<()> {
        let mut caches = self.caches.write().await;
        let store = caches.entry(plugin_id.to_string()).or_insert_with(|| {
            self.load_from_disk(plugin_id).unwrap_or_default()
        });
        store.insert(key.to_string(), value);
        drop(caches);

        self.flush(plugin_id).await
    }

    /// 删除值
    pub async fn delete(&self, plugin_id: &str, key: &str) -> Result<()> {
        let mut caches = self.caches.write().await;
        if let Some(store) = caches.get_mut(plugin_id) {
            store.remove(key);
        }
        drop(caches);

        self.flush(plugin_id).await
    }

    /// 刷写到磁盘
    pub async fn flush(&self, plugin_id: &str) -> Result<()> {
        let caches = self.caches.read().await;
        if let Some(store) = caches.get(plugin_id) {
            let path = self.storage_path(plugin_id);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string(store)?;
            fs::write(path, content)?;
        }
        Ok(())
    }

    /// 删除插件全部存储
    pub async fn clear_plugin(&self, plugin_id: &str) -> Result<()> {
        self.caches.write().await.remove(plugin_id);
        let path = self.storage_path(plugin_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// 从磁盘加载
    fn load_from_disk(&self, plugin_id: &str) -> Result<HashMap<String, Value>> {
        let path = self.storage_path(plugin_id);
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(path)?;
        let store: HashMap<String, Value> = serde_json::from_str(&content)?;
        Ok(store)
    }

    /// 存储文件路径
    fn storage_path(&self, plugin_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}{}", plugin_id, PLUGIN_STORAGE_EXT))
    }
}
