//! host-terminal / terminal-hooks 退役锁（票 15 阶段 B · 移动端）
//!
//! 票 15 阶段 A 把终端消费 UI 域整体迁入 `com.bedcode.terminal-session` 插件
//! 前端后，本批（阶段 B，ABI **v16→17**）把宿主→插件的终端回调/写入面整层退役：
//!
//! - WIT：import `host-terminal`（send）与导出 `terminal-hooks`
//!   （on-terminal-input / on-terminal-output）删除（破坏性收缩——旧产物在
//!   v17 宿主实例化期因缺失 import interface 被点名失败，fail-visible ②）
//! - SDK：`HostTerminal` trait / `TerminalHandler` trait / `terminal_handlers`
//!   扩展点 / `LifecycleContribution.onTerminal*` / `TerminalContribution
//!   .inputHandlers|outputParsers` / TS `TerminalAPI` / `LifecycleAPI.onTerminal*`
//!   整面删除（零消费者：宿主 `terminal_output_activity` 链无生产构造点、
//!   `TerminalAPI.onOutput` 是无发射点的悬挂监听——「文档承诺兑现不了即退役」）
//! - 宿主：`host_impl/terminal.rs`、`component.rs` 的 `host_terminal` Host impl
//!   与 linker 注册、`PluginLifecycleEvent::TerminalInput/TerminalOutput`、
//!   `router/event.rs` 的 `terminal_output_activity` listener、
//!   前端 `LifecycleAPI` terminal 两条
//! - 权限位：`terminal:input` 退役（含 manifest-gen 的 `.terminal` 宽推导线
//!   清理——否则插件源码里的 `.terminal` 子串会把退役权限自动加回 manifest）
//!
//! 保留面（本锁用例 4 反向断言钉住，防止「顺手清光」）：
//! - `host-terminal-stream.forward-output` + `terminal:output` 权限位
//!   （C3 二进制出口，票 12）+ `terminal_stream_gateway` 窄转发
//! - `host-connection.primary-target`（票 13 复用）
//!
//! 只扫非注释行：模块头「为什么退役」的记账段落与本锁自身的说明不算回接。
//! 前端扫描排除 `__tests__/`：测试是本锁的**消费者**（用退役词断言拒绝行为），
//! 不是生产回接面。

use std::path::{Path, PathBuf};

/// WIT 契约不得再出现的退役行（单一事实来源：删了就是删了）
const RETIRED_WIT_LINES: [&str; 4] = [
    "interface host-terminal ",
    "interface terminal-hooks ",
    "import host-terminal;",
    "export terminal-hooks;",
];

/// Rust 面（SDK + 宿主）不得再出现的退役接线符号。
/// 带边界形态：`HostTerminal,` / `impl HostTerminal for` 避免误伤保留的
/// `HostTerminalStream`；`host_terminal::` 避免误伤 `host_terminal_stream::`
const RETIRED_RUST_SYMBOLS: [&str; 9] = [
    "trait HostTerminal ",
    "impl HostTerminal for",
    "HostTerminal,",
    "trait TerminalHandler",
    "fn terminal_handlers(",
    "fn terminal_send(",
    "host_terminal::send",
    "PERMISSION_TERMINAL_INPUT",
    "\"terminal:input\"",
];

/// 前端不得再出现的退役 API / 事件 / 权限词汇
/// （TerminalAPI 整面 + LifecycleAPI terminal 两条 + 退役权限位；
/// `terminal:output` / `terminalOutput` 是保留词汇，不在清单）
const RETIRED_FRONTEND_LITERALS: [&str; 8] = [
    "TerminalAPI",
    "terminal:input",
    "terminal.sendInput",
    "terminal.onInput",
    "terminal.onOutput",
    "onTerminalInput",
    "onTerminalOutput",
    "plugin:lifecycle:terminalInput",
];

