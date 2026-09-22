//! Permission Manager
//!
//! 插件权限校验 — 双重校验的后端最终仲裁层
//! 从桌面端 permission.rs 迁移，作为 api crate 的一部分供插件和主应用共用
//!
//! **权限词汇单一真源**：本文件的 [`VALID_PERMISSIONS`] 与 [`PERMISSION_API_MAP`]
//! 是桌面端唯一的权限清单。打包 CLI（`bin/cli.js`）与宿主前端（`src/plugin/permission.ts`）
//! 的列表都是**生成物**，由 `cargo run --manifest-path rust/Cargo.toml --example
//! gen_permission_vocabulary`（即 SDK 的 `pnpm run gen:permissions`）重出，
//! 禁止手抄；三处一致性由宿主 `plugin/permission.rs` 的词汇漂移锁断言。

use std::collections::{HashMap, HashSet};

/// 所有合法权限常量
pub const PERMISSION_TERMINAL_INPUT: &str = "terminal:input";
pub const PERMISSION_TERMINAL_OUTPUT: &str = "terminal:output";
/// 终端输入观察：注册提交输入行监听器（输入内容可能含密码等敏感信息，需显式授权，见 ADR 0001）
pub const PERMISSION_TERMINAL_OBSERVE: &str = "terminal:observe";
pub const PERMISSION_SESSION_READ: &str = "session:read";
pub const PERMISSION_SESSION_WRITE: &str = "session:write";
pub const PERMISSION_UI_SIDEBAR: &str = "ui:sidebar";
pub const PERMISSION_UI_TOOLBOX: &str = "ui:toolbox";
pub const PERMISSION_UI_STATUSBAR: &str = "ui:statusbar";
pub const PERMISSION_UI_DIALOG: &str = "ui:dialog";
/// 页面工具栏项贡献（纯前端贡献面）：`ui.registerPageToolbarItem`
pub const PERMISSION_UI_PAGE_TOOLBAR: &str = "ui:pageToolbar";
/// 设置分组贡献（纯前端贡献面，无 WASM 宿主函数对应）：`ui.registerSettingsSection`
pub const PERMISSION_UI_SETTINGS: &str = "ui:settings";
pub const PERMISSION_UI_INPUT: &str = "ui:input";
/// 文件处理器贡献（纯前端贡献面）：`ui.registerFileHandler`
pub const PERMISSION_UI_FILE_HANDLER: &str = "ui:fileHandler";
pub const PERMISSION_NETWORK_HTTP: &str = "network:http";
pub const PERMISSION_STORAGE: &str = "storage";
/// 宿主主库 SQL 面（host-database 的 `db_*`，票 02 / P0-1）
///
/// 与 `storage`（插件自有 KV 存储 + `host-plugin-database` 私有库）**分域**：
/// 主库是内核与全部插件共用的库，里面有 `plugin_secrets`（明文宿主托管密钥）、
/// `pairings`、`connection_history`、`settings` 等他人数据。旧形态下所有插件
/// 自动持有 `storage`，主库面又只靠正则表名隔离（逗号多表即可读穿）——
/// 现在主库面要显式声明本位，且访问受 SQLite 引擎层表名白名单仲裁。
/// 当前生产插件零消费者，改判为第一方按需申请的高危位（见 AGENTS §7、票 03 逐位确认）。
pub const PERMISSION_DATABASE_MAIN: &str = "database:main";
pub const PERMISSION_FS_READ: &str = "fs:read";
pub const PERMISSION_FS_WRITE: &str = "fs:write";
pub const PERMISSION_BROADCAST: &str = "broadcast";
/// 定时器：注册宿主周期回调（到点调用插件 command，见 ADR 0003）
pub const PERMISSION_TIMER: &str = "timer:schedule";
/// 进程执行：在桌面端进程内 spawn 外部命令/脚本（host-process，v8）
///
/// 高危能力（执行任意命令），插件 manifest 声明即信任；
/// 每次执行由宿主全量审计日志（命令/参数/cwd/env/结果）
pub const PERMISSION_PROCESS: &str = "process:run";
/// 随包 CLI 生命周期：安装/卸载到用户 bin 目录并注册 PATH（host-app，v8）
///
/// 与 process:run 同信任域（CLI 本质是进程执行入口的封装）；
/// 幂等安装/卸载，仅操作本插件声明的文件与 PATH 条目
pub const PERMISSION_APP_CLI: &str = "app:cli";
/// 对等网络：发现/信任/拨号/收发/浏览的宿主 peer-net 能力（host-peer）
pub const PERMISSION_PEER: &str = "peer";
/// mDNS 基础能力服务（host-mdns v2）：浏览 + 广播原语，事件按属主定向投递
pub const PERMISSION_MDNS: &str = "mdns";
/// WebSocket 客户端域（host-websocket 出站连接：connect / 收发 / 关闭 / 状态查询）
///
/// 与 `ws:server` 分域：出站连接是 SSRF 面（插件可代宿主访问任意 `ws://` 地址），
/// 入站端点是在局域网新增暴露面——单权限通吃会让「只需出站」的插件被动获得
/// 入站监听能力（spec D6）
pub const PERMISSION_WS_CLIENT: &str = "ws:client";
/// WebSocket 服务端域（host-websocket 入站端点：注册 / 收发 / 广播 / 踢出 / 注销 / 清单）
pub const PERMISSION_WS_SERVER: &str = "ws:server";
/// 密钥托管（host-auth / secret-store，v15）：属主隔离的凭据读写
///
/// 认证中心语义下沉的基础权限——声明即信任宿主代管凭据（JWT 密钥 / 配对种子），
/// 插件间互不可见；密钥明文不落日志、持久化于主库 plugin_secrets 表
pub const PERMISSION_AUTH: &str = "auth";
/// 插件私有伪终端·创建域（host-pty，v16）：`spawn` / `kill`
///
/// 与 `pty:io` 分域：spawn 是「在宿主机器上执行任意命令」的高风险面
/// （与 `process:run` 同信任域），kill 决定进程生死；数据面（读写/尺寸/
/// 游标拉取/存活查询）单独一域，便于「只做输出观测」的插件最小授权
pub const PERMISSION_PTY_SPAWN: &str = "pty:spawn";
/// 插件私有伪终端·数据域（host-pty，v16）：`write` / `resize` / `ring-fetch` / `is-running`
pub const PERMISSION_PTY_IO: &str = "pty:io";
/// 宿主并发任务域（host-task，v20，desktop 独有）：`execute-batch` / `submit` /
/// `status` / `cancel` / `list-jobs`
///
/// 管「占用宿主线程池资源」这件事本身（宿主专用 OS 线程池真并行执行单元操作
/// 计划）。**双门结构**：每个单元另过其 kind 对应的既有域权限门（`fs:read` /
/// `fs:write` / `process:run` / `network:http`），仅授本权限不授域权限的插件所有
/// 单元都会失败——并发能力与数据访问能力解耦授权、解耦审计。
pub const PERMISSION_TASK_RUN: &str = "task:run";

