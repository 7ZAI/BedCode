//! 进程创建工具（wasm-core 纯净性收口票 03 / E2-D4：`create_command` 下沉）
//!
//! 原住 `wasm_core::system::process`（Windows 专用进程创建包装）。下沉到基础层
//! 的原因：它被**两个 crate** 消费（`bedcode-wasm-core` 的 `wsl_fs` / `wsl`
//! 引擎面 + `bedcode-pty-engine` 的 Windows kill 路径），放任一侧都会造成跨 crate
//! 依赖（wasm-core 或 pty-engine 互相引用）。放本 crate 后双侧只向下依赖 base，
//! 归属唯一。

use std::process::Command;

/// 创建一个静默执行的外部命令
///
/// Windows 上自动添加 `CREATE_NO_WINDOW` 标志，避免控制台窗口闪现。
/// 其他平台等同于 `Command::new(program)`。
///
/// 注：`mut` 只被 Windows 分支需要（`creation_flags` 取 `&mut self`）；非 Windows
/// 平台必须 allow(unused_mut)，否则该绑定报 unused_mut。
pub fn create_command(program: &str) -> Command {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}