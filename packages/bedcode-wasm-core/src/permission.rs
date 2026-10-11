//! 权限管理器（Host）——中性自持副本（票 02 M4）
//!
//! Permission Manager
//!
//! 插件权限校验 — 双重校验的后端最终仲裁层
//! 中性自持副本（票 02 M4）：原为桌面 SDK 的 `permission` 模块再导出，现整份
//! 自持于 wasm-core（机制层零 SDK 依赖；移动宿主不经本表仲裁——其 granted
//! 权限面直取 manifest，词汇表不在机制内）
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
// v27 退役（会话引擎下沉票 10）：`terminal:observe`（提交输入行观察面，票 03 删派发点）
// 与 `session:write`（宿主会话写原语，随 `host-session` 整 interface 删除）。
/// 会话读取（前端 `session.*` 窗口原语面）：**票 10 起只剩宿主终端窗口事实**
/// （预测初始网格 / 打开 / 关闭窗口 / 窗口在场查询）——会话数据面与事件面已分别
/// 随票 08 / 09 退役（插件读会话走自家命令通道）。
pub const PERMISSION_SESSION_READ: &str = "session:read";
/// 宿主 server 在册连接清单读取（WIT `host-connection`，票 04，desktop 独有）：枚举宿主
/// WS 连接注册表的原始条目（含 `fingerprint` 设备标识、未认证连接也在册）。
///
/// **为什么单独立位**（票 04 裁决）：原挂 `session:read`，但那回答不了审计问题
/// 「谁能读到别人的连接清单」——连接在册与否与会话读写无关（会话真源已在插件登记域）。
/// 域名与权限位同源：`host-connection` ↔ `connection:read`。
pub const PERMISSION_CONNECTION_READ: &str = "connection:read";
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
pub const PERMISSION_FS_READ: &str = "fs:read";
pub const PERMISSION_FS_WRITE: &str = "fs:write";
/// 系统文件选择器（WIT `host-platform` 的 `pick-files` / `pick-folder` /
/// `pick-folders`）：调起**系统原生**对话框（Windows IFileDialog / Linux
/// xdg-desktop-portal），用户可在**整盘任意位置**浏览与选择。
///
/// **为什么单独立位**（与 `fs:read` / `fs:write` 分域）：
/// - 「能调起选择器」与「能读写任意路径」是两种权力。前者只让插件把用户**已经
///   亲手选中**的路径拿到手（拿到后仍要过 `fs_auth` 授权校验），后者是任意路径
///   读写。合成一位会让「只想让用户挑一个文件传出去」的插件被动获得全盘读取面；
/// - 选择器是**平台交互动作**（系统对话框由用户驱动，宿主不读任何数据），但
///   选择结果落到具体路径，因此**选择结束后必须做路径授权校验**（宿主
///   `fs_auth` 三层校验的第二/第三层）：已授权目录下的选择静默放行，未授权的
///   弹一次授权框；「记住」按**所在目录**落账，使同目录后续选择免弹。
///
/// 声明即信任「本插件会调起系统文件选择器」这一行为面；不代表任意路径读写
/// （那仍是 `fs:read` / `fs:write` 的门）。
pub const PERMISSION_FS_PICK: &str = "fs:pick";
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
/// 宿主加密引擎·AEAD 域（host-crypto，v26，desktop 独有双端偏离）：加解密 / 密钥 / nonce
pub const PERMISSION_CRYPTO_AEAD: &str = "crypto:aead";
/// 宿主加密引擎·非对称/密钥交换域（host-crypto，v26）：X25519（rsa 按需扩展）
pub const PERMISSION_CRYPTO_ASYM: &str = "crypto:asym";
/// 宿主加密引擎·密钥派生域（host-crypto，v26）：HKDF
pub const PERMISSION_CRYPTO_KDF: &str = "crypto:kdf";


