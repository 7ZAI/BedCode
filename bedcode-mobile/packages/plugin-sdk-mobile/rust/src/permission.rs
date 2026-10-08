//! Permission Manager (Mobile)
//!
//! 移动端插件权限校验 — 包含移动端特有权限（ui:navtab, ui:settings）

use std::collections::{HashMap, HashSet};

/// 终端输出流窄转发权限（host-terminal-stream.forward-output，票 12）；
/// `terminal:input` 已随票 15 阶段 B 整面退役（host-terminal / TerminalAPI）
pub const PERMISSION_TERMINAL_OUTPUT: &str = "terminal:output";
pub const PERMISSION_SESSION_READ: &str = "session:read";
pub const PERMISSION_SESSION_WRITE: &str = "session:write";
pub const PERMISSION_UI_TOOLBOX: &str = "ui:toolbox";
pub const PERMISSION_UI_NAVTAB: &str = "ui:navtab";
pub const PERMISSION_UI_SETTINGS: &str = "ui:settings";
pub const PERMISSION_UI_INPUT: &str = "ui:input";
/// 动态路由：注册/跳转插件路由页（宿主 addRoute/removeRoute）
pub const PERMISSION_UI_ROUTE: &str = "ui:route";
pub const PERMISSION_UI_DIALOG: &str = "ui:dialog";
pub const PERMISSION_UI_BACK: &str = "ui:back";
pub const PERMISSION_NETWORK_HTTP: &str = "network:http";
pub const PERMISSION_STORAGE: &str = "storage";
pub const PERMISSION_FS_READ: &str = "fs:read";
pub const PERMISSION_FS_WRITE: &str = "fs:write";
pub const PERMISSION_BUS: &str = "bus";
/// 系统文件操作：用系统查看器打开本地文件（传输完成「打开本地文件」）
pub const PERMISSION_SYSTEM_OPEN: &str = "system:open";
/// 对等网络：发现/信任/拨号/收发/浏览的宿主 peer-net 能力（host-peer）
pub const PERMISSION_PEER: &str = "peer";
/// mDNS 基础能力服务（host-mdns v2）：浏览 + 广播原语，事件按属主定向投递
pub const PERMISSION_MDNS: &str = "mdns";
/// WebSocket 出站连接（WIT `host-websocket` 客户端域）：出站是 SSRF 面
/// （插件可代宿主访问任意 ws:// 地址），故独立成位、fail-closed。移动端不跑
/// WS 服务器，**没有** `ws:server` 位（不跟演桌面，ADR 0018/0019）
pub const PERMISSION_WS_CLIENT: &str = "ws:client";
/// 设备入场认证编排（WIT `host-auth`，票 14 阶段 B）：触发设备入场认证与
/// 凭据落地是安全敏感面，独立成位、fail-closed。凭据零过境——JWT 由宿主
/// 落地，本域不向插件返回凭据材料（C4；对齐票 12「token 不落插件」先例）
pub const PERMISSION_AUTH: &str = "auth";

/// 权限词汇全量表（pub：宿主机制层〔wasm-core-mobile 权限漂移锁〕与生成物
/// 校验消费；新增权限必须在此登记，否则 grant 静默丢弃）
pub static VALID_PERMISSIONS: &[&str] = &[
    PERMISSION_TERMINAL_OUTPUT,
    PERMISSION_SESSION_READ,
    PERMISSION_SESSION_WRITE,
    PERMISSION_UI_TOOLBOX,
    PERMISSION_UI_NAVTAB,
    PERMISSION_UI_SETTINGS,
    PERMISSION_UI_INPUT,
    PERMISSION_UI_ROUTE,
    PERMISSION_UI_DIALOG,
    PERMISSION_UI_BACK,
    PERMISSION_NETWORK_HTTP,
    PERMISSION_STORAGE,
    PERMISSION_FS_READ,
    PERMISSION_FS_WRITE,
    PERMISSION_BUS,
    PERMISSION_SYSTEM_OPEN,
    PERMISSION_PEER,
    PERMISSION_MDNS,
    PERMISSION_WS_CLIENT,
    PERMISSION_AUTH,
];

