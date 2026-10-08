//! wire 契约词汇的漂移锁：本 crate 自持副本与桌面 SDK 原版逐字一致
//!
//! 能力域脱绑 P4（2026-10-08）：`bedcode-pty-engine` 不再依赖桌面 SDK
//! （`bedcode-plugin-api`），[`crate::wire`] 自持 wire 词汇副本（topic 规则 /
//! PTY 事件名 / 权限位 / 按键组合线协议）。双真源以本锁钉死：任一侧漂移
//! （改值 / 改定义 / 改注释）→ 测红，改动必须双侧同步。
//!
//! 锁读**源文件**比对文本块（与 http / discovery / peer 域 `wire::drift_lock`
//! 同款手法）：不依赖编译期依赖关系，无 feature 的纯引擎态下照常执行。

use std::fs;
use std::path::PathBuf;

/// 桌面 SDK 的 Rust 源码根（漂移锁比对真源；相对本 crate 根解析）
///
/// 换行符归一化（`\r\n` → `\n`）：仓库双端文件换行风格不一（SDK 侧 CRLF），
/// 「逐字一致」按字符序列比对，换行符风格不算漂移。
fn sdk_src(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-desktop/packages/plugin-sdk-desktop/rust/src")
        .join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("漂移锁读 SDK 源文件失败（{path:?}）：{e}"))
        .replace("\r\n", "\n")
}

/// 提取文件中从 `start_marker`（含）起、到 `end_marker` 首次出现（不含）止的文本块
fn extract_block(source: &str, start_marker: &str, end_marker: &str) -> String {
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("漂移锁：源文件缺起始标记 {start_marker:?}"));
    let tail = &source[start..];
    let end = tail
        .find(end_marker)
        .unwrap_or_else(|| panic!("漂移锁：源文件缺结束标记 {end_marker:?}"));
    tail[..end].to_string()
}

/// 提取文件中以 `line_prefix` 开头的唯一一行（trim 后比对）
fn extract_line(source: &str, line_prefix: &str) -> String {
    source
        .lines()
        .find(|l| l.trim_start().starts_with(line_prefix))
        .unwrap_or_else(|| panic!("漂移锁：源文件缺 {line_prefix:?} 行"))
        .trim()
        .to_string()
}

/// 本 crate `src/wire.rs` 全文（副本比对对象）
fn local_wire() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wire.rs");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("漂移锁读本 crate wire.rs 失败（{path:?}）：{e}"))
        .replace("\r\n", "\n")
}

/// `TOPIC_NS_SEP` 常量块（含注释）与 SDK `host/bus.rs` 逐字一致
#[test]
fn topic_ns_sep_copy_matches_sdk() {
    let sdk = sdk_src("host/bus.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 私有 topic 命名空间分隔符", "/// 互调请求道前缀");
    let local_block = extract_block(&local, "/// 私有 topic 命名空间分隔符", "/// 互调请求道前缀");
    assert_eq!(
        sdk_block, local_block,
        "TOPIC_NS_SEP 自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
    );
}

/// `owned_topic` 函数块（含注释）与 SDK `host/bus.rs` 逐字一致
#[test]
fn owned_topic_copy_matches_sdk() {
    let sdk = sdk_src("host/bus.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 构造属主私有 topic", "/// 解析 topic 的属主");
    let local_block = extract_block(&local, "/// 构造属主私有 topic", "/// 解析 topic 的属主");
    assert_eq!(
        sdk_block, local_block,
        "owned_topic 自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
    );
}

/// PTY 事件常量块（`PTY_EXIT` / `PTY_OUTPUT`，含注释）与 SDK `host/pty.rs` 逐字一致
#[test]
fn pty_event_names_copy_matches_sdk() {
    let sdk = sdk_src("host/pty.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// PTY 进程退出", "/// 生成属主私有事件 topic");
    let local_block = extract_block(&local, "/// PTY 进程退出", "/// 生成属主私有事件 topic");
    assert_eq!(
        sdk_block, local_block,
        "PTY 事件名（PTY_EXIT / PTY_OUTPUT）自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
    );
}

/// 权限位（`PERMISSION_PTY_SPAWN` / `PERMISSION_PTY_IO`）与 SDK `permission.rs` 逐字一致
#[test]
fn pty_permissions_copy_matches_sdk() {
    for (const_name, marker) in [
        ("PERMISSION_PTY_SPAWN", "pub const PERMISSION_PTY_SPAWN"),
        ("PERMISSION_PTY_IO", "pub const PERMISSION_PTY_IO"),
    ] {
        let sdk = sdk_src("permission.rs");
        let local = local_wire();
        let sdk_line = extract_line(&sdk, marker);
        let local_line = extract_line(&local, marker);
        assert_eq!(
            sdk_line, local_line,
            "{const_name} 自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
        );
    }
}

/// `KeyCombo` 生产段（`wire/key.rs`）与 SDK `wire/key.rs` 逐字一致
#[test]
fn key_combo_copy_matches_sdk() {
    let sdk = sdk_src("wire/key.rs");
    let local_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wire/key.rs");
    let local = fs::read_to_string(&local_path)
        .unwrap_or_else(|e| panic!("漂移锁读本 crate wire/key.rs 失败（{local_path:?}）：{e}"))
        .replace("\r\n", "\n");
    let sdk_block = extract_block(&sdk, "//! 按键组合线协议（原宿主", "// ==================== 单元测试 ====================");
    let local_block = extract_block(&local, "//! 按键组合线协议（原宿主", "// ==================== 单元测试 ====================");
    assert_eq!(
        sdk_block, local_block,
        "KeyCombo 生产段自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
    );
}

/// `pty_event_topic` 形状与 SDK 语义等价（owner 命名空间 + 事件名常量拼接）
///
/// SDK 实现调 `host::bus::owned_topic`（已由上方锁钉逐字一致），此处断言本域
/// 对外的形状串（含旧版 `pty:exit` 事件名在 payload 里不会旁路命名空间）。
#[test]
fn pty_event_topic_shape_matches_sdk_semantics() {
    use crate::wire::{PTY_EXIT, PTY_OUTPUT, pty_event_topic};
    assert_eq!(pty_event_topic(PTY_EXIT, "com.x"), "com.x::pty:exit");
    assert_eq!(pty_event_topic(PTY_OUTPUT, "com.x"), "com.x::pty:output");
    assert_ne!(
        pty_event_topic(PTY_EXIT, "a"),
        pty_event_topic(PTY_EXIT, "b"),
        "属主私有命名空间必须隔离"
    );
}