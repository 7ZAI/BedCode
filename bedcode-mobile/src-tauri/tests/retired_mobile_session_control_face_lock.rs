//! 会话控制面退役锁（票 13 · 移动端）+ 结构锁迁移
//!
//! 会话控制（list / start / stop / remove / input-HTTP）自宿主
//! `session::http::SessionHttpClient` + `session::SessionManager` +
//! `commands::session`（三命令）整体迁 `com.bedcode.terminal-session` 插件：
//! 插件经 host-http（`jwtAuth` 宿主代注 Bearer，token 不落插件，C4）直连桌面
//! `/api/sessions*`；宿主 `ws_disconnect` 的「停活跃会话」死分支（活跃态簿记
//! 只在零消费者的死命令路径写入 ⇒ 恒 None）同批退役。
//!
//! 与票 12 锁（`retired_mobile_terminal_link_lock`）分工：那把锁锁终端订阅
//! 协议客户端迁出；本锁锁**会话控制客户端**迁出——防止宿主回接第二份会话
//! 控制路径（客户端真源在插件，宿主侧第二份 = 双真源）。
//!
//! 结构锁迁移：原 `src/session/http.rs` 的三个全 src 结构锁（旧 WS 信封命令名
//! 零残留 / WS 帧级加密零残留 / 控制面迁移文件无信封引用）随文件删除迁入本
//! 文件，覆盖不缩水。
//!
//! 保留面（反向断言钉住）：host-http `jwtAuth` 代注裁决、
//! 插件 session 域五命令 + manifest `network:http`、前端 `sessionCommands.ts`。
//! （`host_impl/terminal.rs` 过渡实现原在保留面，已随票 15 阶段 B 整面退役移出。）
//!
//! 只扫非注释行：模块头「为什么迁」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

fn mobile_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("mobile root")
        .to_path_buf()
}

fn src_tauri_root() -> PathBuf {
    mobile_root().join("src-tauri")
}

/// 递归收集指定扩展名的文件（绝对路径）
fn collect_files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
}

// ==================== 退役面定义 ====================

/// 已退役的宿主会话控制符号（出现即回接）
const RETIRED_SYMBOLS: [&str; 12] = [
    "SessionHttpClient",
    "SessionManager",
    "SESSION_MANAGER",
    "get_session_manager(",
    "fn session_base_url(",
    "fn ws_start_session(",
    "fn ws_stop_session(",
    "fn ws_remove_session(",
    "fn parse_session_list(",
    "fn parse_start_session_id(",
    "fn parse_ok_envelope(",
    "SESSION_NAME_ID_PREFIX_LEN",
];

/// `invoke_handler!` 里不得再出现的会话命令注册项
const RETIRED_HANDLER_ENTRIES: [&str; 3] = [
    "commands::session::ws_start_session",
    "commands::session::ws_stop_session",
    "commands::session::ws_remove_session",
];

/// 前端（宿主 `src/**`，含测试）不得再出现的退役函数/命令字面量
const RETIRED_FRONTEND_LITERALS: [&str; 8] = [
    "'ws_start_session'",
    "'ws_stop_session'",
    "'ws_remove_session'",
    "httpListSessions",
    "httpStartSession",
    "httpStopSession",
    "httpRemoveSession",
    "httpSendSessionInput",
];

/// 旧 WS 信封命令名（票 04 删除清单；原锁存于 `session/http.rs`，随迁本文件）
const RETIRED_ENVELOPE_COMMAND_NAMES: [&str; 8] = [
    "ws_load_sessions",
    "ws_send_input_async",
    "get_terminal_ws_info",
    "ws_load_session_configs",
    "ws_join_session",
    "ws_resize_terminal",
    "ws_send_message",
    "ws_send_and_wait",
];

/// WS 帧级链路加密退役符号（票 06；原锁存于 `session/http.rs`，随迁本文件）
const RETIRED_WS_CRYPTO: [&str; 8] = [
    "install_link_crypto",
    "install_event_crypto",
    "extract_crypto_echo",
    "EVENT_WS_AUTH_TIMEOUT_MS",
    "LINK_CRYPTO_CHANNEL_EVENT",
    "is_event_encryption_active",
    "encrypt_ws_event",
    "ClientWsCrypto",
];

/// 会话控制新面必须仍在（迁移正确性 + fail-visible 的前提）。
/// 注：`host_impl/terminal.rs`（host-terminal 过渡实现）原在保留面——票 15 阶段 B
/// 已整面退役该文件（ABI 17），其归属归票 15 的锁，不再由本锁钉住。
/// 票 17 批次 2b：jwtAuth 代注裁决真源迁入 fork crate（wasm_host.rs → host_api/http_engine.rs），
/// 锁改钉新真源位置。
const KEEP_HOST_FACE: [(&str, &str); 2] = [
    ("../packages/bedcode-wasm-core/src/host_api/http_engine.rs", "request.get(\"jwtAuth\")"),
    ("../packages/bedcode-wasm-core/src/host_api/http_engine.rs", "fn resolve_jwt_auth_header("),
];

