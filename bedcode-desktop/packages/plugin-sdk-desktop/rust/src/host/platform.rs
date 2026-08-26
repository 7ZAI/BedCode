//! 通用平台能力域（WIT `host-platform`，ADR 0022 v2）
//!
//! 系统对话框/授权等与领域无关的平台交互。选源对话框本身即用户授权动作，
//! 不叠加权限门。

use crate::host::HostError;

/// 平台能力 trait —— 函数签名与 WIT `host-platform` 一一对应
pub trait HostPlatform {
    /// 系统多文件选择器（用户取消为空数组）
    fn platform_pick_files(&self) -> Result<Vec<String>, HostError>;
    /// 系统文件夹选择器（用户取消返回空串；移动端为 SAF 目录树 URI）
    fn platform_pick_folder(&self) -> Result<String, HostError>;
}
