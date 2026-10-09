//! wire 契约词汇的漂移锁：本 crate 自持副本与移动 SDK 原版逐字一致
//!
//! 引擎抽根（2026-10-09）：`bedcode-ws-client-engine` 不再依赖移动 SDK
//! （`bedcode_plugin_api_mobile`），[`crate::wire`] 自持 wire 词汇副本（事件名 /
//! topic 拼法 / 帧信封 kind 与头长 / 权限字面量）。双真源（移动 SDK 原定义 vs 本 crate
//! 副本）以本锁钉死：任一侧漂移（改值 / 改定义 / 改注释）→ 测红，改动必须双侧同步。
//!
//! 锁读**源文件**比对（与 `bedcode-discovery-engine` 的 `wire::drift_lock` 同款手法）：
//! 不依赖编译期依赖关系，故「零 SDK 依赖」的前提与本锁并存。
//!
//! 换行符归一化（`\r\n` → `\n`）：「逐字一致」按字符序列比对，行尾风格不算漂移。

use std::fs;
use std::path::PathBuf;

/// 移动 SDK 的 Rust 源码根（漂移锁比对真源；相对本 crate 根解析）
fn sdk_src(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-mobile/packages/plugin-sdk-mobile/rust/src")
        .join(rel);
    read_normalized(&path)
}

/// 本 crate `src/wire.rs` 全文（副本比对对象）
fn local_wire() -> String {
    read_normalized(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wire.rs"))
}

fn read_normalized(path: &PathBuf) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("漂移锁读源文件失败（{path:?}）：{e}"))
        .replace("\r\n", "\n")
}

/// 提取文件中从 `start_marker`（含）起、到 `end_marker` 首次出现（不含）止的文本块
///
/// **起始标记用 `rfind`（最后一次出现）**，不是 `find`：本 crate 的 `wire.rs` 模块
/// 文档里逐字引用了这两个标记（说明「漂移锁提取『…』至『…』之间的文本块比对」），
/// 用 `find` 会命中那段**记账注释**而不是真实常量块，抽出的是一句散文——锁于是
/// 比对两段不相干文本，永远红（首版实测即如此）。结束标记仍取 `find`：从真实块
/// 起点出发，结束标记的第一次出现就是块的结尾（记账注释在块之前）。
fn extract_block(source: &str, start_marker: &str, end_marker: &str) -> String {
    let start = source
        .rfind(start_marker)
        .unwrap_or_else(|| panic!("漂移锁：源文件缺起始标记 {start_marker:?}"));
    let tail = &source[start..];
    let end = tail
        .find(end_marker)
        .unwrap_or_else(|| panic!("漂移锁：源文件缺结束标记 {end_marker:?}"));
    tail[..end].to_string()
}

/// 某一行（trim 后）是否出现在文件里
fn has_line(source: &str, needle: &str) -> bool {
    source.lines().any(|l| l.trim() == needle)
}

/// 状态事件名常量块（`WS_OPEN` / `WS_ERROR` / `WS_CLOSE` / `WS_RECONNECT_SCHEDULED`，
/// 含注释）与移动 SDK `host/ws.rs` 逐字一致
#[test]
fn event_name_constants_copy_matches_mobile_sdk() {
    let sdk_block = extract_block(
        &sdk_src("host/ws.rs"),
        "/// 连接建立（客户端域）",
        "/// 生成属主私有状态事件 topic",
    );
    let local_block = extract_block(
        &local_wire(),
        "/// 连接建立（客户端域）",
        "/// 生成属主私有状态事件 topic",
    );
    assert_eq!(
        sdk_block, local_block,
        "状态事件名自持副本与移动 SDK 原版漂移（事件名 / 注释任一改动都必须双侧同步）"
    );
    assert!(
        sdk_block.contains("ws:open") && sdk_block.contains("ws:reconnect-scheduled"),
        "漂移锁空转：提取块不含预期事件名，实得 {sdk_block:?}"
    );
}

/// topic 助手块（`ws_event_topic` / `WS_MESSAGE` / `ws_message_topic`，含注释）
/// 与移动 SDK `host/ws.rs` 逐字一致
#[test]
fn topic_helpers_copy_matches_mobile_sdk() {
    let sdk_block = extract_block(
        &sdk_src("host/ws.rs"),
        "/// 生成属主私有状态事件 topic",
        "// ==================== 帧信封（宿主 → 插件的入站帧） ====================",
    );
    let local_block = extract_block(
        &local_wire(),
        "/// 生成属主私有状态事件 topic",
        "// ==================== 帧信封（宿主 → 插件的入站帧） ====================",
    );
    assert_eq!(
        sdk_block, local_block,
        "属主私有 topic 拼法自持副本与移动 SDK 原版漂移——宿主发布 topic 与插件订阅 topic \
         必须同源（漂移的表现为「订阅了却永远收不到」，静默无报错）"
    );
}

/// 帧信封的形状常量（kind / 头长）与权限字面量：行级比对（含 SDK 侧行必须在场）
#[test]
fn frame_shape_and_permission_lines_match_mobile_sdk() {
    let sdk_ws = sdk_src("host/ws.rs");
    let sdk_permission = sdk_src("permission.rs");
    let local = local_wire();

    // 帧 kind 与头长：信封字节布局的双方约定（宿主构造 / 插件解析）
    for line in [
        "pub const WS_FRAME_KIND_TEXT: u8 = 1;",
        "pub const WS_FRAME_KIND_BINARY: u8 = 2;",
        "pub const WS_FRAME_HEADER_LEN: usize = 3;",
    ] {
        assert!(
            has_line(&sdk_ws, line),
            "移动 SDK host/ws.rs 缺帧形状行：{line}"
        );
        assert!(
            has_line(&local, line),
            "本 crate wire.rs 缺帧形状行（漂移）：{line}"
        );
    }

    // 权限字面量：拒绝文案与 manifest 声明词汇共同引用它
    let permission_line = "pub const PERMISSION_WS_CLIENT: &str = \"ws:client\";";
    assert!(
        has_line(&sdk_permission, permission_line),
        "移动 SDK permission.rs 缺 {permission_line}"
    );
    assert!(
        has_line(&local, permission_line),
        "本 crate wire.rs 的权限字面量与移动 SDK 漂移：{permission_line}"
    );
}