/// 权限词汇反射表：`(常量标识符, 权限串)`
///
/// 标识符由 `stringify!` 取自常量本身，与值同源、不可能漂移。有了这一列，宿主测试
/// 就能把「源码里出现的 `PERMISSION_*` 引用」机械还原成权限串，从而断言
/// 「每一条词汇都有真实门禁落点」（见桌面 `src-tauri/src/plugin/permission.rs` 的词汇漂移锁），
/// 而不需要再手抄一份清单。
pub const PERMISSION_VOCABULARY: &[(&str, &str)] = &[
    (stringify!(PERMISSION_TERMINAL_INPUT), PERMISSION_TERMINAL_INPUT),
    (stringify!(PERMISSION_TERMINAL_OUTPUT), PERMISSION_TERMINAL_OUTPUT),
    (stringify!(PERMISSION_TERMINAL_OBSERVE), PERMISSION_TERMINAL_OBSERVE),
    (stringify!(PERMISSION_SESSION_READ), PERMISSION_SESSION_READ),
    (stringify!(PERMISSION_SESSION_WRITE), PERMISSION_SESSION_WRITE),
    (stringify!(PERMISSION_UI_SIDEBAR), PERMISSION_UI_SIDEBAR),
    (stringify!(PERMISSION_UI_TOOLBOX), PERMISSION_UI_TOOLBOX),
    (stringify!(PERMISSION_UI_STATUSBAR), PERMISSION_UI_STATUSBAR),
    (stringify!(PERMISSION_UI_DIALOG), PERMISSION_UI_DIALOG),
    (stringify!(PERMISSION_UI_PAGE_TOOLBAR), PERMISSION_UI_PAGE_TOOLBAR),
    (stringify!(PERMISSION_UI_SETTINGS), PERMISSION_UI_SETTINGS),
    (stringify!(PERMISSION_UI_INPUT), PERMISSION_UI_INPUT),
    (stringify!(PERMISSION_UI_FILE_HANDLER), PERMISSION_UI_FILE_HANDLER),
    (stringify!(PERMISSION_NETWORK_HTTP), PERMISSION_NETWORK_HTTP),
    (stringify!(PERMISSION_STORAGE), PERMISSION_STORAGE),
    (stringify!(PERMISSION_DATABASE_MAIN), PERMISSION_DATABASE_MAIN),
    (stringify!(PERMISSION_FS_READ), PERMISSION_FS_READ),
    (stringify!(PERMISSION_FS_WRITE), PERMISSION_FS_WRITE),
    (stringify!(PERMISSION_BROADCAST), PERMISSION_BROADCAST),
    (stringify!(PERMISSION_TIMER), PERMISSION_TIMER),
    (stringify!(PERMISSION_PROCESS), PERMISSION_PROCESS),
    (stringify!(PERMISSION_APP_CLI), PERMISSION_APP_CLI),
    (stringify!(PERMISSION_PEER), PERMISSION_PEER),
    (stringify!(PERMISSION_MDNS), PERMISSION_MDNS),
    (stringify!(PERMISSION_WS_CLIENT), PERMISSION_WS_CLIENT),
    (stringify!(PERMISSION_WS_SERVER), PERMISSION_WS_SERVER),
    (stringify!(PERMISSION_AUTH), PERMISSION_AUTH),
    (stringify!(PERMISSION_PTY_SPAWN), PERMISSION_PTY_SPAWN),
    (stringify!(PERMISSION_PTY_IO), PERMISSION_PTY_IO),
    (stringify!(PERMISSION_TASK_RUN), PERMISSION_TASK_RUN),
];

