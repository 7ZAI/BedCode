//! 认证编排命令面退役锁（票 14 阶段 B · 移动端）
//!
//! 配对 / QR / 生物挑战的**流程编排**自宿主 `commands::auth` 迁
//! `com.bedcode.terminal-session` 插件配对域（WIT `host-auth` 触达引擎）：
//! 五个宿主命令（`ws_request_pairing` / `ws_verify_pairing_code` /
//! `ws_authenticate_with_qr` / `ws_authenticate_with_biometric` /
//! `ws_get_auth_status`——最后者零消费者直接退役）与三个流程事件发射 helper
//! （`emit_pairing_request` / `emit_pairing_verified` / `emit_auth_failed`）
//! 退役。
//!
//! 与票 14 阶段 A 锁（本地配对码编排面）分工：那把锁锁「WS 握手时代的死
//! 实现」，本锁锁「活编排的迁出面」——防止宿主命令面被挂回形成绕过插件的
//! 第二条编排路径（编排真源在插件，宿主侧第二份编排 = 双真源）。
//!
//! 保留面（反向断言钉住，防「连引擎一起删后改走旁路」）：
//! - **引擎**（C4）：`auth/manager.rs` 的 HTTP 认证方法与凭据落地；
//! - **投影**：`host_impl/auth.rs` 的 `host-auth` 5 原语（凭据零过境）+
//!   宿主窄读命令 `ws_get_auth_credentials`（前端持久化镜像唯一取数口）；
//! - **插件编排域**：`plugins/terminal-session` 的 `auth.rs` 流程编排 +
//!   命令分派 + manifest `auth` 权限位声明（权限门 fail-closed 的前提）。
//!
//! 只扫非注释行：模块头「为什么迁」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 已退役的宿主编排命令 / 事件发射 helper（函数定义），出现即回接
const RETIRED_SYMBOLS: [&str; 8] = [
    "fn ws_request_pairing(",
    "fn ws_verify_pairing_code(",
    "fn ws_authenticate_with_qr(",
    "fn ws_authenticate_with_biometric(",
    "fn ws_get_auth_status(",
    "fn emit_pairing_request(",
    "fn emit_pairing_verified(",
    "fn emit_auth_failed(",
];

/// `invoke_handler!` 里不得再出现的编排命令注册项
const RETIRED_HANDLER_ENTRIES: [&str; 5] = [
    "commands::auth::ws_request_pairing",
    "commands::auth::ws_verify_pairing_code",
    "commands::auth::ws_authenticate_with_qr",
    "commands::auth::ws_authenticate_with_biometric",
    "commands::auth::ws_get_auth_status",
];

/// 前端不得再出现的退役命令 invoke 字面量
const RETIRED_FRONTEND_LITERALS: [&str; 5] = [
    "'ws_request_pairing'",
    "'ws_verify_pairing_code'",
    "'ws_authenticate_with_qr'",
    "'ws_authenticate_with_biometric'",
    "'ws_get_auth_status'",
];

/// 认证引擎 + 投影 + 窄读命令必须仍在（C4 与 fail-visible 的保留面）
const KEEP_FACE: [(&str, &str); 7] = [
    ("src/auth/manager.rs", "pub async fn request_pairing("),
    ("src/auth/manager.rs", "pub async fn verify_pairing_code("),
    ("src/auth/manager.rs", "pub async fn authenticate_with_qr("),
    ("src/auth/manager.rs", "pub async fn authenticate_with_biometric("),
    ("src/plugin/wasm_runtime/host_impl/auth.rs", "fn auth_verify_pairing_code("),
    ("src/plugin/wasm_runtime/host_impl/auth.rs", "fn auth_has_credentials("),
    ("src/commands/auth.rs", "fn ws_get_auth_credentials("),
];

