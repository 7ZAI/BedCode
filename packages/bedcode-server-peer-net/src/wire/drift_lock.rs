//! wire 契约词汇的漂移锁：本 crate 自持副本与桌面 SDK 原版逐字一致
//!
//! 能力域脱绑 P4（2026-10-08）：`bedcode-server-peer-net` 的权限位判据字符
//! （`peer`）改为自持副本。双真源（桌面 SDK 原常量 vs 本 crate 副本）以本锁
//! 钉死：任一侧漂移（改值 / 改注释）→ 测红，改动必须双侧同步。
//!
//! 锁读**源文件**比对文本行（与 http 域 `wire::drift_lock` 同款手法）：不依赖
//! 编译期依赖关系，无 feature 的纯引擎态下照常执行。

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

/// [`crate::wire::PERMISSION_PEER`] 与 SDK `permission.rs` 逐字一致
#[test]
fn permission_peer_copy_matches_sdk() {
    let sdk = sdk_src("permission.rs");
    let local = local_wire();
    let sdk_line = extract_line(&sdk, "pub const PERMISSION_PEER");
    let local_line = extract_line(&local, "pub const PERMISSION_PEER");
    assert_eq!(
        sdk_line, local_line,
        "PERMISSION_PEER 自持副本与桌面 SDK 原版漂移——能力域脱绑 P4：双侧必须逐字一致"
    );
}