/// 权限词汇反射表：`(常量标识符, 权限串)`
///
/// 标识符由 `stringify!` 取自常量本身，与值同源、不可能漂移。有了这一列，宿主测试
/// 就能把「源码里出现的 `PERMISSION_*` 引用」机械还原成权限串，从而断言
/// 「每一条词汇都有真实门禁落点」（见桌面 `src-tauri/src/plugin/permission.rs` 的词汇漂移锁），
/// 而不需要再手抄一份清单。
pub const PERMISSION_VOCABULARY: &[(&str, &str)] = &[
    (stringify!(PERMISSION_TERMINAL_INPUT), PERMISSION_TERMINAL_INPUT),
    (stringify!(PERMISSION_TERMINAL_OUTPUT), PERMISSION_TERMINAL_OUTPUT),
    (stringify!(PERMISSION_SESSION_READ), PERMISSION_SESSION_READ),
    (stringify!(PERMISSION_CONNECTION_READ), PERMISSION_CONNECTION_READ),
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
    (stringify!(PERMISSION_FS_READ), PERMISSION_FS_READ),
    (stringify!(PERMISSION_FS_WRITE), PERMISSION_FS_WRITE),
    (stringify!(PERMISSION_FS_PICK), PERMISSION_FS_PICK),
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
    (stringify!(PERMISSION_CRYPTO_AEAD), PERMISSION_CRYPTO_AEAD),
    (stringify!(PERMISSION_CRYPTO_ASYM), PERMISSION_CRYPTO_ASYM),
    (stringify!(PERMISSION_CRYPTO_KDF), PERMISSION_CRYPTO_KDF),
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
    // 票 08：`terminal.sendInput` 随宿主输入通道（`plugin_terminal_send_input`）
    // 注销——插件写自家会话输入走自有命令通道（不受本表管辖）。
    (PERMISSION_TERMINAL_INPUT, &["terminal.onInput"]),
    (PERMISSION_TERMINAL_OUTPUT, &["terminal.onOutput"]),
    (
        PERMISSION_SESSION_READ,
        &[
            // 票 08：`session.list` / `session.get` 随宿主会话数据命令面注销
            // （插件读会话走自有命令通道 `session.list` / `session.get`）。
            // 票 09：`session.onStatusChange` 随内核状态订阅转接通道注销。
            // 本表剩下的全是**宿主窗口事实**（窗口本体 / 字体测量在宿主，
            // spec D3）：预测初始网格 / 打开 / 关闭宿主终端窗口 / 窗口在场查询。
            "session.predictTerminalSize",
            "session.openTerminal",
            "session.closeTerminal",
            "session.isTerminalOpen",
        ],
    ),
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
    // 票 08（websocket 业务下沉）：`broadcast.sync` 随 host-events.broadcast-sync
    // 退役（插件事件走 bus/emit）——PERMISSION_BROADCAST 不再映射任何宿主方法。
    // `broadcast` 位保留：前端 events.on/emit 仍经 manifest-gen FRONTEND_PERMISSION_RULES
    // 声明它（apiMap 空清单按 api_map_keys_must_be_known_permissions 约定省略条目）。
    (PERMISSION_FS_READ, &["fs.read", "fs.copy"]),
    (PERMISSION_FS_WRITE, &["fs.write", "fs.copy"]),
    // 系统文件选择器（WASM-only：宿主 `host-platform.pick-*`，无前端调用点）。
    // 与权限位同源命名，前缀 `platform.` 区别于 `fs.*`（后者是 host-fs 域）
    (PERMISSION_FS_PICK, &["platform.pickFiles", "platform.pickFolder", "platform.pickFolders"]),
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
    (PERMISSION_CRYPTO_AEAD, &["crypto.aeadEncrypt", "crypto.aeadDecrypt", "crypto.aeadGenerateKey", "crypto.aeadGenerateNonce"]),
    (PERMISSION_CRYPTO_ASYM, &["crypto.keyAgreementGenerate", "crypto.keyAgreementShared"]),
    (PERMISSION_CRYPTO_KDF, &["crypto.kdfDerive"]),
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

#[cfg(all(test, feature = "common-host"))]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// `bedcode-desktop` 根目录（2026-10-08 迁根：本 crate 在根 `packages/bedcode-wasm-core`，
    /// 上两级即仓库根，再进 `bedcode-desktop`）
    fn sdk_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bedcode-desktop")
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "{} 不可读: {e}（生成物缺失？重跑 SDK 的 pnpm run gen:permissions）",
                path.display()
            )
        })
    }

    /// 取一行里所有单引号包裹的字面量（生成物为手写排版，无需处理转义）
    fn quoted_literals(line: &str) -> Vec<String> {
        line.split('\'').skip(1).step_by(2).map(|s| s.to_string()).collect()
    }

    /// 剥掉纯注释行（整行 `//` / `*` / `/*` 前缀，R-12/R-14）
    ///
    /// 漂移锁扫描「源码/生成物里出现权限词即算落点」时，注释/文档里的权限词
    /// 不该算数——纯注释行剥掉后再扫，只有真实代码引用才算门禁落点；生成物解析
    /// 同理：注释行不参与字面量收集（否则注释里的单引号会插入假权限）。
    /// 只剥**整行注释**，不动行内注释与字符串（避免误伤 `"http://..."`）。
    fn strip_comment_lines(text: &str) -> String {
        text.lines()
            .filter(|line| {
                let t = line.trim_start();
                !(t.starts_with("//") || t.starts_with("/*") || t.starts_with("*/") || t.starts_with("*"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 解析前端生成物：`(权限词汇, API 映射表)`
    ///
    /// 严格形状解析（R-14）：生成器输出是固定格式（`  'perm',` 与
    /// `  'perm': ['m1', 'm2'],`），任何漂移（换行、注释插队、多字面量、
    /// 缺 `': [` 形状）**直接报错**而非静默移位解析集合——"格式化/注释变化
    /// 可静默改变集合"正是本锁要防的静默通道。
    fn parse_generated_ts(text: &str) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
        let mut permissions = BTreeSet::new();
        let mut api_map = BTreeMap::new();
        let mut section = "";
        for (idx, raw) in strip_comment_lines(text).lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.starts_with("export const GENERATED_VALID_PERMISSIONS") {
                section = "permissions";
                continue;
            }
            if trimmed.starts_with("export const GENERATED_PERMISSION_API_MAP") {
                section = "apiMap";
                continue;
            }
            if trimmed == "]" || trimmed == "}" || trimmed == "];" || trimmed == ";}" {
                section = "";
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            let literals = quoted_literals(trimmed);
            match section {
                "permissions" => {
                    assert_eq!(
                        literals.len(),
                        1,
                        "permission-vocabulary.ts:{}: 权限段行形状漂移（应恰一条单引号字面量）: {}",
                        idx + 1,
                        raw
                    );
                    permissions.insert(literals[0].clone());
                }
                "apiMap" => {
                    assert!(
                        !literals.is_empty(),
                        "permission-vocabulary.ts:{}: apiMap 段行无字面量: {}",
                        idx + 1,
                        raw
                    );
                    assert!(
                        trimmed.contains("': ["),
                        "permission-vocabulary.ts:{}: apiMap 行形状漂移（应 `'perm': [...]`）: {}",
                        idx + 1,
                        raw
                    );
                    let (key, methods) = (literals[0].clone(), literals[1..].to_vec());
                    api_map.insert(key, methods);
                }
                _ => {}
            }
        }
        (permissions, api_map)
    }

    /// 解析打包 CLI 生成物：`(权限词汇, API 映射表)`
    fn parse_generated_json(text: &str) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
        let value: serde_json::Value =
            serde_json::from_str(text).expect("CLI 生成物 permission-vocabulary.json 不是合法 JSON");
        let permissions = value["permissions"]
            .as_array()
            .expect("生成物缺 permissions 数组")
            .iter()
            .map(|p| p.as_str().expect("权限必须是字符串").to_string())
            .collect();
        let mut api_map = BTreeMap::new();
        if let Some(map) = value["apiMap"].as_object() {
            for (perm, methods) in map {
                api_map.insert(
                    perm.clone(),
                    methods
                        .as_array()
                        .expect("apiMap 值必须是数组")
                        .iter()
                        .map(|m| m.as_str().expect("方法名必须是字符串").to_string())
                        .collect(),
                );
            }
        }
        (permissions, api_map)
    }

    fn sdk_vocabulary() -> BTreeSet<String> {
        VALID_PERMISSIONS.iter().map(|p| (*p).to_string()).collect()
    }

    fn sdk_api_map() -> BTreeMap<String, Vec<String>> {
        PERMISSION_API_MAP
            .iter()
            .map(|(perm, apis)| ((*perm).to_string(), apis.iter().map(|a| (*a).to_string()).collect()))
            .collect()
    }

    /// 递归收集目录下的源码文件
    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("宿主插件目录可读") {
            let path = entry.expect("目录条目可读").path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                out.push(path);
            }
        }
    }

    /// 递归收集目录下全部 `*.json` 文件（R-13：嵌套目录中的 manifest 不得逃逸扫描）
    ///
    /// 跳过 `node_modules` / `target`（第三方依赖与构建产物里的 JSON 不是 plugin
    /// manifest；tsconfig 等带注释的 JSON 也会让解析直接崩掉）
    fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("目录可读") {
            let path = entry.expect("目录条目可读").path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str());
                // node_modules / target：第三方依赖与构建产物；template：脚手架模板
                // （含 `${...}` 占位符，不是合法 JSON 也不是真实 manifest）
                if matches!(name, Some("node_modules" | "target" | "template" | "templates")) {
                    continue;
                }
                collect_json(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }

    // ==================== 三副本集合相等 ====================

    /// 漂移锁①：SDK 真源 == 前端生成物 == 打包 CLI 生成物（集合相等，非包含）
    ///
    /// 生成物由 SDK 的 `gen_permission_vocabulary` 出，正常情况下恒等；本断言存在的
    /// 意义是**手改任一份生成物即转红**，以及生成物缺文件/解析空转不再静默通过。
    #[test]
    fn permission_vocabulary_is_equal_across_all_three_copies() {
        let root = sdk_root();
        let ts = read(&root.join("src/plugin/permission-vocabulary.ts"));
        let json = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));

        let (ts_perms, ts_api) = parse_generated_ts(&ts);
        let (json_perms, json_api) = parse_generated_json(&json);
        let sdk_perms = sdk_vocabulary();

        assert!(
            sdk_perms.len() >= 30,
            "SDK 词汇表实测 {n} 条，少于既有基线——解析或真源出问题",
            n = sdk_perms.len()
        );
        let missing_in_ts: Vec<&String> = sdk_perms.difference(&ts_perms).collect();
        let extra_in_ts: Vec<&String> = ts_perms.difference(&sdk_perms).collect();
        let missing_in_cli: Vec<&String> = sdk_perms.difference(&json_perms).collect();
        let extra_in_cli: Vec<&String> = json_perms.difference(&sdk_perms).collect();
        assert_eq!(
            (missing_in_ts, extra_in_ts, missing_in_cli, extra_in_cli),
            (vec![], vec![], vec![], vec![]),
            "权限词汇三副本漂移（SDK / 前端生成物 / CLI 生成物）"
        );

        // API 映射同样逐字比对：前端 requirePermission 按这张表快速失败。
        // 比对前先按生成器的口径补齐「无 API 方法的权限 → 空数组」——生成器就是把
        // 全部词汇都写进映射表（让前端查表成为全函数），右值只列有方法的权限。
        let mut sdk_api = sdk_api_map();
        for perm in VALID_PERMISSIONS {
            sdk_api.entry((*perm).to_string()).or_default();
        }
        assert_eq!(ts_api, sdk_api, "前端生成物的 PERMISSION_API_MAP 与 SDK 真源不一致");
        assert_eq!(json_api, sdk_api, "CLI 生成物的 PERMISSION_API_MAP 与 SDK 真源不一致");
    }

    /// 漂移锁②：两份生成物自称生成物，且宿主前端确实 import 生成物（不再手抄清单）
    #[test]
    fn generated_copies_are_marked_and_consumed_not_copied() {
        let root = sdk_root();
        let ts = read(&root.join("src/plugin/permission-vocabulary.ts"));
        let json = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));
        assert!(ts.contains("生成物，勿手改"), "前端生成物缺少生成物标注");
        assert!(json.contains("生成物，勿手改"), "CLI 生成物缺少生成物标注");

        let frontend = read(&root.join("src/plugin/permission.ts"));
        assert!(
            frontend.contains("./permission-vocabulary"),
            "前端 permission.ts 必须 import 生成物，而不是自带权限清单"
        );
        for perm in VALID_PERMISSIONS {
            assert!(
                !frontend.contains(&format!("'{perm}'")),
                "前端 permission.ts 仍手抄权限 {perm}"
            );
        }
    }

    // ==================== 每条词汇都有门禁落点 ====================

    /// 漂移锁③：词汇表里没有「无人执行」的死权限位
    ///
    /// 判定：要么 host 侧源码引用了该权限常量/字面量（`check_permission` /
    /// `permission().check`），要么它在前端 API 映射里有方法可门（纯前端贡献面）。
    /// 两者皆无 = 声明了也不会被任何一处执行，正是本轮审计发现的漂移形态。
    #[test]
    fn every_permission_has_an_enforcement_point() {
        // 整核抽出：原扫描 `src-tauri/src/wasm_core`，现 wasm_core 整核在本 crate；
        // 票 02 批次 04：迁宿主的域（路径 B，plugin/auth.rs 的 `auth` 门等）其权限门
        // 落点在宿主 src——**扫描面必须跟着代码走**（`l2_gating_test::L2_SCAN_ROOTS`
        // 同款判据），否则每次域迁移都会把该词汇的落点扫丢（本锁曾因此红）。
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        collect_rs(&manifest_dir.join("src"), &mut files);
        collect_rs(&manifest_dir.join("../../bedcode-desktop/src-tauri/src"), &mut files);
        assert!(files.len() > 20, "宿主插件源码文件数异常，扫描未生效");

        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for file in &files {
            // 本锁自身按名字引用权限串，不能算作门禁落点
            if file.file_name().map(|n| n == "permission.rs").unwrap_or(false) {
                continue;
            }
            // 剥掉纯注释行（R-12）：注释/文档里出现权限词不算门禁落点——
            // 变异判据：把某权限的 check_permission 落点改成注释即转红
            let text = strip_comment_lines(&read(file));
            for (ident, perm) in PERMISSION_VOCABULARY {
                if text.contains(ident) || text.contains(&format!("\"{perm}\"")) {
                    referenced.insert((*perm).to_string());
                }
            }
        }

        let api_gated: BTreeSet<String> = PERMISSION_API_MAP
            .iter()
            .filter(|(_, apis)| !apis.is_empty())
            .map(|(perm, _)| (*perm).to_string())
            .collect();

        for perm in VALID_PERMISSIONS {
            assert!(
                referenced.contains(*perm) || api_gated.contains(*perm),
                "权限 {perm} 没有任何门禁落点（host 源码未引用、前端 API 映射也无对应方法）",
            );
        }
    }

    // ==================== 生产 manifest 无死词汇 ====================

    /// 漂移锁④：桌面生产插件与测试 fixture 的 manifest 只声明合法权限
    ///
    /// SDK 授权时按词汇表过滤，未列入的声明等于没写——本轮审计实测 file-transfer 的
    /// `bus` / `fileservice` / `transfer` 即此类装饰词汇（移动端 `bus` 是合法位，
    /// 属 ADR 0018 双端契约分叉，不在本锁范围）。
    ///
    /// **扫描范围**：目录下**全部** `*.json`（递归，R-13——旧实现只读直接子级，
    /// 嵌套目录里的 manifest 声明词汇表外权限可逃逸），靠「有 `id` 字段」筛出
    /// plugin manifest。夹具按 feature 分文件放（`http.json` / `pty.json` /
    /// `task.json` / `ws.json` + 根 `plugin.json`）；同目录的 `package.json` /
    /// `tsconfig.json` 无 `id` 字段，天然被跳过。
    #[test]
    fn production_manifests_declare_only_known_vocabulary() {
        let root = sdk_root();
        let mut checked = 0usize;
        // 桌面 wasm 应用源码目录 2026-09-25 起为 `wasm-apps/`（旧 `plugins/` rename）
        for dir in ["wasm-apps", "packages"] {
            let base = root.join(dir);
            for entry in fs::read_dir(&base).expect("插件目录可读") {
                let entry_path = entry.expect("目录条目可读").path();
                if !entry_path.is_dir() {
                    continue;
                }
                let mut candidates = Vec::new();
                collect_json(&entry_path, &mut candidates);
                candidates.sort();
                for manifest in candidates {
                    let text = read(&manifest);
                    let value: serde_json::Value = serde_json::from_str(&text)
                        .unwrap_or_else(|e| panic!("{} 不是合法 JSON: {e}", manifest.display()));
                    // 非 plugin manifest（package.json / tsconfig.json）——按 id 筛除
                    if value.get("id").is_none() {
                        continue;
                    }
                    let permissions = value["permissions"]
                        .as_array()
                        .unwrap_or_else(|| panic!("{} 缺 permissions 数组", manifest.display()));
                    for perm in permissions {
                        let perm = perm.as_str().expect("权限必须是字符串");
                        assert!(
                            VALID_PERMISSIONS.contains(&perm),
                            "{} 声明了词汇表外的权限 {perm}（宿主授权时会被静默过滤）",
                            manifest.display()
                        );
                    }
                    checked += 1;
                }
            }
        }
        // 下限 11 = 4 个 wasm 应用 + bench-test + wasi-test + 合集 crate 的 5 份
        // （http/plugin/pty/task/ws）。递归扫描只可能让实测数更多；若变小先查扫描范围
        assert!(
            checked >= 11,
            "实测仅校验了 {checked} 份 manifest（预期 >= 11），扫描范围可疑"
        );
    }

    // ==================== 校验锁确实挂在链上 ====================

    /// 漂移锁⑤：`bedcode-plugin validate` 挂在插件构建链上
    ///
    /// 校验规则写在 CLI 里却没人跑 = 给后人「有锁」的错觉（票 01 实测：CLI 的词汇表
    /// 落后 SDK 8 项正因为它长期不在任何链路上）。
    #[test]
    fn manifest_validation_runs_in_the_plugin_build_chain() {
        let root = sdk_root();
        // 剥注释再断言调用（R-12）：`validateManifest(` 若只出现在注释/字符串里
        // 不算挂在链上——注释里的调用点不能当作强制执行证据
        let build = strip_comment_lines(&read(&root.join("scripts/plugin-build.js")));
        assert!(
            build.contains("validateManifest("),
            "插件构建链必须调用 manifest 校验（scripts/plugin-build.js）"
        );
        let vocabulary = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));
        let validate = read(&root.join("packages/plugin-sdk-desktop/bin/manifest-validate.js"));
        assert!(
            validate.contains("permission-vocabulary.json"),
            "CLI 校验必须读权限词汇生成物，而不是手抄清单"
        );
        assert!(vocabulary.contains("\"permissions\""));
    }
}