/// 插件编排域（同仓库 plugins/，相对 bedcode-mobile 根）
const PLUGIN_AUTH_FACE: [(&str, &str); 5] = [
    ("plugins/terminal-session/rust/src/auth.rs", "fn request_pairing("),
    ("plugins/terminal-session/rust/src/auth.rs", "fn verify_pairing_code("),
    ("plugins/terminal-session/rust/src/commands.rs", "\"terminal-session.verify-pairing-code\""),
    ("plugins/terminal-session/rust/src/commands.rs", "\"terminal-session.authenticate-with-biometric\""),
    ("plugins/terminal-session/plugin.json", "\"auth\""),
];

fn mobile_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("mobile root")
        .to_path_buf()
}

/// 逐行扫描指定文件（跳过纯注释行），返回命中的违规记录
fn scan(root: &Path, files: &[(String, String)], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for (group, rel) in files {
        let Ok(content) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in needles {
                if line.contains(needle) {
                    violations.push(format!("{group}/{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }
    violations
}

#[test]
fn retired_auth_orchestration_symbols_are_absent() {
    // 宿主不再持有配对流程编排（命令函数 + 流程事件发射 helper）。
    let root = mobile_root().join("src-tauri");
    let files: Vec<(String, String)> = vec![
        ("src-tauri".to_string(), "src/commands/auth.rs".to_string()),
        ("src-tauri".to_string(), "src/router/event.rs".to_string()),
    ];
    let violations = scan(&root, &files, &RETIRED_SYMBOLS);
    assert!(
        violations.is_empty(),
        "认证编排命令面（票 14 阶段 B 退役）出现回接：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_auth_orchestration_commands_are_not_registered() {
    // 注册面第二道拦截：编排真源在插件，宿主注册项 = 绕过插件的第二条路径。
    let path = mobile_root().join("src-tauri/src/lib.rs");
    let content = std::fs::read_to_string(&path).expect("read lib.rs");
    let mut violations: Vec<String> = Vec::new();
    let mut in_handler = false;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        // 真实形态是 `.invoke_handler(tauri::generate_handler![ … ])`（宏名不带 `!`）
        if line.contains("invoke_handler(") || line.contains("generate_handler![") {
            in_handler = true;
            continue;
        }
        if in_handler {
            if line.starts_with(']') {
                break;
            }
            for needle in RETIRED_HANDLER_ENTRIES {
                if line.starts_with(needle) {
                    violations.push(format!("src/lib.rs:{}: {}", idx + 1, line));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "invoke_handler! 出现已退役的认证编排命令（票 14 阶段 B）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_has_no_retired_auth_command_literal() {
    // 前端是唯一的 invoke 发起方：编排命令字面量必须只以插件命令形态存在。
    let root = mobile_root();
    let mut files: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(root.join("src/composables")).expect("list composables") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".ts") {
            files.push(("src".to_string(), format!("src/composables/{name}")));
        }
    }
    let violations = scan(&root, &files, &RETIRED_FRONTEND_LITERALS);
    assert!(
        violations.is_empty(),
        "前端出现已退役的认证编排命令字面量（票 14 阶段 B）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn auth_engine_projection_and_plugin_domain_stay() {
    // 反向断言：编排迁出 ≠ 引擎迁出。引擎方法（C4）、host-auth 投影（凭据
    // 零过境）、宿主窄读命令、插件编排域与 manifest 权限位缺一不可——少了
    // 任何一面都说明有人用「删面」代替「迁移」，认证链路将断或被旁路。
    let root = mobile_root();
    for (rel, needle) in KEEP_FACE {
        let content = std::fs::read_to_string(root.join("src-tauri").join(rel))
            .unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(
            content.contains(needle),
            "认证保留面缺失：{rel} 中找不到 `{needle}`"
        );
    }
    for (rel, needle) in PLUGIN_AUTH_FACE {
        let content = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(
            content.contains(needle),
            "插件认证编排域缺失：{rel} 中找不到 `{needle}`"
        );
    }
}
