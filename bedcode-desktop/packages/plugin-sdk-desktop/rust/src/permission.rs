//! Permission Manager
//!
//! 插件权限校验 — 双重校验的后端最终仲裁层
//! 从桌面端 permission.rs 迁移，作为 api crate 的一部分供插件和主应用共用

use std::collections::{HashMap, HashSet};

/// 所有合法权限常量
pub const PERMISSION_TERMINAL_INPUT: &str = "terminal:input";
pub const PERMISSION_TERMINAL_OUTPUT: &str = "terminal:output";
pub const PERMISSION_SESSION_READ: &str = "session:read";
pub const PERMISSION_SESSION_WRITE: &str = "session:write";
pub const PERMISSION_UI_SIDEBAR: &str = "ui:sidebar";
pub const PERMISSION_UI_TOOLBOX: &str = "ui:toolbox";
pub const PERMISSION_UI_STATUSBAR: &str = "ui:statusbar";
pub const PERMISSION_UI_INPUT: &str = "ui:input";
pub const PERMISSION_NETWORK_HTTP: &str = "network:http";
pub const PERMISSION_STORAGE: &str = "storage";

/// 合法权限集合
static VALID_PERMISSIONS: &[&str] = &[
    PERMISSION_TERMINAL_INPUT,
    PERMISSION_TERMINAL_OUTPUT,
    PERMISSION_SESSION_READ,
    PERMISSION_SESSION_WRITE,
    PERMISSION_UI_SIDEBAR,
    PERMISSION_UI_TOOLBOX,
    PERMISSION_UI_STATUSBAR,
    PERMISSION_UI_INPUT,
    PERMISSION_NETWORK_HTTP,
    PERMISSION_STORAGE,
];

/// 权限到 API 方法的映射
static PERMISSION_API_MAP: &[(&str, &[&str])] = &[
    (PERMISSION_TERMINAL_INPUT, &["terminal.sendInput", "terminal.onInput"]),
    (PERMISSION_TERMINAL_OUTPUT, &["terminal.onOutput"]),
    (PERMISSION_SESSION_READ, &["session.list", "session.get", "session.onStatusChange"]),
    (PERMISSION_SESSION_WRITE, &["session.create", "session.stop"]),
    (PERMISSION_UI_SIDEBAR, &["ui.registerSidebarPanel"]),
    (PERMISSION_UI_TOOLBOX, &["ui.registerToolboxPage"]),
    (PERMISSION_UI_STATUSBAR, &["ui.registerStatusBarItem", "ui.registerTitleBarItem"]),
    (PERMISSION_UI_INPUT, &["ui.registerInputExtension", "ui.registerTerminalToolbarItem"]),
    (PERMISSION_NETWORK_HTTP, &["http.registerEndpoint"]),
    (PERMISSION_STORAGE, &["storage.get", "storage.set", "storage.delete", "storage.flush"]),
];

/// 权限管理器
pub struct PermissionManager {
    /// 插件 ID → 已授予的权限集合
    granted: std::sync::RwLock<HashMap<String, HashSet<String>>>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self {
            granted: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 为插件授权（从 manifest permissions 字段解析，过滤非法权限）
    pub fn grant_permissions(&self, plugin_id: &str, requested: &[String]) -> HashSet<String> {
        let valid_set: HashSet<&str> = VALID_PERMISSIONS.iter().copied().collect();
        let granted: HashSet<String> = requested
            .iter()
            .filter(|p| valid_set.contains(p.as_str()))
            .cloned()
            .collect();

        // storage 权限默认授予
        let mut granted = granted;
        granted.insert(PERMISSION_STORAGE.to_string());

        let mut lock = self.granted.write().unwrap_or_else(|e| e.into_inner());
        lock.insert(plugin_id.to_string(), granted.clone());

        granted
    }

    /// 检查插件是否拥有指定权限
    pub fn check(&self, plugin_id: &str, permission: &str) -> bool {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        lock.get(plugin_id)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }

    /// 检查插件是否拥有调用指定 API 方法的权限
    pub fn check_api(&self, plugin_id: &str, api_method: &str) -> bool {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        let perms = match lock.get(plugin_id) {
            Some(p) => p,
            None => return false,
        };

        for (perm, apis) in PERMISSION_API_MAP {
            if apis.iter().any(|a| *a == api_method) {
                return perms.contains(*perm);
            }
        }
        false
    }

    /// 移除插件的权限（停用时调用）
    pub fn revoke_all(&self, plugin_id: &str) {
        let mut lock = self.granted.write().unwrap_or_else(|e| e.into_inner());
        lock.remove(plugin_id);
    }

    /// 获取插件的已授予权限列表
    pub fn get_granted(&self, plugin_id: &str) -> HashSet<String> {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        lock.get(plugin_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_filters_invalid() {
        let pm = PermissionManager::new();
        let granted = pm.grant_permissions("test-plugin", &[
            "terminal:input".to_string(),
            "invalid:permission".to_string(),
        ]);
        assert!(granted.contains("terminal:input"));
        assert!(!granted.contains("invalid:permission"));
        assert!(granted.contains("storage"));
    }

    #[test]
    fn test_check_permission() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:input".to_string()]);
        assert!(pm.check("test-plugin", "terminal:input"));
        assert!(!pm.check("test-plugin", "terminal:output"));
    }

    #[test]
    fn test_check_api() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:input".to_string()]);
        assert!(pm.check_api("test-plugin", "terminal.sendInput"));
        assert!(!pm.check_api("test-plugin", "terminal.onOutput"));
    }

    #[test]
    fn test_revoke_all() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:input".to_string()]);
        pm.revoke_all("test-plugin");
        assert!(!pm.check("test-plugin", "terminal:input"));
    }

    #[test]
    fn test_unknown_plugin_has_no_permissions() {
        let pm = PermissionManager::new();
        assert!(!pm.check("unknown", "storage"));
        assert!(!pm.check_api("unknown", "storage.get"));
    }
}
