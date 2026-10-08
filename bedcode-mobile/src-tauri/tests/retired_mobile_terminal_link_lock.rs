//! 终端订阅协议客户端退役锁（票 12 · 移动端）
//!
//! 票 12 把宿主侧的**终端订阅协议客户端**整体迁入
//! `com.bedcode.terminal-session` wasm app（D6 选项 A）并退役：
//! `terminal_link.rs`（1,363 行：`TerminalLink` / `TerminalLinkManager` /
//! `TerminalEventSink` / link_io 重连 / special-key 翻译）+ `enums/special_key.rs`
//! + `terminal_*` 前端命令面八命令。协议状态机（subscribe 门控 / ack 节流 /
//! ring_resync 重锚 / session_missing 三振 / 输入计划）归插件——宿主按 ADR 0022
//! 判据命中 **B2/B4**（终端消费是产品语义）。
//!
//! 宿主**保留面**（本锁用例 4 反向断言钉住，防止「顺手清光」）：
//! - `terminal_stream_gateway.rs`：页面 Channel 表 + 零解析窄转发
//!   （ADR 0022 四类薄壳④；Channel 是 Tauri 传输机制，插件无法持有）
//! - `host_impl/terminal_stream.rs`：WIT `forward-output` 权限门（C3 二进制出口）
//! - `host_impl/ws.rs`：`jwt-auth` 代发（C4：token 不落插件）+ 心跳 + auto-reconnect
//! - `host_impl/connection.rs`：`primary-target` 主连接事实读取（票 13 复用）
//!
//! 只扫非注释行：模块头「为什么退役」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 已彻底退役的符号（结构体 / 函数定义 / 宿主侧按键翻译），出现即回接
const RETIRED_SYMBOLS: [&str; 12] = [
    "struct TerminalLink",
    "struct TerminalLinkManager",
    "fn terminal_link_manager(",
    "trait TerminalEventSink",
    "enum LinkPhase",
    "enum LinkExit",
    "async fn connect_once(",
    "async fn link_io(",
    "fn terminal_reconnect_policy(",
    "enum KeyCode",
    "struct KeyCombo",
    "fn to_pty_bytes(",
];

/// `invoke_handler!` 里不得再出现的终端命令注册项
/// （带 `terminal_link::` 前缀：活命令 `terminal_page_subscribe` 在
/// `terminal_stream_gateway::` 下，不会被误伤）
const RETIRED_HANDLER_ENTRIES: [&str; 10] = [
    "terminal_link::terminal_subscribe",
    "terminal_link::terminal_unsubscribe",
    "terminal_link::terminal_page_subscribe",
    "terminal_link::terminal_page_unsubscribe",
    "terminal_link::terminal_unsubscribe_all",
    "terminal_link::terminal_remove",
    "terminal_link::terminal_send_input",
    "terminal_link::terminal_ack_rendered",
    "terminal_link::terminal_get_history",
    "terminal_link::terminal_get_state",
];

/// 前端不得出现的退役 Tauri 命令字面量（协议面已走 plugin_invoke；
/// `terminal_page_subscribe` / `terminal_page_unsubscribe` 是保留的宿主
/// Channel 登记命令，不在退役清单）
const RETIRED_FRONTEND_LITERALS: [&str; 8] = [
    "'terminal_subscribe'",
    "'terminal_unsubscribe'",
    "'terminal_unsubscribe_all'",
    "'terminal_remove'",
    "'terminal_send_input'",
    "'terminal_ack_rendered'",
    "'terminal_get_history'",
    "'terminal_get_state'",
];

/// 宿主保留面（窄转发 + ABI v15 原语）必须仍在：本票只摘协议编排，不摘传输面
const RETAINED_FACE: [(&str, &str); 6] = [
    (
        "src/terminal_stream_gateway.rs",
        "pub fn forward_output(",
    ),
    (
        "src/terminal_stream_gateway.rs",
        "pub fn page_subscribe(",
    ),
    (
        "src/plugin/wasm_runtime/host_impl/terminal_stream.rs",
        "fn terminal_stream_forward_output(",
    ),
    (
        "src/plugin/wasm_runtime/host_impl/ws.rs",
        "jwt_auth == Some(true)",
    ),
    (
        "src/plugin/wasm_runtime/host_impl/ws.rs",
        "fn run_reconnect(",
    ),
    (
        "src/plugin/wasm_runtime/host_impl/connection.rs",
        "fn connection_primary_target(",
    ),
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
fn retired_terminal_link_symbols_are_absent() {
    // 终端订阅协议客户端（链路状态机 / 重连 / 按键翻译）在宿主侧整体退役，
    // 协议逻辑归 `com.bedcode.terminal-session` 插件（票 12）。
    let root = mobile_root().join("src");
    let mut files: Vec<String> = Vec::new();
    collect_rs(&root, &root, &mut files);
    assert!(!files.is_empty(), "未收集到任何 .rs 源文件，扫描路径有误");

    let violations = scan(&root, &files, &RETIRED_SYMBOLS);
    assert!(
        violations.is_empty(),
        "终端订阅协议客户端（票 12 退役）出现回接：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_terminal_link_commands_are_not_registered() {
    // 注册面是第二道拦截：协议命令注册回宿主 = 终端消费面回到宿主（B2/B4 回接）。
    // 实际形态是 `.invoke_handler(tauri::generate_handler![ … ])`：
    // 只匹配 `invoke_handler!` 会永远进不去块（该宏名不带 `!`），锁会退化成恒真
    let path = mobile_root().join("src/lib.rs");
    let content = std::fs::read_to_string(&path).expect("read lib.rs");
    let mut violations: Vec<String> = Vec::new();
    let mut in_handler = false;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
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
        "invoke_handler! 出现已退役的终端协议命令（票 12）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_has_no_retired_terminal_command_literal() {
    // 前端协议面已走 plugin_invoke（插件命令面）；这些 Tauri 命令字面量
    // 重新出现 = 前端绕过插件命令面直连宿主（协议编排回接）。
    let root = mobile_root().parent().unwrap().join("src");
    let mut files: Vec<String> = Vec::new();
    for ext in ["ts", "vue"] {
        collect_ext(&root, &root, ext, &mut files);
    }
    assert!(!files.is_empty(), "未收集到任何前端源文件，扫描路径有误");
    let violations = scan(&root, &files, &RETIRED_FRONTEND_LITERALS);
    assert!(
        violations.is_empty(),
        "前端出现已退役的终端协议命令字面量（票 12）：\n{}",
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
fn mobile_terminal_retained_face_stays() {
    // 反向断言：票 12 只摘「协议编排」，宿主保留面必须原样在场——
    // 窄转发（四类薄壳④）+ ABI v15 原语（jwt-auth / auto-reconnect /
    // primary-target）。缺失 = 有人为了「清空终端面」连传输面一起删掉，
    // 前端输出通道与认证出示将失去宿主承载。
    let root = mobile_root();
    for (rel, needle) in RETAINED_FACE {
        let content =
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(
            content.contains(needle),
            "终端保留面缺失（票 12 只摘编排，不摘传输面）：{rel} 中找不到 `{needle}`"
        );
    }
}
