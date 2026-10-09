//! host-websocket 客户端域边界锁（票 11 · 移动端）
//!
//! 移动端 `host-websocket` 是桌面同名接口的**客户端子集**（5 函数）：
//! 服务端域 9 函数（插件入站端点）与 `connection-context` 不跟演——移动端
//! 不跑 WS 服务器（ADR 0018 移动端是消费端），权限词汇也只有 `ws:client`
//! 一点（出站连接是 SSRF 面，fail-closed；不与未来服务端能力混位）。
//!
//! 两条 fail-visible 保险：
//! - **跟演锁**：桌面服务端域函数名不得在移动端 WIT 的非注释行再现
//!   （跟演即越界：给消费端开「在宿主上挂端点」的能力）；
//! - **权限词汇锁**：`ws:server` 权限位（常量 / 字面量）不得在移动端
//!   SDK 与宿主的非注释行出现——权限位与 WIT 函数集是同一条边界的两半，
//!   一半漂移即整条边界失守。
//!
//! 只扫非注释行：模块头「为什么不跟演」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 桌面服务端域 + connection-context 的函数名（kebab-case，WIT 形态）
///
/// `broadcast-` 与 `-to-client` 是桌面 `broadcast-*` / `send-*-to-client`
/// 两组通配函数名的片段形态；`connection-context` 是 v28 安全上下文查询。
const DESKTOP_SERVER_DOMAIN_FNS: [&str; 8] = [
    "register-endpoint",
    "unregister-endpoint",
    "list-clients",
    "list-endpoints",
    "close-client",
    "broadcast-",
    "-to-client",
    "connection-context",
];

/// `ws:server` 权限位的两种出现形态：字面量与 Rust 常量名
const WS_SERVER_PERMISSION_NEEDLES: [&str; 2] = ["ws:server", "PERMISSION_WS_SERVER"];

fn mobile_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf().join("..")
}

/// 逐行扫描文件（跳过纯注释行），返回命中 needle 的违规记录
fn scan_file(path: &Path, needles: &[&str]) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut violations = Vec::new();
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
    violations
}

/// 移动端 WIT 不得跟演桌面服务端域（含 `connection-context`）
#[test]
fn mobile_wit_never_grows_websocket_server_domain() {
    let wit = mobile_root().join("packages/plugin-sdk-mobile/rust/wit/bedcode.wit");
    let violations = scan_file(&wit, &DESKTOP_SERVER_DOMAIN_FNS);
    assert!(
        violations.is_empty(),
        "移动端 host-websocket 是客户端子集（5 函数，ADR 0018/0019 不跑 WS 服务器），\
         WIT 出现桌面服务端域痕迹：\n{}",
        violations.join("\n")
    );
}

/// `ws:server` 权限位不得在移动端出现（SDK 权限词汇 + 宿主 ws 域实现 + WS 客户端
/// 引擎 crate）
///
/// 覆盖面随 ADR 0043 抽根更新：ws 域实现的真源自 2026-10-09 起在移动 fork crate
/// （`packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs`），引擎机制在
/// 根 `packages/bedcode-ws-client-engine`（其 `wire.rs` 自持 `ws:client` 权限字面量
/// 副本——服务端词汇若出现在那里，等于给通用引擎装了个「WS 服务器」概念）。
#[test]
fn mobile_never_defines_ws_server_permission() {
    let targets = [
        // 权限词汇真源（SDK）
        mobile_root().join("packages/plugin-sdk-mobile/rust/src/permission.rs"),
        // ws 域实现（移动 fork crate；原指向 `src/plugin/wasm_runtime/host_impl/ws.rs`
        // 的路径自 host_impl 迁入 fork crate 后即陈旧——陈旧路径会让本锁静默扫不到
        // 任何文件，故必须随迁移同步更新）
        mobile_root().join("packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs"),
        // WS 客户端引擎 crate（通用能力域；wire 词汇自持副本 + 引擎主体）
        mobile_root().join("../packages/bedcode-ws-client-engine/src/wire.rs"),
        mobile_root().join("../packages/bedcode-ws-client-engine/src/engine.rs"),
    ];
    let mut violations = Vec::new();
    for path in &targets {
        assert!(
            path.is_file(),
            "边界锁空转：扫描目标不存在 {}（路径随迁移更新后必须同步，否则锁静默失效）",
            path.display()
        );
        violations.extend(scan_file(path, &WS_SERVER_PERMISSION_NEEDLES));
    }
    assert!(
        violations.is_empty(),
        "移动端只有 ws:client 权限位（出站 SSRF 面 fail-closed），\
         ws:server 属桌面服务端域词汇，出现即越界：\n{}",
        violations.join("\n")
    );
}
