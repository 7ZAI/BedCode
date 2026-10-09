//! 移动 SDK ↔ WIT 真源 ↔ fork crate 消费面**契约对照锁**（票 19 Part A）
//!
//! 「双端机制双份 = 每次机制修复 / ABI 演进两处同步」的漂移税，用源码扫描锁钉死。
//! 数据底座 = **WIT / SDK 源文件本身**（运行时读源解析，不手抄清单——手抄即第二真源）。
//!
//! - **A1 · WIT 接口清单锁**：17 import / 5 export / `events-binary` 可选 /
//!   2 world / ABI 18——增删改名接口即红，ABI bump 必须显式走 ADR 0019 流程先改锁；
//! - **A2 · 权限词汇五同步锁**：SDK `VALID_PERMISSIONS` 静态表（真源）↔ fork crate
//!   re-export 可见集逐字一致 + **无第二份白名单**（五同步点②③的口径是「不另立」
//!   而非「同步维护」）；
//! - **A3 · WIT ↔ host_impl 接线对照**（票 17 批次 2b 后全量启用）：17 接口逐函数
//!   显式对照表（WIT 函数名 ↔ host_impl 实现函数名 ↔ component.rs 委托行），三方
//!   缺一即红；
//! - **A4 · wire 形状单源防副本锁**（`mobile_parallel_copy_shape_lock` 兑现，**口径
//!   按实测修正**）：票 19 原预设「宿主 enums 是 SDK wire 的平行副本」实测不成立
//!   ——5 个 enums 文件全部为宿主自持（`AuthStage`/`AuthPayload` 等全仓唯一副本，
//!   无 SDK 对照面），故本锁钉「**第二副本不得出现**」：契约面（双端 SDK + 双端
//!   wasm-core）出现同名 `enum/struct` 定义即红，合法收口（如迁移 SDK）必须先
//!   在本锁登记白名单并注明去向（票 21 文档联动）。
//!
//! 只扫非注释行：本锁自身的说明与源文件里的记账注释不算违例。

use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 移动 WIT 真源（数据底座）
fn wit_path() -> PathBuf {
    manifest_dir().join("../plugin-sdk-mobile/rust/wit/bedcode.wit")
}

/// 移动 SDK 源根
fn sdk_src() -> PathBuf {
    manifest_dir().join("../plugin-sdk-mobile/rust/src")
}

/// 桌面整核源根（对称结构 / A4 副本扫描面）
fn desktop_core_src() -> PathBuf {
    manifest_dir().join("../../../packages/bedcode-wasm-core/src")
}

/// 桌面 SDK 源根（A4 副本扫描面）
fn desktop_sdk_src() -> PathBuf {
    manifest_dir().join("../../../bedcode-desktop/packages/plugin-sdk-desktop/rust/src")
}

/// 移动宿主源根（A2 第二白名单 / A4 真源在场扫描面）
fn host_src() -> PathBuf {
    manifest_dir().join("../../src-tauri/src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 不可读: {e}（真源文件缺失？）", path.display()))
}

/// 取 WIT `world plugin { ... }` 块（首个以 `}` 单独成行的行收口）
fn world_plugin_block(wit: &str) -> String {
    let start = wit
        .find("world plugin {")
        .expect("WIT 缺 world plugin（v18 结构漂移？）");
    let rest = &wit[start..];
    let end = rest
        .lines()
        .position(|l| l.trim() == "}")
        .expect("world plugin 块无收口}");
    rest.lines().take(end).collect::<Vec<_>>().join("\n")
}

/// 全文件 `interface <name> {` 名集合（按行首解析，不做语法分析）
fn all_interfaces(wit: &str) -> Vec<String> {
    wit.lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("interface ")?
                .trim_end()
                .strip_suffix("{")
        })
        .map(|n| n.trim().to_string())
        .collect()
}

// ==================== A1 · WIT 接口清单锁（ABI 漂移早发现） ====================

