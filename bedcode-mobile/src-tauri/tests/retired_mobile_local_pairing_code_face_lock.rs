//! 本地配对码编排面退役锁（票 14 · 移动端）
//!
//! 票 14 把宿主侧的**本地配对码编排面**整体退役：
//! `connection/pairing_service.rs`（`PairingService`：生成 / 持有 / 校验消费 / 清除
//! 配对码 + 待配对设备登记）与 `auth/pairing.rs`（`PairingCode` / `PendingDevice` /
//! `PAIRING_CODE_TTL_SECS`），连同前端命令面
//! `system::commands::{generate,get_current,verify,clear}_pairing_code` 四命令与
//! `system/constants::auth::PAIRING_CODE_DIGITS` 常量。
//!
//! 为什么退役而不是「下沉」：移动端自认证 HTTP 化（spec §4.5 六端点）起就**不再是
//! 配对码颁发方**——配对码由桌面端生成并展示，移动端只经
//! `commands::auth::ws_request_pairing` / `ws_verify_pairing_code` 提交。宿主这份
//! 「本地生成 6 位码 + TTL 过期判定 + 单次消费」的实现是 WS 握手时代的遗留，命中
//! ADR 0022 判据 **B1**（产品类型 `PairingCode` / `PendingDevice`）、**B2**（配对码
//! 生命周期编排）、**B5**（TTL 60s 与 6 位码的默认值策略在宿主替插件决定）。
//! 它**没有任何消费者**：四个命令注册在 `invoke_handler!` 里，但前端 `src/`、
//! 插件、Kotlin 侧全仓零 invoke。留着它等于宿主承诺一个与桌面端重复的配对码
//! 颁发面——且它与真实链路（桌面端颁发）无同步通道，是典型的双真源残留。
//!
//! 保留的安全边界（C4，本锁明确不覆盖）：设备身份文件（`device_identity.json`）、
//! JWT 持有与全局 token、`AuthHttpClient`、生物凭证绑定与 Keystore 签名。这些仍在
//! `auth/manager.rs` 与 `auth/http.rs`，用例 3 以反向断言钉住——防止有人为了「清空
//! auth 模块」连引擎一起删掉后改走旁路。
//!
//! 只扫非注释行：模块头「为什么退役」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 已彻底退役的符号（结构体 / 常量 / 函数定义），出现即回接
const RETIRED_SYMBOLS: [&str; 9] = [
    "struct PairingService",
    "struct PairingCode",
    "struct PendingDevice",
    "PAIRING_CODE_TTL_SECS",
    "PAIRING_CODE_DIGITS",
    "fn generate_pairing_code(",
    "fn get_current_pairing_code(",
    "fn clear_pairing_code(",
    "fn verify_and_consume_code(",
];

/// `invoke_handler!` 里不得再出现的本地配对码注册项
/// （不带 `system::commands::` 前缀会误伤 `ws_verify_pairing_code` 等活命令字面量）
const RETIRED_HANDLER_ENTRIES: [&str; 4] = [
    "system::commands::generate_pairing_code",
    "system::commands::get_current_pairing_code",
    "system::commands::verify_pairing_code",
    "system::commands::clear_pairing_code",
];

/// 前端不得出现的退役命令字面量（不带 ws_ 前缀：活命令是 `ws_verify_pairing_code`）
const RETIRED_FRONTEND_LITERALS: [&str; 3] = [
    "generate_pairing_code",
    "get_current_pairing_code",
    "clear_pairing_code",
];

/// 认证引擎面（安全边界 C4）必须仍在：本票只摘编排，不摘引擎
const AUTH_ENGINE_FACE: [(&str, &str); 5] = [
    ("src/auth/manager.rs", "pub struct AuthManager"),
    ("src/auth/manager.rs", "pub async fn request_pairing("),
    ("src/auth/manager.rs", "pub async fn verify_pairing_code("),
    ("src/auth/manager.rs", "pub async fn authenticate_with_qr("),
    ("src/auth/manager.rs", "pub async fn bind_biometric_credential("),
];

fn mobile_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 递归收集目录下所有 `.rs` 文件（相对 `base` 的路径字符串）
fn collect_rs(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(base, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// 逐行扫描指定文件（跳过纯注释行），返回命中的违规记录
fn scan(root: &Path, files: &[String], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for rel in files {
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
                    violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }
    violations
}

#[test]
fn retired_local_pairing_code_symbols_are_absent() {
    // 本地配对码编排面（服务 / 数据结构 / TTL 常量 / 命令函数）在宿主侧整体退役。
    let root = mobile_root().join("src");
    let mut files: Vec<String> = Vec::new();
    collect_rs(&root, &root, &mut files);
    assert!(!files.is_empty(), "未收集到任何 .rs 源文件，扫描路径有误");

    let violations = scan(&root, &files, &RETIRED_SYMBOLS);
    assert!(
        violations.is_empty(),
        "本地配对码编排面（票 14 退役）出现回接：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_local_pairing_code_commands_are_not_registered() {
    // 注册面是第二道拦截：命令函数删掉后，任何一次「加回函数 + 注册」都会让宿主
    // 重新成为配对码颁发方（与桌面端颁发面双真源）。
    let path = mobile_root().join("src/lib.rs");
    let content = std::fs::read_to_string(&path).expect("read lib.rs");
    let mut violations: Vec<String> = Vec::new();
    let mut in_handler = false;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        // 实际形态是 `.invoke_handler(tauri::generate_handler![ … ])`：
        // 只匹配 `invoke_handler!` 会永远进不去块（该宏名不带 `!`），锁会退化成恒真
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
        "invoke_handler! 出现已退役的本地配对码命令（票 14）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_has_no_retired_pairing_command_literal() {
    // 前端是唯一的 invoke 发起方（AGENTS §6 前端零资源访问红线）：本票零前端改动，
    // 因此前端本就无这些字面量；此用例钉住「将来有人重新接线」的情况。
    let root = mobile_root().parent().unwrap().join("src");
    let mut files: Vec<String> = Vec::new();
    for ext in ["ts", "vue"] {
        collect_ext(&root, &root, ext, &mut files);
    }
    let violations = scan(&root, &files, &RETIRED_FRONTEND_LITERALS);
    assert!(
        violations.is_empty(),
        "前端出现已退役的本地配对码命令字面量（票 14）：\n{}",
        violations.join("\n")
    );
}

/// 收集指定扩展名的文件（相对 `base`）
fn collect_ext(base: &Path, dir: &Path, ext: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ext(base, &path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn mobile_auth_engine_face_stays() {
    // 反向断言：票 14 只摘「本地配对码编排」，认证引擎（设备身份 / JWT 持有 /
    // AuthHttpClient / 生物凭证绑定，安全边界 C4）必须原样在场——防止有人连引擎
    // 一起删掉后另起一条不经过既有 auth 模块的旁路。
    let root = mobile_root();
    for (rel, needle) in AUTH_ENGINE_FACE {
        let content = std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(
            content.contains(needle),
            "认证引擎面缺失（票 14 只摘编排，不摘引擎）：{rel} 中找不到 `{needle}`"
        );
    }
}
