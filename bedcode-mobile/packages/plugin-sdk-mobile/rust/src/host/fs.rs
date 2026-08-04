//! 宿主能力：文件系统访问（三层授权）

use super::HostError;

/// 文件系统访问
///
/// 权限 + 访问双层校验：
/// 1. 权限位：读需要 `fs:read`，写需要 `fs:write`，复制需要两者
/// 2. 访问校验：路径白名单 → 插件白名单 → 用户弹窗授权（宿主侧 fs_auth）
pub trait HostFs {
    /// 读取文件内容；文件不存在返回 `Ok(None)`
    fn fs_read(&self, path: &str) -> Result<Option<String>, HostError>;

    /// 写入文件（自动创建父目录）
    fn fs_write(&self, path: &str, data: &str) -> Result<(), HostError>;

    /// 复制文件（自动创建目标父目录）
    fn fs_copy(&self, src: &str, dst: &str) -> Result<(), HostError>;

    /// 检查文件是否存在；路径不可访问返回 `Ok(false)`
    fn fs_exists(&self, path: &str) -> Result<bool, HostError>;
}
