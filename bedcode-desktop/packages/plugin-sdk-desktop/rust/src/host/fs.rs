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

    /// 删除文件；文件不存在视为成功（幂等，用于清理场景）
    fn fs_delete(&self, path: &str) -> Result<(), HostError>;

    /// 检查文件是否存在；路径不可访问返回 `Ok(false)`
    fn fs_exists(&self, path: &str) -> Result<bool, HostError>;

    /// 批量请求目录授权（未授权路径合并为一次用户弹窗，阻塞等待答复）
    ///
    /// 返回 `true` 表示全部路径已获授权（含此前已授权路径）；
    /// `false` 表示用户拒绝或超时。常用于插件 activate 时集中申请
    /// 数据目录访问权，拒绝则激活失败。
    fn fs_request_auth(&self, paths: &[String]) -> Result<bool, HostError>;
}