/// WIT v18 结构声明表（增删/改名接口 = ABI 演进，先改本表再动 WIT，走 ADR 0019）
const DECLARED_IMPORTS: [&str; 17] = [
    "host-storage",
    "host-database",
    "host-plugin-database",
    "host-events",
    "host-notify",
    "host-http",
    "host-fs",
    "host-config",
    "host-log",
    "host-bus",
    "host-peer",
    "host-mdns",
    "host-platform",
    "host-websocket",
    "host-terminal-stream",
    "host-connection",
    "host-auth",
];

const DECLARED_EXPORTS: [&str; 5] = ["command", "lifecycle", "events", "manifest", "abi"];

/// 全文件接口名集合 = 17 import + 5 export + `events-binary`（可选导出，
/// 宿主实例化后动态探测，不进 plugin world）
const DECLARED_ALL_INTERFACES: [&str; 23] = [
    "host-storage",
    "host-database",
    "host-plugin-database",
    "host-events",
    "host-notify",
    "host-http",
    "host-fs",
    "host-config",
    "host-log",
    "host-bus",
    "host-peer",
    "host-mdns",
    "host-platform",
    "host-websocket",
    "host-terminal-stream",
    "host-connection",
    "host-auth",
    "command",
    "lifecycle",
    "events",
    "events-binary",
    "manifest",
    "abi",
];

#[test]
fn a1_wit_interface_inventory_matches_declared_v18() {
    let wit = read(&wit_path());

    // ① world plugin 的 import / export 集合逐名点名
    let block = world_plugin_block(&wit);
    let mut imports: Vec<String> = Vec::new();
    let mut exports: Vec<String> = Vec::new();
    for line in block.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("import ") {
            imports.push(name.trim_end().trim_end_matches(';').to_string());
        }
        if let Some(name) = t.strip_prefix("export ") {
            exports.push(name.trim_end().trim_end_matches(';').to_string());
        }
    }
    let mut sorted_imports = imports.clone();
    sorted_imports.sort();
    let mut declared_imports = DECLARED_IMPORTS.map(str::to_string);
    declared_imports.sort();
    assert_eq!(
        sorted_imports, declared_imports,
        "world plugin import 集合与 v18 声明表不一致——ABI 演进必须先改本锁（ADR 0019 双端同步流程），禁静默增删"
    );
    let mut sorted_exports = exports.clone();
    sorted_exports.sort();
    let mut declared_exports = DECLARED_EXPORTS.map(str::to_string);
    declared_exports.sort();
    assert_eq!(
        sorted_exports, declared_exports,
        "world plugin export 集合与 v19 声明表不一致——同上，先改锁再改 WIT"
    );

    // ② 全文件接口集合：含可选 events-binary，且桌面独有面（events-ws / events-task /
    //    auth-policy / host-pty / host-task / host-timer / host-process / host-app /
    //    host-crypto / host-api-call）不回流（缺席即由集合相等断言保证）
    let mut interfaces = all_interfaces(&wit);
    interfaces.sort();
    let mut declared_all = DECLARED_ALL_INTERFACES.map(str::to_string);
    declared_all.sort();
    assert_eq!(
        interfaces, declared_all,
        "WIT 全文件接口集合与 v19 声明表不一致（含可选导出面）——先改锁再改 WIT"
    );

    // ③ events-binary 专用 world（SDK 绑定用；宿主动态探测不进 plugin world）
    assert!(
        wit.contains("world plugin-binary {") && wit.contains("export events-binary;"),
        "world plugin-binary / events-binary 可选导出结构漂移"
    );

    // ④ ABI 版本真源：SDK abi.rs 常量与 v18 一致（abi interface 无 form 字段——
    //    一次性切割，不存在 core 形态共存）
    let abi = read(&sdk_src().join("abi.rs"));
    assert!(
        abi.contains("pub const ABI_VERSION: u32 = 18"),
        "SDK ABI_VERSION 与 v18 声明不符（ABI bump 走 ADR 0019：先双端同步、再改本锁）"
    );
    let abi_iface = wit[base_index(&wit, "interface abi {")..].to_string();
    assert!(
        !abi_iface.contains("form:"),
        "移动 abi interface 出现 form 字段（桌面形态回流？ADR 0018 契约独立）"
    );
}

