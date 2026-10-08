//! wire 契约词汇的漂移锁：本 crate 自持副本与桌面 SDK 原版逐字一致
//!
//! 能力域脱绑 P2（2026-10-08）：`bedcode-discovery-engine` 不再依赖桌面 SDK
//! （`bedcode-plugin-api`），[`crate::wire`] 自持 wire 词汇副本（mDNS 事件名 +
//! topic 拼接规则）。双真源（桌面 SDK 原常量 vs 本 crate 副本）以本锁钉死：
//! 任一侧漂移（改值 / 改定义 / 改注释）→ 测红，改动必须双侧同步。
//!
//! 锁读**源文件**比对文本块（与 http 域 `wire::drift_lock` 同款手法）：不依赖
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
        "TOPIC_NS_SEP 自持副本与桌面 SDK 原版漂移——能力域脱绑 P2：双侧必须逐字一致"
    );
}

/// `owned_topic` 函数块（含注释）与 SDK `host/bus.rs` 逐字一致
///
/// SDK 中 `TOPIC_NS_SEP` 与 `owned_topic` 定义之间隔着互调前缀常量，
/// 本 crate 不复制那些宿主面常量，故拆两块锁。
#[test]
fn owned_topic_copy_matches_sdk() {
    let sdk = sdk_src("host/bus.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 构造属主私有 topic", "/// 解析 topic 的属主");
    let local_block = extract_block(&local, "/// 构造属主私有 topic", "/// 解析 topic 的属主");
    assert_eq!(
        sdk_block, local_block,
        "owned_topic 自持副本与桌面 SDK 原版漂移——能力域脱绑 P2：双侧必须逐字一致"
    );
}

/// mDNS 事件名常量块（`MDNS_FOUND` / `MDNS_LOST`）与 SDK `host/mdns.rs` 逐字一致
#[test]
fn mdns_event_names_copy_matches_sdk() {
    let sdk = sdk_src("host/mdns.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 发现到新实例", "/// 生成属主私有发现事件 topic");
    let local_block = extract_block(&local, "/// 发现到新实例", "/// 生成属主私有发现事件 topic");
    assert_eq!(
        sdk_block, local_block,
        "mDNS 事件名（MDNS_FOUND / MDNS_LOST）自持副本与桌面 SDK 原版漂移——能力域脱绑 P2：双侧必须逐字一致"
    );
}