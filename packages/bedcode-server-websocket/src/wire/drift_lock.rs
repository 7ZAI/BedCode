//! wire 契约词汇的漂移锁：本 crate 自持副本与桌面 SDK 原版逐字一致
//!
//! 能力域脱绑 P3（2026-10-08）：`bedcode-server-websocket` 不再依赖桌面 SDK
//! （`bedcode-plugin-api`），`[`crate::wire`]` 自持 wire 词汇副本。双真源
//! （桌面 SDK 原常量 vs 本 crate 副本）以本锁钉死：任一侧漂移（改值 / 改定义 /
//! 改注释）→ 测红，改动必须双侧同步。
//!
//! 锁读**源文件**比对文本块（与 `capability_crates_no_product_ids` 同款手法）：
//! 不依赖编译期依赖关系，无 feature 的纯引擎态下照常执行。

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

/// [`crate::wire`] 的 WS_* 状态事件常量定义块（含注释）与 SDK `host/ws.rs` 逐字一致
#[test]
fn ws_constants_copy_matches_sdk() {
    let sdk = sdk_src("host/ws.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 连接建立（客户端域）", "/// 生成属主私有状态事件 topic");
    let local_block = extract_block(&local, "/// 连接建立（客户端域）", "/// 生成属主私有状态事件 topic");
    assert_eq!(
        sdk_block, local_block,
        "WS_* 状态事件常量自持副本与桌面 SDK 原版漂移——能力域脱绑 P3：双侧必须逐字一致"
    );
}

/// [`crate::wire::EndpointAuth`] 定义块（含注释）与 SDK `types.rs` 逐字一致
#[test]
fn endpoint_auth_copy_matches_sdk() {
    let sdk = sdk_src("types.rs");
    let local = local_wire();
    let sdk_block = extract_block(&sdk, "/// 端点认证档位", "/// 一条 HTTP 端点声明");
    let local_block = extract_block(&local, "/// 端点认证档位", "/// 一条 HTTP 端点声明");
    assert_eq!(
        sdk_block, local_block,
        "EndpointAuth 自持副本与桌面 SDK 原版漂移——能力域脱绑 P3：双侧必须逐字一致"
    );
}

/// [`crate::wire::PERMISSION_WS_CLIENT`] / [`crate::wire::PERMISSION_WS_SERVER`]
/// 与 SDK `permission.rs` 逐字一致
#[test]
fn permission_ws_copy_matches_sdk() {
    let sdk = sdk_src("permission.rs");
    let local = local_wire();
    for prefix in [
        "pub const PERMISSION_WS_CLIENT",
        "pub const PERMISSION_WS_SERVER",
    ] {
        let sdk_line = extract_line(&sdk, prefix);
        let local_line = extract_line(&local, prefix);
        assert_eq!(
            sdk_line, local_line,
            "{prefix} 自持副本与桌面 SDK 原版漂移——能力域脱绑 P3：双侧必须逐字一致"
        );
    }
}

/// [`crate::wire::TOPIC_NS_SEP`] / [`crate::wire::owned_topic`] 与 SDK `host/bus.rs`
/// 逐字一致（签名 + 函数体行级比对；SDK 的 API_TOPIC_PREFIX / REPLY_TOPIC_PREFIX
/// 本域不消费，不复制、不比对）
#[test]
fn topic_helpers_copy_matches_sdk() {
    let sdk = sdk_src("host/bus.rs");
    let local = local_wire();
    for prefix in [
        "pub const TOPIC_NS_SEP",
        "pub fn owned_topic",
        "format!(\"{owner}{TOPIC_NS_SEP}{name}\")",
    ] {
        let sdk_line = extract_line(&sdk, prefix);
        let local_line = extract_line(&local, prefix);
        assert_eq!(
            sdk_line, local_line,
            "{prefix} 自持副本与桌面 SDK 原版漂移——能力域脱绑 P3：双侧必须逐字一致"
        );
    }
}