fn base_index(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("WIT 缺 `{needle}`"))
}

// ==================== A2 · 权限词汇五同步锁 ====================

/// 从 SDK `permission.rs` 源码解析 `VALID_PERMISSIONS` 静态表的 `PERMISSION_*` 条目
/// （与 fork crate `permission.rs` 内嵌单测同一解析口径——真源是静态表本身）
fn parse_sdk_permissions() -> std::collections::BTreeSet<String> {
    let source = read(&sdk_src().join("permission.rs"));
    let table_start = source
        .find("static VALID_PERMISSIONS")
        .expect("SDK 权限词汇表缺失（permission.rs 形态漂移？）");
    // 表体只到 `];` 收口——表后的 match 臂 / 文档若含 PERMISSION_ 字样不得过采
    let table_end = source[table_start..]
        .find("\n];")
        .map(|e| table_start + e)
        .expect("SDK 权限词汇表无 `];` 收口");
    let mut perms = std::collections::BTreeSet::new();
    for line in source[table_start..table_end].lines() {
        if let Some(name) = line.trim().strip_prefix("PERMISSION_") {
            if let Some(const_name) = name.strip_suffix(',') {
                perms.insert(format!("PERMISSION_{const_name}"));
            }
        }
    }
    assert!(!perms.is_empty(), "SDK 权限词汇表解析为空：真源形态漂移？");
    perms
}

#[test]
fn a2_permission_vocabulary_single_source_and_no_second_whitelist() {
    // ① fork crate 可见词汇**值集** = SDK 表条目常量的值集
    //    （表条目是 PERMISSION_* 常量名，可见集是权限字符串——经常量定义表换算后
    //    逐字比较；比对缺失/多出/漂移任一即红）
    let source = read(&sdk_src().join("permission.rs"));
    let mut const_values = std::collections::BTreeMap::new();
    for line in source.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("pub const PERMISSION_") {
            if let Some((name, value)) = rest.split_once(": &str = \"") {
                let value = value.trim_end().trim_end_matches("\";").to_string();
                const_values.insert(format!("PERMISSION_{name}"), value);
            }
        }
    }
    let sdk_perms = parse_sdk_permissions();
    let expected_values: std::collections::BTreeSet<String> = sdk_perms
        .iter()
        .map(|c| {
            const_values
                .get(c)
                .unwrap_or_else(|| panic!("SDK 表条目 {c} 缺常量定义（permission.rs 形态漂移？）"))
                .clone()
        })
        .collect();
    let visible: std::collections::BTreeSet<String> =
        bedcode_wasm_core_mobile::permission::VALID_PERMISSIONS
            .iter()
            .map(|p| (*p).to_string())
            .collect();
    assert_eq!(
        expected_values, visible,
        "fork crate 可见词汇集与 SDK 真源不一致（glob re-export 断裂 / 词汇漂移）"
    );

    // ② 注册完备性：SDK 每个 `pub const PERMISSION_*` 定义都必须登记进
    //    VALID_PERMISSIONS 表——「定义了常量却未登记表」= 新权限被 grant 静默
    //    丢弃（五同步点①↔⑤断链，permission.rs 表注释点名的失效模式）
    let consts: std::collections::BTreeSet<String> = const_values.keys().cloned().collect();
    assert_eq!(
        consts, sdk_perms,
        "SDK 权限常量定义集与 VALID_PERMISSIONS 登记表不一致——新增权限必须登记进表，否则 grant 静默丢弃"
    );

    // ② 无第二份白名单（五同步点②③口径）：移动侧打包脚本 / 宿主 / dev-shell
    //    出现独立的 `PERMISSION_*` 常量定义或 `VALID_PERMISSIONS` 集合即红——
    //    词汇只允许从 SDK re-export，不允许另立集合（出现 = 五同步点被拆成两真源）
    let scan_roots = [
        host_src(),
        manifest_dir().join("../../../scripts"),
        manifest_dir().join("../plugin-sdk-mobile/dev-shell/src"),
    ];
    let mut violations: Vec<String> = Vec::new();
    for root in &scan_roots {
        visit_rs(root, &mut |rel, content| {
            for (idx, raw) in content.lines().enumerate() {
                let line = raw.trim_start();
                if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
                    continue;
                }
                let is_def = (line.contains("const PERMISSION_")
                    || line.contains("static VALID_PERMISSIONS"))
                    && (line.contains(':') || line.contains('='));
                if is_def {
                    violations.push(format!("{rel}:{}: {line}", idx + 1));
                }
            }
        });
    }
    assert!(
        violations.is_empty(),
        "移动侧出现第二份权限词汇定义（五同步点②③口径：词汇只从 SDK re-export，不另立集合）——\n{}",
        violations.join("\n")
    );
}

