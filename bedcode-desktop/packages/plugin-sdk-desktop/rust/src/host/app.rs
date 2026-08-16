//! 宿主能力：插件随包 CLI 生命周期（v8，host-app）
//!
//! 供调度框架类插件（如计划任务）在激活/停用时安装/卸载自身随包的 CLI
//! （如 bedtask）：复制到用户 bin 目录 + 注册 PATH（Windows 用户级注册表 /
//! unix symlink），全部由宿主侧实现——WASM 插件无注册表等直接通道。
//!
//! 需要 `app:cli` 权限（manifest `permissions` 声明）。
//!
//! 幂等语义：
//! - 重复安装不产生重复 PATH 条目，文件覆盖（升级）
//! - 卸载仅删除本插件的文件与 PATH 条目，保留用户原有项
//! - 应用关闭流程中的卸载自动跳过（CLI 随下次激活重新安装）

use super::HostError;

/// 插件随包 CLI 生命周期管理
pub trait HostApp {
    /// 安装 CLI（幂等），返回安装后的 bin 目录绝对路径
    ///
    /// `file_name`：CLI 文件名（Windows 自动补 .exe，如 "bedtask"）；
    /// 源文件位于插件包目录 `cli/<file-name>`。
    /// `bin_dir`：目标目录，为空用平台默认
    /// （Windows `%LOCALAPPDATA%/com.bedcode.app/bin`；unix `~/.bedcode/bin`）。
    fn cli_install(&self, file_name: &str, bin_dir: &str) -> Result<String, HostError>;

    /// 卸载 CLI（幂等）：删除文件 + 移除仅本插件添加的 PATH 条目
    fn cli_uninstall(&self, file_name: &str, bin_dir: &str) -> Result<(), HostError>;
}