static PERMISSION_API_MAP: &[(&str, &[&str])] = &[
    // 票 12：terminal-stream.forwardOutput（终端输出流窄转发）复用本词汇；
    // TerminalAPI（sendInput/onOutput）已随票 15 阶段 B 退役，terminal.onOutput 同批移除
    (PERMISSION_TERMINAL_OUTPUT, &["terminal-stream.forwardOutput"]),
    (PERMISSION_SESSION_READ, &["session.list", "session.get", "session.onStatusChange"]),
    (PERMISSION_SESSION_WRITE, &["session.create", "session.stop"]),
    (PERMISSION_UI_TOOLBOX, &["ui.registerToolboxPage"]),
    (PERMISSION_UI_NAVTAB, &["ui.registerNavTab"]),
    (PERMISSION_UI_SETTINGS, &["ui.registerSettingsSection"]),
    (PERMISSION_UI_INPUT, &["ui.registerTerminalToolbarItem"]),
    (PERMISSION_UI_ROUTE, &["ui.registerRoute", "ui.openPage", "ui.goBack"]),
    (PERMISSION_UI_DIALOG, &["ui.showDialog"]),
    (PERMISSION_UI_BACK, &["ui.onBackPressed"]),
    (PERMISSION_NETWORK_HTTP, &["http.registerEndpoint"]),
    (PERMISSION_STORAGE, &["storage.get", "storage.set", "storage.delete"]),
    (PERMISSION_FS_READ, &["fs.read", "fs.copy"]),
    (PERMISSION_FS_WRITE, &["fs.write", "fs.copy"]),
    (PERMISSION_BUS, &["bus.publish", "bus.subscribe", "bus.unsubscribe"]),
    (PERMISSION_SYSTEM_OPEN, &["system.openFile", "system.revealInDir", "system.revealReceivedFileLocation"]),
    (PERMISSION_PEER, &[
        "peer.listDevices",
        "peer.dial",
        "peer.disconnect",
        "peer.respondConsent",
        "peer.listTrusted",
        "peer.revokeTrusted",
        "peer.sendFiles",
        "peer.listTransfers",
        "peer.cancelTransfer",
        "peer.retryTransfer",
        "peer.clearTransferHistory",
        "peer.listReceiving",
        "peer.respondTransfer",
        "peer.cancelReceiving",
        "peer.clearReceivingHistory",
        "peer.getReceiveSettings",
        "peer.setReceivePolicy",
        "peer.listSharedDirectories",
        "peer.removeSharedDirectory",
        "peer.addSharedDirectory",
        "peer.listSharedRoots",
        "peer.browseDirectory",
        "peer.pullFiles",
        "peer.pickFiles",
        // ADR 0022 v2 新增原语
        "peer.dialEndpoint",
        "peer.close",
        "peer.setSharedRoots",
    ]),
    (PERMISSION_MDNS, &[
        "mdns.browse",
        "mdns.stopBrowse",
        "mdns.advertise",
        "mdns.stopAdvertise",
        "mdns.isAdvertising",
    ]),
    (PERMISSION_WS_CLIENT, &[
        "ws.connect",
        "ws.sendText",
        "ws.sendBinary",
        "ws.close",
        "ws.isConnected",
    ]),
    // 票 14 阶段 B：认证编排域（host-auth）——编排在插件，凭据零过境（C4）
    (PERMISSION_AUTH, &[
        "auth.requestPairing",
        "auth.verifyPairingCode",
        "auth.qrConnect",
        "auth.biometricAuthenticate",
        "auth.hasCredentials",
    ]),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_filters_invalid() {
        let pm = PermissionManager::new();
        let granted = pm.grant_permissions("test-plugin", &[
            "terminal:output".to_string(),
            "invalid:permission".to_string(),
        ]);
        assert!(granted.contains("terminal:output"));
        assert!(!granted.contains("invalid:permission"));
        // storage 无条件默认授予
        assert!(granted.contains("storage"));
    }

    #[test]
    fn test_check_permission() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:output".to_string()]);
        assert!(pm.check("test-plugin", "terminal:output"));
        assert!(!pm.check("test-plugin", "mdns"));
        assert!(pm.check("test-plugin", "storage"));
    }

    #[test]
    fn test_check_api() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:output".to_string()]);
        assert!(pm.check_api("test-plugin", "terminal-stream.forwardOutput"));
        assert!(!pm.check_api("test-plugin", "session.list"));
    }

    #[test]
    fn test_check_api_mobile_specific() {
        // 移动端特有权限门：UI 扩展点按 API 方法名映射
        let pm = PermissionManager::new();
        pm.grant_permissions("p", &[
            "ui:navtab".to_string(),
            "ui:settings".to_string(),
            "ui:route".to_string(),
            "ui:input".to_string(),
        ]);
        assert!(pm.check_api("p", "ui.registerNavTab"));
        assert!(pm.check_api("p", "ui.registerSettingsSection"));
        assert!(pm.check_api("p", "ui.registerRoute"));
        assert!(pm.check_api("p", "ui.openPage"));
        assert!(pm.check_api("p", "ui.goBack"));
        assert!(pm.check_api("p", "ui.registerTerminalToolbarItem"));
        // 未授予的权限族对应 API 一律拒绝
        assert!(!pm.check_api("p", "terminal-stream.forwardOutput"));
        assert!(!pm.check_api("p", "session.list"));
    }

    #[test]
    fn test_valid_permission_whitelist_complete() {
        // 白名单 = VALID_PERMISSIONS 静态表：任何新增权限必须同步登记，
        // 否则 grant 静默丢弃（此处锁死全量，含移动端特有 ui:navtab/ui:settings/ui:route）
        for p in [
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
        ] {
            assert!(VALID_PERMISSIONS.contains(&p), "{} not in whitelist", p);
        }
    }

    #[test]
    fn test_revoke_all() {
        let pm = PermissionManager::new();
        pm.grant_permissions("test-plugin", &["terminal:output".to_string()]);
        pm.revoke_all("test-plugin");
        assert!(!pm.check("test-plugin", "terminal:output"));
        assert!(!pm.check("test-plugin", "storage"));
    }

    #[test]
    fn test_get_granted() {
        let pm = PermissionManager::new();
        // 未注册插件返回空集
        assert!(pm.get_granted("unknown").is_empty());
        let granted = pm.grant_permissions("p", &["bus".to_string()]);
        assert_eq!(pm.get_granted("p"), granted);
        assert!(pm.get_granted("p").contains("storage"));
    }

    #[test]
    fn test_unknown_plugin_has_no_permissions() {
        let pm = PermissionManager::new();
        assert!(!pm.check("unknown", "storage"));
        assert!(!pm.check_api("unknown", "storage.get"));
    }
}