// ==================== A3 · WIT ↔ host_impl 接线对照（2b 后全量启用） ====================

/// 17 接口逐函数显式对照表：`(WIT 接口, 实现文件, [(WIT 函数名, host_impl 实现名)])`
///
/// 显式全表（不派生命名）：`dial-peer → peer_dial` 一类特例靠表逐字钉住；
/// `host-log` 是 component.rs 内联域（tracing 直发 + status_reporter），实现文件
/// 记为 `component.rs`。新增 WIT 函数 = 先改本表再动 WIT（ABI 流程）。
const WIRING_TABLE: &[(&str, &str, &[(&str, &str)])] = &[
    (
        "host-storage",
        "storage.rs",
        &[("get", "storage_get"), ("set", "storage_set"), ("delete", "storage_delete")],
    ),
    (
        "host-database",
        "db.rs",
        &[
            ("execute", "db_execute"),
            ("query", "db_query"),
            ("execute-params", "db_execute_params"),
            ("query-params", "db_query_params"),
            ("execute-batch", "db_execute_batch"),
        ],
    ),
    (
        "host-plugin-database",
        "db.rs",
        &[
            ("execute", "plugin_db_execute"),
            ("query", "plugin_db_query"),
            ("execute-params", "plugin_db_execute_params"),
            ("query-params", "plugin_db_query_params"),
            ("execute-batch", "plugin_db_execute_batch"),
        ],
    ),
    ("host-events", "event.rs", &[("emit", "emit_event")]),
    (
        "host-notify",
        "notify.rs",
        &[
            ("notify", "notify"),
            ("check-permission", "notify_check_permission"),
            ("request-permission", "notify_request_permission"),
            ("vibrate", "notify_vibrate"),
            ("play-sound", "notify_play_sound"),
        ],
    ),
    ("host-http", "http.rs", &[("fetch", "http_fetch")]),
    (
        "host-fs",
        "fs.rs",
        &[
            ("read", "fs_read"),
            ("write", "fs_write"),
            ("copy", "fs_copy"),
            ("delete", "fs_delete"),
            ("exists", "fs_exists"),
            ("request-auth", "fs_request_auth"),
            ("write-media-downloads", "fs_write_media_downloads"),
            ("save-to-document", "fs_save_to_document"),
        ],
    ),
    ("host-config", "config.rs", &[("get", "config_get")]),
    (
        "host-log",
        "component.rs",
        &[
            ("info", "info"),
            ("debug", "debug"),
            ("warn", "warn"),
            ("error", "error"),
            ("mark-plugin-error", "mark_plugin_error"),
        ],
    ),
    (
        "host-bus",
        "bus.rs",
        &[
            ("publish", "bus_publish"),
            ("publish-binary", "bus_publish_binary"),
            ("subscribe", "bus_subscribe"),
            ("subscribe-binary", "bus_subscribe_binary"),
            ("unsubscribe", "bus_unsubscribe"),
        ],
    ),
    (
        "host-peer",
        "peer.rs",
        &[
            ("dial-peer", "peer_dial"),
            ("close", "peer_close"),
            ("respond-consent", "peer_respond_consent"),
            ("list-trusted", "peer_list_trusted"),
            ("revoke-trusted", "peer_revoke_trusted"),
            ("send-files", "peer_send_files"),
            ("respond-transfer", "peer_respond_transfer"),
            ("set-receive-policy", "peer_set_receive_policy"),
            ("pause-transfer", "peer_pause_transfer"),
            ("resume-transfer", "peer_resume_transfer"),
            ("set-shared-roots", "peer_set_shared_roots"),
            ("list-shared-roots", "peer_list_shared_roots"),
            ("browse-directory", "peer_browse_directory"),
            ("pull-files", "peer_pull_files"),
            ("set-download-dir", "peer_set_download_dir"),
            ("start-node", "peer_start_node"),
            ("stop-node", "peer_stop_node"),
            ("active-transfers", "peer_active_transfers"),
            ("collect-outgoing", "peer_collect_outgoing"),
        ],
    ),
    (
        "host-mdns",
        "mdns.rs",
        &[
            ("browse", "mdns_browse"),
            ("stop-browse", "mdns_stop_browse"),
            ("advertise", "mdns_advertise"),
            ("stop-advertise", "mdns_stop_advertise"),
            ("is-advertising", "mdns_is_advertising"),
        ],
    ),
    (
        "host-platform",
        "platform.rs",
        &[
            ("pick-files", "platform_pick_files"),
            ("pick-folder", "platform_pick_folder"),
        ],
    ),
    (
        "host-websocket",
        "ws.rs",
        &[
            ("connect", "ws_connect"),
            ("send-text", "ws_send_text"),
            ("send-binary", "ws_send_binary"),
            ("close", "ws_close"),
            ("is-connected", "ws_is_connected"),
        ],
    ),
    (
        "host-terminal-stream",
        "terminal_stream.rs",
        &[("forward-output", "terminal_stream_forward_output")],
    ),
    (
        "host-connection",
        "connection.rs",
        &[("primary-target", "connection_primary_target")],
    ),
    (
        "host-auth",
        "auth.rs",
        &[
            ("request-pairing", "auth_request_pairing"),
            ("verify-pairing-code", "auth_verify_pairing_code"),
            ("qr-connect", "auth_qr_connect"),
            ("biometric-authenticate", "auth_biometric_authenticate"),
            ("has-credentials", "auth_has_credentials"),
        ],
    ),
];