/// 合法权限集合 — 桌面端权限词汇的**唯一真源**（由 [`PERMISSION_VOCABULARY`] 派生）
///
/// 未列入本表的权限在 [`PermissionManager::grant_permissions`] 授权时被静默过滤，
/// 因此 manifest 声明了这里没有的字段等于没声明（票 01 的词汇清零即为此而设）。
/// 新增/拆分权限位只需在反射表加一行，再重跑生成器（见模块头），
/// 否则宿主词汇漂移锁转红。
const fn valid_permissions() -> [&'static str; PERMISSION_VOCABULARY.len()] {
    let mut out = [""; PERMISSION_VOCABULARY.len()];
    let mut i = 0;
    while i < out.len() {
        out[i] = PERMISSION_VOCABULARY[i].1;
        i += 1;
    }
    out
}

pub static VALID_PERMISSIONS: &[&str] = &valid_permissions();

/// 权限到 API 方法的映射
///
/// 两种消费者共用本表（生成物由前端与打包 CLI 各自读取）：
/// - **前端 context API 面**（活锁）：`ui.*` / `session.*` / `terminal.*` / `storage.*`
///   / `http.*` 这些名字就是插件前端实际调用的方法名，宿主前端
///   `src/plugin/context.ts` 的 `requirePermission` 按本表快速失败；
/// - **`PermissionManager::check_api` 审计名**：`pty.*` / `ws.*` / `peer.*` / `mdns.*`
///   等 WASM-only 方法无前端调用点，登记于此仅作审计与互调面口径，
///   实际门禁在 host_impl 各函数入口的 `check_permission`。
///
/// 表内每个权限都必有一条门禁落点（前端 `requirePermission` 或 host_impl
/// `check_permission`）——无落点的权限位由宿主词汇漂移锁拒绝。
pub static PERMISSION_API_MAP: &[(&str, &[&str])] = &[
    (PERMISSION_TERMINAL_INPUT, &["terminal.sendInput", "terminal.onInput"]),
    (PERMISSION_TERMINAL_OUTPUT, &["terminal.onOutput"]),
    (PERMISSION_TERMINAL_OBSERVE, &["terminal.onInputSubmitted"]),
    (
        PERMISSION_SESSION_READ,
        &[
            "session.list",
            "session.get",
            "session.onStatusChange",
            // 终端窗口原语（票 13，前端上下文面）：预测初始网格 / 打开 / 关闭
            // 宿主终端窗口 / 窗口在场查询
            "session.predictTerminalSize",
            "session.openTerminal",
            "session.closeTerminal",
            "session.isTerminalOpen",
        ],
    ),
    (PERMISSION_SESSION_WRITE, &["session.create", "session.stop"]),
    (PERMISSION_UI_SIDEBAR, &["ui.registerSidebarPanel", "ui.registerPage"]),
    (PERMISSION_UI_TOOLBOX, &["ui.registerToolboxPage"]),
    (PERMISSION_UI_STATUSBAR, &["ui.registerStatusBarItem", "ui.registerTitleBarItem"]),
    (PERMISSION_UI_DIALOG, &["ui.showDialog"]),
    (PERMISSION_UI_PAGE_TOOLBAR, &["ui.registerPageToolbarItem"]),
    (PERMISSION_UI_SETTINGS, &["ui.registerSettingsSection"]),
    (PERMISSION_UI_INPUT, &["ui.registerInputExtension", "ui.registerTerminalToolbarItem"]),
    (PERMISSION_UI_FILE_HANDLER, &["ui.registerFileHandler"]),
    (PERMISSION_NETWORK_HTTP, &["http.registerEndpoint"]),
    (PERMISSION_STORAGE, &["storage.get", "storage.set", "storage.delete", "storage.flush"]),
    (PERMISSION_BROADCAST, &["broadcast.sync"]),
    (PERMISSION_FS_READ, &["fs.read", "fs.copy"]),
    (PERMISSION_FS_WRITE, &["fs.write", "fs.copy"]),
    (PERMISSION_TIMER, &["timer.register"]),
    (PERMISSION_PROCESS, &["process.run", "process.kill"]),
    (PERMISSION_APP_CLI, &["app.cliInstall", "app.cliUninstall"]),
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
    (PERMISSION_WS_SERVER, &[
        "ws.registerEndpoint",
        "ws.sendTextToClient",
        "ws.sendBinaryToClient",
        "ws.broadcastText",
        "ws.broadcastBinary",
        "ws.closeClient",
        "ws.unregisterEndpoint",
        "ws.listClients",
        "ws.listEndpoints",
    ]),
    (PERMISSION_PTY_SPAWN, &["pty.spawn", "pty.kill"]),
    (PERMISSION_PTY_IO, &["pty.write", "pty.resize", "pty.ringFetch", "pty.isRunning"]),
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
    ///
    /// 只授予 manifest 声明且在本表内的权限——**没有任何默认授予**（票 02：
    /// 旧形态在此无条件塞进 `storage`，使主库/私有库权限门恒过，
    /// 「manifest 声明即信任」变成了「不声明也有」）。
    /// 词汇表外的声明会被过滤，调用方（宿主激活路径）负责把被过滤项告警出来，
    /// 不允许静默丢弃。
    pub fn grant_permissions(&self, plugin_id: &str, requested: &[String]) -> HashSet<String> {
        let valid_set: HashSet<&str> = VALID_PERMISSIONS.iter().copied().collect();
        let granted: HashSet<String> = requested
            .iter()
            .filter(|p| valid_set.contains(p.as_str()))
            .cloned()
            .collect();

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
        // 票 02：storage 不再是「人人自动持有」的默认位
        assert!(
            !granted.contains(PERMISSION_STORAGE),
            "storage 不得再被默认授予——它曾是主库/私有库权限门恒过的根因"
        );
    }

    /// 票 02 反例：不声明就没有，声明了才给（无默认授予、无隐式扩权）
    #[test]
    fn storage_is_not_granted_unless_declared() {
        let pm = PermissionManager::new();
        pm.grant_permissions("quiet-plugin", &[]);
        assert!(!pm.check("quiet-plugin", PERMISSION_STORAGE));
        assert!(!pm.check("quiet-plugin", PERMISSION_DATABASE_MAIN));
        assert!(!pm.check_api("quiet-plugin", "storage.get"));

        pm.grant_permissions("loud-plugin", &[PERMISSION_STORAGE.to_string()]);
        assert!(pm.check("loud-plugin", PERMISSION_STORAGE));
        assert!(pm.check_api("loud-plugin", "storage.get"));
    }

    /// 票 02：主库面与私有库面是两个位，互不代持
    #[test]
    fn main_db_bit_is_separate_from_plugin_storage_bit() {
        let pm = PermissionManager::new();
        pm.grant_permissions("one-faced", &[PERMISSION_STORAGE.to_string()]);
        assert!(pm.check("one-faced", PERMISSION_STORAGE));
        assert!(
            !pm.check("one-faced", PERMISSION_DATABASE_MAIN),
            "持有 storage 不等于可碰宿主主库"
        );

        pm.grant_permissions("two-faced", &[PERMISSION_DATABASE_MAIN.to_string()]);
        assert!(pm.check("two-faced", PERMISSION_DATABASE_MAIN));
        assert!(
            !pm.check("two-faced", PERMISSION_STORAGE),
            "主库位不应反向附带私有库/KV 能力"
        );
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

    /// 票 01：词汇表由反射表派生，两处必须逐项同序一致（加位只改一张表）
    #[test]
    fn valid_permissions_are_derived_from_vocabulary_table() {
        assert_eq!(
            VALID_PERMISSIONS.len(),
            PERMISSION_VOCABULARY.len(),
            "VALID_PERMISSIONS 与 PERMISSION_VOCABULARY 条数不符"
        );
        for ((ident, value), perm) in PERMISSION_VOCABULARY.iter().zip(VALID_PERMISSIONS.iter()) {
            assert_eq!(value, perm, "反射表与派生词汇顺序不符");
            assert!(
                ident.starts_with("PERMISSION_"),
                "反射表标识符 {ident} 不是 PERMISSION_* 常量"
            );
        }
    }

    /// 票 01：前端确有门禁落点的贡献面权限必须在词汇表内
    ///
    /// `ui:pageToolbar` / `ui:fileHandler` 曾只存在于前端手抄清单：插件声明了却在
    /// 授权时被过滤，宿主侧等于零权限，正是「声明即静默丢弃」的形态。
    #[test]
    fn frontend_contribution_permissions_survive_grant() {
        for perm in [
            PERMISSION_UI_PAGE_TOOLBAR,
            PERMISSION_UI_FILE_HANDLER,
            PERMISSION_UI_SETTINGS,
            PERMISSION_UI_INPUT,
            PERMISSION_UI_SIDEBAR,
            PERMISSION_UI_DIALOG,
        ] {
            let pm = PermissionManager::new();
            let granted = pm.grant_permissions("com.bedcode.contrib", &[perm.to_string()]);
            assert!(granted.contains(perm), "{perm} 声明后被过滤——SDK 词汇表缺位");
        }
    }

    /// 票 01：每条有 API 映射的权限，其映射键必须与词汇表同名（不认陌生键）
    #[test]
    fn api_map_keys_must_be_known_permissions() {
        for (perm, apis) in PERMISSION_API_MAP {
            assert!(
                VALID_PERMISSIONS.contains(perm),
                "PERMISSION_API_MAP 含词汇表外的键 {perm}"
            );
            assert!(!apis.is_empty(), "{perm} 登记了空 API 清单（应直接省略该条目）");
        }
    }
}