/// 保留面必须仍在：本票只摘「宿主↔插件终端回调/写入面」，不摘输出传输面
const RETAINED_FACE: [(&str, &str); 6] = [
    // WIT 保留接口（票 12 的 C3 二进制出口与连接事实原语）
    (
        "../packages/plugin-sdk-mobile/rust/wit/bedcode.wit",
        "interface host-terminal-stream ",
    ),
    (
        "../packages/plugin-sdk-mobile/rust/wit/bedcode.wit",
        "forward-output: func(",
    ),
    (
        "../packages/plugin-sdk-mobile/rust/wit/bedcode.wit",
        "primary-target: func(",
    ),
    // SDK 权限词汇：terminal:output 保留（窄转发权限门复用）
    (
        "../packages/plugin-sdk-mobile/rust/src/permission.rs",
        "pub const PERMISSION_TERMINAL_OUTPUT",
    ),
    (
        "src/terminal_stream_gateway.rs",
        "pub fn forward_output(",
    ),
    (
        "src/plugin/wasm_runtime/host_impl/terminal_stream.rs",
        "fn terminal_stream_forward_output(",
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

/// 递归收集指定扩展名的文件（相对 `base`；跳过 name 排除目录，如 `__tests__`）
fn collect_ext(base: &Path, dir: &Path, ext: &str, exclude_dirs: &[&str], out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if exclude_dirs.contains(&name) {
                continue;
            }
            collect_ext(base, &path, ext, exclude_dirs, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// 逐行扫描指定文件（跳过注释行），返回命中的违规记录。
/// Rust 用 `//`；TS/Vue 另有块注释 `/*` 与续行 `*`——「为什么退役」的记账
/// 注释落在这些行里，与锁自身的说明一样不算回接。
fn scan(root: &Path, files: &[String], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for rel in files {
        let Ok(content) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
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
fn retired_host_terminal_interfaces_are_absent_from_wit() {
    // WIT 是插件契约的单一事实来源：import `host-terminal` 与导出
    // `terminal-hooks` 重新出现 = 退役面被重新接回（ABI v17 破坏性收缩回退）
    let path = mobile_root().join("../packages/plugin-sdk-mobile/rust/wit/bedcode.wit");
    let content = std::fs::read_to_string(&path).expect("read bedcode.wit");
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        for needle in RETIRED_WIT_LINES {
            if line.contains(needle) {
                violations.push(format!("bedcode.wit:{}: {}", idx + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "WIT 出现已退役的 host-terminal / terminal-hooks（票 15 阶段 B，ABI v17）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_host_terminal_rust_wiring_is_absent() {
    // SDK 与宿主两侧的 Rust 接线（trait 定义 / impl / linker 注册 / 权限常量 /
    // 权限字面量）出现即回接——WIT 已删面，SDK/宿主同批删的是被契约排除的死码
    let mut roots: Vec<PathBuf> = vec![mobile_root().join("src")];
    roots.push(mobile_root().join("../packages/plugin-sdk-mobile/rust/src"));
    let mut violations: Vec<String> = Vec::new();
    for root in &roots {
        let mut files: Vec<String> = Vec::new();
        collect_rs(root, root, &mut files);
        assert!(!files.is_empty(), "未收集到任何 .rs 源文件：{}", root.display());
        violations.extend(scan(root, &files, &RETIRED_RUST_SYMBOLS));
    }
    assert!(
        violations.is_empty(),
        "SDK/宿主出现已退役的 host-terminal / terminal-hooks 接线（票 15 阶段 B）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_has_no_retired_terminal_api_or_permission() {
    // 前端 TerminalAPI（sendInput/onOutput）与 LifecycleAPI.onTerminalInput/
    // onTerminalOutput 已整面退役（SDK types.ts 为契约真源）；`terminal:input`
    // 权限词汇随 host-terminal 退役。重新出现 = 悬挂监听 / 退役权限位回接。
    // 排除 `__tests__`：测试文件是本锁的消费者（断言退役词被拒），不是回接面。
    let root = mobile_root().parent().unwrap().join("src");
    let mut files: Vec<String> = Vec::new();
    for ext in ["ts", "vue"] {
        collect_ext(&root, &root, ext, &["__tests__"], &mut files);
    }
    assert!(!files.is_empty(), "未收集到任何前端源文件，扫描路径有误");
    let violations = scan(&root, &files, &RETIRED_FRONTEND_LITERALS);
    assert!(
        violations.is_empty(),
        "前端出现已退役的 TerminalAPI / LifecycleAPI terminal 钩子 / terminal:input 词汇（票 15 阶段 B）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn mobile_terminal_stream_retained_face_stays() {
    // 反向断言：票 15 阶段 B 只摘「宿主↔插件的终端回调/写入面」，输出传输面
    // 必须原样在场——host-terminal-stream.forward-output（C3 二进制出口）+
    // terminal:output 权限位 + terminal_stream_gateway 窄转发 +
    // host-connection.primary-target。缺失 = 有人为了「清空终端面」连
    // 输出通道一起删掉，前端页面将收不到任何终端输出。
    let root = mobile_root();
    for (rel, needle) in RETAINED_FACE {
        let content =
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
        assert!(
            content.contains(needle),
            "终端输出流保留面缺失（票 15 阶段 B 只摘回调面，不摘传输面）：{rel} 中找不到 `{needle}`"
        );
    }
}