/// 逐函数实现文件覆盖（默认用行内 `file` 列；跨文件域在此登记——host-events
/// 的 emit 在 event.rs、notify 在 notify.rs）
const FN_FILE_OVERRIDES: &[(&str, &str, &str)] = &[("host-events", "notify", "notify.rs")];

fn impl_file_for(iface: &str, wit_fn: &str, default_file: &str) -> String {
    FN_FILE_OVERRIDES
        .iter()
        .find(|(i, w, _)| *i == iface && *w == wit_fn)
        .map(|(_, _, f)| f.to_string())
        .unwrap_or_else(|| default_file.to_string())
}

/// 解析 WIT：每 interface 块内的函数名集合（`name: func` 行首形态，不做语法分析）
fn wit_functions_per_interface(
    wit: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut map = std::collections::BTreeMap::new();
    let mut current: Option<String> = None;
    for line in wit.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("interface ") {
            current = rest
                .trim_end()
                .strip_suffix('{')
                .map(|n| n.trim().to_string());
            continue;
        }
        if t.starts_with('}') {
            current = None;
            continue;
        }
        if let (Some(iface), Some(colon)) = (&current, t.find(": func")) {
            // find 返回冒号位置，t[..colon] 已排除冒号本身
            let name = t[..colon].trim();
            if !name.is_empty() {
                map.entry(iface.clone())
                    .or_insert_with(std::collections::BTreeSet::new)
                    .insert(name.to_string());
            }
        }
    }
    map
}