/// 插件会话控制域（同仓库 plugins/，相对 bedcode-mobile 根）
const PLUGIN_SESSION_FACE: [(&str, &str); 7] = [
    ("wasm-apps/terminal-session/rust/src/session.rs", "fn list_sessions("),
    ("wasm-apps/terminal-session/rust/src/session.rs", "fn send_http_input("),
    ("wasm-apps/terminal-session/rust/src/session.rs", "\"jwtAuth\": true"),
    ("wasm-apps/terminal-session/rust/src/commands.rs", "\"terminal-session.list-sessions\""),
    ("wasm-apps/terminal-session/rust/src/commands.rs", "\"terminal-session.send-http-input\""),
    ("wasm-apps/terminal-session/plugin.json", "\"network:http\""),
    ("wasm-apps/terminal-session/plugin.json", "\"terminal-session.start-session\""),
];

/// 宿主前端命令面封装必须仍在
const FRONTEND_FACE: [(&str, &str); 3] = [
    ("src/plugin/sessionCommands.ts", "'terminal-session.list-sessions'"),
    ("src/plugin/sessionCommands.ts", "'terminal-session.stop-session'"),
    ("src/composables/useMobileConnection.ts", "sessionCommands."),
];

// ==================== 通用扫描 ====================

/// 逐行扫描文件（跳过纯注释行），返回违规记录
fn scan_files(files: &[PathBuf], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for path in files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in needles {
                if line.contains(needle) {
                    violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                }
            }
        }
    }
    violations
}

fn collect_rs_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(&src_tauri_root().join("src"), "rs", &mut files);
    files
}

// ==================== 锁 ====================

#[test]
fn retired_session_control_symbols_are_absent() {
    // 宿主不再持有会话控制客户端 / 本地簿记 / 三命令（真源在插件）。
    let files = collect_rs_files();
    let checked = files.len();
    let violations = scan_files(&files, &RETIRED_SYMBOLS);
    assert!(
        checked > 50,
        "结构锁扫描范围异常：仅扫到 {checked} 个 .rs 文件"
    );
    assert!(
        violations.is_empty(),
        "会话控制面（票 13 退役）出现回接：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_session_commands_are_not_registered() {
    // 注册面第二道拦截：真源在插件，宿主注册项 = 绕过插件的第二条路径。
    let path = src_tauri_root().join("src/lib.rs");
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
        "invoke_handler! 出现已退役的会话命令（票 13）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_has_no_retired_session_control_literal() {
    // 前端是唯一 invoke 发起方：会话控制字面量必须只以插件命令形态存在
    // （含测试文件——避免测试回接退役面形成假绿）。
    let root = mobile_root().join("src");
    let mut files = Vec::new();
    collect_files(&root, "ts", &mut files);
    collect_files(&root, "vue", &mut files);
    let violations = scan_files(&files, &RETIRED_FRONTEND_LITERALS);
    assert!(
        violations.is_empty(),
        "宿主前端出现已退役的会话控制字面量（票 13）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn session_control_new_face_stays() {
    // 反向断言：迁出 ≠ 功能消失。host-http jwtAuth 代注裁决、host-terminal
    // 过渡实现（票 15 退役整面）、插件会话域五命令 + network:http 权限、
    // 前端命令面封装缺一不可。
    let root = mobile_root();
    for (rel, needle) in KEEP_HOST_FACE {
        let content = std::fs::read_to_string(root.join("src-tauri").join(rel))
            .unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(content.contains(needle), "宿主保留面缺失：{rel} 中找不到 `{needle}`");
    }
    for (rel, needle) in PLUGIN_SESSION_FACE {
        let content = std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(content.contains(needle), "插件会话控制域缺失：{rel} 中找不到 `{needle}`");
    }
    for (rel, needle) in FRONTEND_FACE {
        let content = std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(content.contains(needle), "前端命令面缺失：{rel} 中找不到 `{needle}`");
    }
}

// ==================== 结构锁迁移（原 session/http.rs，覆盖不缩水） ====================

#[test]
fn retired_envelope_command_names_have_no_production_hits() {
    // 旧 WS 信封命令名（票 04 删除清单）：Host src/ 生产代码零命中。
    let files = collect_rs_files();
    let checked = files.len();
    let violations = scan_files(&files, &RETIRED_ENVELOPE_COMMAND_NAMES);
    assert!(
        checked > 50,
        "结构锁扫描范围异常：仅扫到 {checked} 个 .rs 文件"
    );
    assert!(
        violations.is_empty(),
        "旧信封命令名残留：\n{}",
        violations.join("\n")
    );
}

#[test]
fn ws_link_crypto_has_no_production_residue() {
    // WS 帧级链路加密退役后（票 06），生产 src/ 下不得回接任何加密符号/通道名。
    let files = collect_rs_files();
    let checked = files.len();
    let violations = scan_files(&files, &RETIRED_WS_CRYPTO);
    assert!(
        checked > 50,
        "结构锁扫描范围异常：仅扫到 {checked} 个 .rs 文件"
    );
    assert!(
        violations.is_empty(),
        "WS 帧级加密已退役，出现回接：\n{}",
        violations.join("\n")
    );
}

// 注：原第三把随迁结构锁 `migrated_control_plane_has_no_envelope_usage`（控制面
// 迁移文件实现段零信封引用，目标 `host_impl/terminal.rs`）已随票 15 阶段 B 整面
// 退役该文件而移除——锁对象不存在，锁语义自然消亡；信封符号的零残留由上方两把
// 全 src 扫描锁继续覆盖。
