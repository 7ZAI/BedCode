//! 引擎面 system 模块（移动 fork 形态，票 17）
//!
//! 桌面 fork 面原经 `pub use bedcode_server_base::{constants, error, error_boundary}`
//! 复导出（真源在桌面基础层）；移动宿主不依赖 `bedcode-server-base`，故本 crate
//! **自持**插件机制所需的常量子集与错误类型（error 真源 = [`crate::error`]，
//! 本模块仅作路径兼容层——保持 fork 面内 `crate::system::error::*` 引用路径
//! 与桌面逐字一致，减小 fork 漂移面）。
//!
//! 只收录 fork 面实际消费的常量（原值自 bedcode-server-base/constants.rs 逐字
//! 对齐，票 19 抽共享核时随机制一并上提）。

pub mod constants {
    /// 插件开发模式热重载通知
    pub const PLUGIN_DEV_RELOAD: &str = "plugin:dev-reload";
    /// 插件包文件名（zip 安装约定）
    pub const PLUGIN_MANIFEST_FILE: &str = "plugin.json";
    /// WASM 模块文件扩展名
    pub const WASM_FILE_EXT: &str = ".wasm";
    /// 用户安装来源标记（写于插件目录，与移动端约定一致）
    pub const PLUGIN_SOURCE_MARKER: &str = ".bedcode-source";
    /// 来源标记值：本地 zip 安装
    pub const SOURCE_FILE_INSTALL: &str = "file-install";
    /// zip 安装临时目录（用户插件目录下，安装失败/完成后清理）
    pub const PLUGIN_DOWNLOAD_TEMP_DIR: &str = "plugins/_download_tmp";
    /// dev 热重载防抖窗口（毫秒）
    ///
    /// 同一插件在防抖窗口内只触发一次重载，避免 cargo build 连续写入多次触发
    pub const PLUGIN_RELOAD_DEBOUNCE_MS: u64 = 500;
    /// 插件数据目录（app_data_dir 下；插件私有库落点，真源宿主
    /// `system/constants/plugin.rs::PLUGIN_DATA_DIR` 逐字对齐）
    pub const PLUGIN_DATA_DIR: &str = "plugins";
    /// 插件存储文件子目录名（旧 JSON 文件存储迁移扫描面；真源宿主
    /// `system/constants/plugin.rs::PLUGIN_STORAGE_DIR` 同值）
    pub const PLUGIN_STORAGE_DIR: &str = "plugins";
}

// 错误类型路径兼容层：`crate::system::error::AppError` 形态与桌面 fork 面
// 逐字一致；类型真源在 [`crate::error`]（移动 AppError 形状）
pub use crate::error as error;

