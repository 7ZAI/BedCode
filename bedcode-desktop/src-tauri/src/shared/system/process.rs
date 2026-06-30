//! Windows 进程创建工具
//!
//! 在 Windows 上执行外部命令时，默认会弹出控制台窗口。
//! 此模块提供 `create_command` 函数，自动添加 `CREATE_NO_WINDOW` 标志，
//! 确保后台静默执行。

use std::process::Command;

/// 创建一个静默执行的外部命令
///
/// Windows 上自动添加 `CREATE_NO_WINDOW` 标志，避免控制台窗口闪现。
/// 其他平台等同于 `Command::new(program)`。
pub fn create_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}