#[test]
fn a3_wit_to_host_impl_wiring_matches_full_table() {
    let wit = read(&wit_path());
    let parsed = wit_functions_per_interface(&wit);
    let component = read(&manifest_dir().join("src/manager/runtime/component.rs"));

    // 表覆盖面 == world plugin import 面（新增接口必须先进 A3 表 + A1 声明表）
    let mut table_ifaces: Vec<&str> = WIRING_TABLE.iter().map(|(i, _, _)| *i).collect();
    table_ifaces.sort();
    let mut declared = DECLARED_IMPORTS.map(str::to_string);
    declared.sort();
    assert_eq!(
        table_ifaces, declared,
        "A3 接线表接口面与 v18 import 声明不一致（新接口漏登记 / 多登记）"
    );

    for (iface, file, fns) in WIRING_TABLE {
        // ① WIT 侧：接口的函数集合与表逐字一致（双向——WIT 增/删/改名函数即红）
        let wit_fns = parsed
            .get(*iface)
            .unwrap_or_else(|| panic!("WIT 缺 interface {iface}（A1 应已红；两锁须同改）"));
        let table_fns: std::collections::BTreeSet<String> =
            fns.iter().map(|(w, _)| w.to_string()).collect();
        assert_eq!(
            wit_fns, &table_fns,
            "interface {iface} 的 WIT 函数集与 A3 对照表不一致——ABI 演进先改锁（ADR 0019）"
        );

        // ② 绑定侧：component.rs 有该接口的 Host impl（WIT 接口名 kebab-case，
        //    Rust 模块名 snake_case）
        let rust_iface = iface.replace('-', "_");
        assert!(
            component.contains(&format!(
                "impl bedcode::plugin::{rust_iface}::Host for WasmPluginState"
            )),
            "component.rs 缺 {iface} 的 Host impl（WIT 接口在、接线断）"
        );

        // ③ 实现侧：每个 WIT 函数都有 host_impl 实现名 + component.rs 委托行
        //    （host-log 是内联域：实现与委托都在 component.rs）
        for (wit_fn, impl_fn) in *fns {
            let file = impl_file_for(iface, wit_fn, file);
            let impl_file = if file == "component.rs" {
                component.clone()
            } else {
                read(&manifest_dir().join(format!("src/manager/runtime/host_impl/{file}")))
            };
            assert!(
                impl_file.contains(&format!("fn {impl_fn}(")),
                "{iface}.{wit_fn} 的实现 `{impl_fn}` 在 {file} 缺失（WIT 函数在、实现漂移）"
            );
            if file != "component.rs" {
                assert!(
                    component.contains(&format!("host_impl::{impl_fn}(")),
                    "{iface}.{wit_fn} 的委托行 `host_impl::{impl_fn}` 在 component.rs 缺失（接线断裂）"
                );
            }
        }
    }
}

// ==================== A4 · wire 形状对照对锁 + 单源防副本锁 ====================

/// **平行副本对照对**（票 19 实测认定的真实双份——逐变体锁「在场 + 字段集逐字一致」；
/// 任一侧漂移即红，收口 = 单侧退役 + 本表更新 + 票 21 文档联动）：
/// - `PluginQuestion` / `PluginQuestionOption`：移动宿主 `enums/plugin.rs` ↔ 桌面 SDK `events.rs`
/// - `SessionSummary`：移动宿主 `enums/sumary.rs` ↔ 桌面 SDK `summary.rs`
const PARALLEL_PAIRS: &[(&str, &str, &str)] = &[
    ("PluginQuestion", "plugin.rs", "events.rs"),
    ("PluginQuestionOption", "plugin.rs", "events.rs"),
    ("SessionSummary", "sumary.rs", "summary.rs"),
];

/// **宿主自持单源形状**（实测无 SDK 对照面——`AuthStage`/`AuthPayload` 等全仓唯一
/// 副本）：契约面出现同名定义 = 平行副本回潮，即红
const SINGLE_SOURCE_SHAPES: [&str; 9] = [
    "AuthStage",
    "AuthPayload",
    "CryptoProposal",
    "SessionControlPayload",
    "SessionControlAction",
    "SessionStatus",
    "TaskStatus",
    "SessionConfigSummary",
    "QuickActionSummary",
];

/// 契约面扫描根（这些面出现单源形状的定义 = 回潮）
fn a4_scan_roots() -> Vec<(PathBuf, &'static str)> {
    vec![
        (sdk_src(), "移动 SDK"),
        (desktop_sdk_src(), "桌面 SDK"),
        (desktop_core_src(), "桌面 wasm-core"),
        (manifest_dir().join("src"), "移动 fork crate"),
    ]
}

