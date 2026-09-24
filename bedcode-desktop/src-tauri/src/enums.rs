//! Enums Module
//!
//! 公共枚举类型定义
//!
//! **终态 = 引擎级类型 + 传输面契约形状**：按键组合（`special_key`）的 wire 定义
//! 已收编 SDK `bedcode-plugin-api::wire`（会话事件下沉专项票 01），本目录对应文件只
//! re-export，保持 `crate::enums::*` 导入路径不变。`auth`（认证 wire）与
//! `pty_status`（PTY 引擎枚举）仍在宿主定义。**websocket 业务下沉票 08**：会话同步
//! （`sync`/`summary`）与 WS 控制/终端帧（`control`）re-export 已随宿主 `Message`
//! 业务协议退役删除（wire 定义在 SDK 不再被宿主消费；插件 wire 面只剩 `summary`/
//! `key`）。**新增跨端 wire 形状一律进 SDK，不再落在本目录。**

pub mod auth;
pub mod plugin;
pub mod pty_status;
pub mod special_key;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use pty_status::PtySessionStatus;
// 会话状态/类型已归位 `protocol::session`（票 08 线协议域）；此处为兼容 re-export
// 保留 `enums::SessionStatus` / `enums::SessionType` 路径，避免破坏既有 import。
// 新增会话 wire 形状一律放 `protocol/`，不再落在本目录。
pub use crate::protocol::session::{SessionStatus, SessionType};
pub use special_key::{KeyCode, KeyCombo};

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 类型身份锁（编译期）：宿主 `enums::*` 与 SDK 真源必须是**同一个**类型。
    ///
    /// 谁把 wire 定义抄回宿主造出第二份（或改了 re-export 指向），下面的函数指针
    /// 强制转换即编译失败——形状锁因此不可能在「两份定义各自绿」的情况下静默漂移。
    #[test]
    fn host_paths_are_the_same_types_as_sdk_source_of_truth() {
        let _: fn(bedcode_plugin_api::wire::KeyCombo) -> KeyCombo = |s| s;
        let _: fn(bedcode_plugin_api::wire::KeyCode) -> KeyCode = |s| s;
        let _: fn(bedcode_plugin_api::events::PluginQuestion) -> PluginQuestion = |s| s;
        let _: fn(bedcode_plugin_api::events::PluginQuestionOption) -> PluginQuestionOption = |s| s;
    }

    /// 反双份锁（源层面）：垫片文件只允许 `pub use`，不得再落类型定义或实现体。
    ///
    /// 身份锁挡不住「宿主另定义一份、恰好没人把它传给 SDK 类型」的形态（例如新增
    /// 一份 `DesktopSyncPayload` 走旁路），这条按行扫源码把它一起挡在门外。
    #[test]
    fn wire_shim_files_contain_no_definitions() {
        const FORBIDDEN_PREFIXES: &[&str] = &[
            "pub enum",
            "enum ",
            "pub struct",
            "struct ",
            "pub trait",
            "trait ",
            "pub fn",
            "fn ",
            "pub const",
            "const ",
            "impl",
            "#[derive",
            "mod ",
        ];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/enums");
        let mut violations: Vec<String> = Vec::new();
        // 票 08：sync/summary/control 三个垫片随宿主 Message 业务协议退役删除；
        // special_key（宿主 PTY 写入面）与 plugin（共享类型）仍在
        for name in ["special_key.rs", "plugin.rs"] {
            let path = dir.join(name);
            let Ok(content) = std::fs::read_to_string(&path) else {
                violations.push(format!("{name}: 读取失败（re-export 垫片被删除？）"));
                continue;
            };
            for (idx, raw) in content.lines().enumerate() {
                // trim 而非 trim_start：垫片可能是 CRLF，行尾 \r 会破坏 ends_with 判断
                let line = raw.trim();
                // 注释里出现类型名是记账（说明为什么迁走），不算定义
                if line.is_empty() || line.starts_with("//") || line.starts_with("//!") {
                    continue;
                }
                if line.starts_with("pub use") || line.starts_with("}") || line.starts_with('{') {
                    continue;
                }
                // `pub use {` 的多行展开：条目行以标识符开头、以逗号/`};` 结尾
                if line.ends_with(',') || line == "};" {
                    continue;
                }
                if FORBIDDEN_PREFIXES.iter().any(|p| line.starts_with(p)) {
                    violations.push(format!("{}:{}: {}", path.display(), idx + 1, line));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "宿主 enums 的 wire 垫片只允许 re-export（真源在 SDK bedcode-plugin-api::wire）:\n{}",
            violations.join("\n")
        );
    }
}
