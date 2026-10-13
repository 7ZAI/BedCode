//! 通用平台能力域（WIT `host-platform`，ADR 0022 v2）
//!
//! 系统对话框/授权等与领域无关的平台交互。选源对话框本身即用户授权动作，
//! 不叠加权限门。

use crate::host::HostError;

/// 平台能力 trait —— 函数签名与 WIT `host-platform` 一一对应
pub trait HostPlatform {
    /// 宿主平台标识（v21 函数级追加；双端共享产物运行期分支用）
    ///
    /// 无权限门（平台事实不是能力，ADR 0022 裁剪线）；取值由各端宿主定义
    /// （桌面 `desktop` / 移动 `mobile`）。**不可失败**——端值是宿主编译期
    /// 常量，没有运行期失败模态；套 `Result` 会诱使调用方写恒 `Ok` 的假分支
    /// （ADR 0044 T4 对 `emit_event` 的同款修正）。共享产物据此在同一 trait
    /// 的双实现间选择，空/未知取值在 guest 侧 fail-visible。
    fn platform(&self) -> String;
    /// 系统多文件选择器（用户取消为空数组）
    fn platform_pick_files(&self) -> Result<Vec<String>, HostError>;
    /// 系统文件夹选择器（SAF 目录树选择器；用户取消返回空串，成功为树 URI）
    fn platform_pick_folder(&self) -> Result<String, HostError>;
}
