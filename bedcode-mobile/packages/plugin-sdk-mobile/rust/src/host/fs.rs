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

    /// 删除文件；文件不存在视为成功（幂等）
    fn fs_delete(&self, path: &str) -> Result<(), HostError>;

    /// 写入 MediaStore 公共下载目录（接收方向统一落点，M2）
    ///
    /// 将 src_path（app 私有下载目录中的最终文件）流拷贝到系统公共
    /// Download 目录（API 29+ 零权限、文件管理器可见）；失败（含 API<29
    /// 设备不支持）时调用方应回退私有目录。display_name 为目标文件名，
    /// mime_type 为空串时由宿主按扩展名推断。
    fn fs_write_media_downloads(
        &self,
        src_path: &str,
        display_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError>;

    /// 「保存到…」（M3）：弹系统保存对话框并把 src_path 拷贝到用户选择的位置
    ///
    /// 宿主弹 ACTION_CREATE_DOCUMENT 单文件对话框（用户选位置，默认文件名
    /// suggested_name）→ ContentResolver 流拷贝（写完即达）。用户取消/失败
    /// 返回 Err，调用方应保留 src_path 副本（回退语义）。mime_type 为空串
    /// 时由宿主按扩展名推断。仅在 Android 可用。
    fn fs_save_to_document(
        &self,
        src_path: &str,
        suggested_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError>;

    /// 批量请求目录授权（未授权路径合并为一次用户弹窗，阻塞等待答复）
    ///
    /// 返回 `true` 表示全部路径已获授权（含此前已授权路径）；
    /// `false` 表示用户拒绝或超时。常用于插件 activate 时集中申请
    /// 数据目录访问权，拒绝则激活失败。
    fn fs_request_auth(&self, paths: &[String]) -> Result<bool, HostError>;
}
