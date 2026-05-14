//! Mobile Settings Storage Module
//!
//! 移动端设置存储 - 使用 JSON 文件而非 SQLite
//!
//! 仅在移动端编译 (target_os = "android" || target_os = "ios")

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 移动端设置存储结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobileSettings {
    /// 设置键值对
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

impl MobileSettings {
    /// 从文件加载设置
    pub fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)?;
        let settings: MobileSettings = serde_json::from_str(&content)
            .map_err(|e| crate::AppError::Config(e.to_string()))?;
        Ok(settings)
    }

    /// 保存设置到文件
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| crate::AppError::Config(e.to_string()))?;
        fs::write(path, content)?;
        Ok(())
    }

    /// 获取设置值
    pub fn get(&self, key: &str) -> Option<String> {
        self.settings.get(key).cloned()
    }

    /// 设置值
    pub fn set(&mut self, key: String, value: String) {
        self.settings.insert(key, value);
    }

    /// 获取所有设置
    pub fn get_all(&self) -> Vec<(String, String)> {
        self.settings.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// 移动端设置管理器
pub struct SettingsManager {
    /// 设置文件路径
    path: PathBuf,
    /// 内存缓存
    settings: Arc<RwLock<MobileSettings>>,
}

impl SettingsManager {
    /// 创建新的设置管理器
    pub fn new(app_data_dir: &PathBuf) -> Result<Self> {
        let path = app_data_dir.join("mobile_settings.json");
        let settings = MobileSettings::load(&path)?;

        Ok(Self {
            path,
            settings: Arc::new(RwLock::new(settings)),
        })
    }

    /// 获取设置值
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let settings = self.settings.read().await;
        Ok(settings.get(key))
    }

    /// 设置值
    pub async fn set(&self, key: String, value: String) -> Result<()> {
        let mut settings = self.settings.write().await;
        settings.set(key, value);
        // 在写锁作用域内保存，避免并发竞争
        settings.save(&self.path)?;
        Ok(())
    }

    /// 获取所有设置
    pub async fn get_all(&self) -> Result<Vec<(String, String)>> {
        let settings = self.settings.read().await;
        Ok(settings.get_all())
    }

    /// 保存到文件
    async fn save(&self) -> Result<()> {
        let settings = self.settings.read().await;
        settings.save(&self.path)?;
        Ok(())
    }
}