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
    /// 系统多目录选择器（用户取消为空数组；移动端返回 SAF 目录树 URI 列表）
    fn platform_pick_folders(&self) -> Result<Vec<String>, HostError>;
    /// WSL 发行版名列表（v19 函数级追加，票 13）
    ///
    /// 宿主无 WSL（非 Windows / 未安装 / 命令不可用）时返回错误而非空列表——
    /// 空列表与「没有安装任何发行版」不可区分，调用方据错误渲染「未检测到 WSL」。
    fn platform_wsl_distros(&self) -> Result<Vec<String>, HostError>;
    /// 本机可访问的 IPv4 地址列表（v19 函数级追加，票 14）
    ///
    /// 已排除回环与链路本地地址（与宿主原命令 `get_local_ip_addresses` 同口径）。
    /// 无可用地址时返回空数组——「没有可用地址」是合法状态，调用方渲染占位提示；
    /// 这与 `platform_wsl_distros` 的显性报错口径不同（那里空列表与「未安装」不可区分）。
    fn platform_local_ipv4_addresses(&self) -> Result<Vec<String>, HostError>;
    /// 在系统文件管理器中定位并选中文件/目录（v22 函数级追加）
    ///
    /// 参数为绝对路径；路径不存在时宿主**显性报错**（`reveal: path not found: …`）。
    /// 与 `pick-*` 同口径：定位是平台交互动作、不读数据，**不需要权限声明**
    /// （ADR 0022 裁剪线）；本原语替代已退役的宿主命令 `plugin_reveal_in_dir`
    /// 与 `system:open` 权限（`context.system.revealInDir`）。
    fn platform_reveal_in_dir(&self, path: &str) -> Result<(), HostError>;
}
