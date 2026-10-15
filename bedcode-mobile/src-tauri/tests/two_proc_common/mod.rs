//! 两进程 rig 的测试侧公共模块（移动包内）
//!
//! - [`desktop`]：桌面进程驱动（spawn `desktop-harness` bin + stdin/stdout JSON-RPC）。
//! - [`mobile_ctx`] / [`mobile_world`]：移动侧夹具（真插件产物 + 无头宿主 + 页面窄转发）。
//!   `mobile_ctx.rs` 是 `cross-end-tests/tests/common/mobile_ctx.rs` 的同名副本
//!   （移动-only，逐字一致）；`mobile_world.rs` 是其 `mobile_plugin.rs` 的副本
//!   （差异仅产物路径一行）。
//!
//! ⚠ 本模块**不得**引用桌面 lib / server-crate 的 `common-host` 绑定：两进程形态要求
//! 测试二进制只链移动侧（依赖图 = 本包），否则 feature 统一会把 server-crate 绑定
//! 模块编译进来、移动 whitelist 直接红（spec §5 实测记录）。装配守卫见
//! `tests/two_proc_selfcheck.rs`。

pub mod desktop;

// 共享夹具文件：在只用部分的测试二进制里未用项不算死代码（selfcheck 只用
// DesktopHarness；terminal 场景用全套）。项在对应场景二进制里照常被引用。
#[allow(dead_code)]
pub mod mobile_ctx;

#[allow(dead_code)]
pub mod mobile_world;
