//! Permission Manager (Mobile)
//!
//! 移动端插件权限校验 — 包含移动端特有权限（ui:navtab, ui:settings）

use std::collections::{HashMap, HashSet};

pub const PERMISSION_TERMINAL_INPUT: &str = "terminal:input";
pub const PERMISSION_TERMINAL_OUTPUT: &str = "terminal:output";
pub const PERMISSION_SESSION_READ: &str = "session:read";
pub const PERMISSION_SESSION_WRITE: &str = "session:write";
pub const PERMISSION_UI_TOOLBOX: &str = "ui:toolbox";
pub const PERMISSION_UI_NAVTAB: &str = "ui:navtab";
pub const PERMISSION_UI_SETTINGS: &str = "ui:settings";
pub const PERMISSION_UI_INPUT: &str = "ui:input";
/// 动态路由：注册/跳转插件路由页（宿主 addRoute/removeRoute）
pub const PERMISSION_UI_ROUTE: &str = "ui:route";
pub const PERMISSION_NETWORK_HTTP: &str = "network:http";
pub const PERMISSION_STORAGE: &str = "storage";
pub const PERMISSION_FS_READ: &str = "fs:read";
pub const PERMISSION_FS_WRITE: &str = "fs:write";
pub const PERMISSION_BUS: &str = "bus";
/// 文件服务：挂载受控文件服务端点（与桌面端同名权限，见内网文件传输插件规格）
pub const PERMISSION_FILESERVICE: &str = "fileservice";
/// 传输引擎：发起断点续传的文件上传/下载任务
pub const PERMISSION_TRANSFER: &str = "transfer";

static VALID_PERMISSIONS: &[&str] = &[
    PERMISSION_TERMINAL_INPUT,
    PERMISSION_TERMINAL_OUTPUT,
    PERMISSION_SESSION_READ,
    PERMISSION_SESSION_WRITE,
    PERMISSION_UI_TOOLBOX,
    PERMISSION_UI_NAVTAB,
    PERMISSION_UI_SETTINGS,
    PERMISSION_UI_INPUT,
    PERMISSION_UI_ROUTE,
    PERMISSION_NETWORK_HTTP,
    PERMISSION_STORAGE,
    PERMISSION_FS_READ,
    PERMISSION_FS_WRITE,
    PERMISSION_BUS,
    PERMISSION_FILESERVICE,
    PERMISSION_TRANSFER,
];

static PERMISSION_API_MAP: &[(&str, &[&str])] = &[
    (PERMISSION_TERMINAL_INPUT, &["terminal.sendInput", "terminal.onInput"]),
    (PERMISSION_TERMINAL_OUTPUT, &["terminal.onOutput"]),
    (PERMISSION_SESSION_READ, &["session.list", "session.get", "session.onStatusChange"]),
    (PERMISSION_SESSION_WRITE, &["session.create", "session.stop"]),
    (PERMISSION_UI_TOOLBOX, &["ui.registerToolboxPage"]),
    (PERMISSION_UI_NAVTAB, &["ui.registerNavTab"]),
    (PERMISSION_UI_SETTINGS, &["ui.registerSettingsSection"]),
    (PERMISSION_UI_INPUT, &["ui.registerTerminalToolbarItem"]),
    (PERMISSION_UI_ROUTE, &["ui.registerRoute", "ui.openPage", "ui.goBack"]),
    (PERMISSION_NETWORK_HTTP, &["http.registerEndpoint"]),
    (PERMISSION_STORAGE, &["storage.get", "storage.set", "storage.delete"]),
    (PERMISSION_FS_READ, &["fs.read", "fs.copy"]),
    (PERMISSION_FS_WRITE, &["fs.write", "fs.copy"]),
    (PERMISSION_BUS, &["bus.publish", "bus.subscribe", "bus.unsubscribe"]),
    (PERMISSION_FILESERVICE, &[
        "fileService.mount",
        "fileService.unmount",
        "fileService.updateRoots",
        "fileService.getPeer",
        "fileService.pickDirectory",
        "fileService.requestAllFilesAccess",
    ]),
    (PERMISSION_TRANSFER, &["transfer.start", "transfer.cancel"]),
];

pub struct PermissionManager {
    granted: std::sync::RwLock<HashMap<String, HashSet<String>>>,
}

impl PermissionManager {
    pub fn new() -> Self { Self { granted: std::sync::RwLock::new(HashMap::new()) } }

    pub fn grant_permissions(&self, plugin_id: &str, requested: &[String]) -> HashSet<String> {
        let valid_set: HashSet<&str> = VALID_PERMISSIONS.iter().copied().collect();
        let mut granted: HashSet<String> = requested.iter().filter(|p| valid_set.contains(p.as_str())).cloned().collect();
        granted.insert(PERMISSION_STORAGE.to_string());
        let mut lock = self.granted.write().unwrap_or_else(|e| e.into_inner());
        lock.insert(plugin_id.to_string(), granted.clone());
        granted
    }

    pub fn check(&self, plugin_id: &str, permission: &str) -> bool {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        lock.get(plugin_id).map(|perms| perms.contains(permission)).unwrap_or(false)
    }

    pub fn check_api(&self, plugin_id: &str, api_method: &str) -> bool {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        let perms = match lock.get(plugin_id) { Some(p) => p, None => return false };
        for (perm, apis) in PERMISSION_API_MAP {
            if apis.iter().any(|a| *a == api_method) { return perms.contains(*perm); }
        }
        false
    }

    pub fn revoke_all(&self, plugin_id: &str) {
        let mut lock = self.granted.write().unwrap_or_else(|e| e.into_inner());
        lock.remove(plugin_id);
    }

    pub fn get_granted(&self, plugin_id: &str) -> HashSet<String> {
        let lock = self.granted.read().unwrap_or_else(|e| e.into_inner());
        lock.get(plugin_id).cloned().unwrap_or_default()
    }
}

impl Default for PermissionManager {
    fn default() -> Self { Self::new() }
}