/// 从 Rust 源码提取 `struct <name> { ... }` 的字段名集（`pub <field>:` 行形态；
/// serde 属性与注释天然被行过滤排除——不含 `pub` 前缀）
fn struct_fields(content: &str, name: &str) -> Option<std::collections::BTreeSet<String>> {
    let start = content.find(&format!("struct {name} {{"))?;
    let body = &content[start..];
    let end = body
        .lines()
        .position(|l| l.trim() == "}")
        .expect("struct 无收口}");
    let mut fields = std::collections::BTreeSet::new();
    for line in body.lines().take(end) {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("pub ") {
            if let Some(colon) = rest.find(':') {
                fields.insert(rest[..colon].trim().to_string());
            }
        }
    }
    Some(fields)
}

/// 在根目录下**按形状定位** `struct <name>`（桌面 SDK 正被共享 lib M3 重组
/// wire/ 目录——对照锁钉形状与字段集，不钉文件路径，抗搬家）
fn find_struct_fields(
    root: &Path,
    name: &str,
) -> Option<(PathBuf, std::collections::BTreeSet<String>)> {
    let found = std::sync::Mutex::new(None);
    visit_rs(root, &mut |rel, content| {
        let mut guard = found.lock().unwrap();
        if guard.is_some() {
            return;
        }
        if let Some(fields) = struct_fields(content, name) {
            *guard = Some((PathBuf::from(rel), fields));
        }
    });
    found.into_inner().unwrap()
}

#[test]
fn a4_wire_shapes_parallel_pairs_match_and_single_source_holds() {
    // ① 平行副本对照对：双侧在场 + 字段名集逐字一致（逐变体对照的机械执行面；
    //    桌面侧按形状定位——M3 正重组 wire/ 目录，锁钉形状不钉路径）
    for (name, mobile_file, _desktop_rel) in PARALLEL_PAIRS {
        let mobile = read(&host_src().join("enums").join(mobile_file));
        let mobile_fields = struct_fields(&mobile, name)
            .unwrap_or_else(|| panic!("宿主 enums/{mobile_file} 缺 struct {name}（真源漂移？）"));
        let (desktop_rel, desktop_fields) = find_struct_fields(&desktop_sdk_src(), name)
            .unwrap_or_else(|| panic!("桌面 SDK 缺 struct {name}（副本被移走？本锁登记去向）"));
        assert_eq!(
            mobile_fields, desktop_fields,
            "wire 形状 {name} 双端副本字段集漂移（移动 enums/{mobile_file} ↔ 桌面 SDK {desktop_rel:?}）——跨端 wire 协议单真源红线（ADR 0022）：改动必须双端同步 + 本锁同批更新"
        );
    }

    // ② 单源形状：契约面无第二副本（出现 = 平行副本回潮；合法迁移先在本锁登记去向）
    let mut violations: Vec<String> = Vec::new();
    for (root, label) in a4_scan_roots() {
        for name in SINGLE_SOURCE_SHAPES {
            let pattern_enum = format!("enum {name} {{");
            let pattern_struct = format!("struct {name} {{");
            visit_rs(&root, &mut |rel, content| {
                for (idx, raw) in content.lines().enumerate() {
                    let line = raw.trim_start();
                    if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
                        continue;
                    }
                    if line.contains(&pattern_enum) || line.contains(&pattern_struct) {
                        violations.push(format!("[{label}] {rel}:{}: {line}", idx + 1));
                    }
                }
            });
        }
    }
    assert!(
        violations.is_empty(),
        "宿主自持 wire 形状在契约面出现第二副本（平行副本回潮——逐变体漂移税复活；合法迁移走本锁登记 + 票 21 收口）：\n{}",
        violations.join("\n")
    );
}

// ==================== 通用遍历 ====================

fn visit_rs(dir: &Path, f: &mut impl FnMut(&str, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_rs(&path, f);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if let Ok(content) = std::fs::read_to_string(&path) {
                f(&rel, &content);
            }
        }
    }
}

// ==================== 通用遍历 ====================
