//! 宿主能力：文件系统访问（三层授权）

use serde::{Deserialize, Serialize};
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

    /// 目录直读（v19 追加，票 03 文件浏览域）：返回 `[{name, nodeType}]`
    /// （nodeType ∈ "folder" | "file" | "other"，DirEntry::file_type 语义，
    /// symlink 为 "other"）。权限 `fs:read` + fs_auth 三层校验。
    fn fs_read_dir(&self, path: &str) -> Result<Vec<FsDirEntry>, HostError>;

    /// canonicalize 绝对路径（v19 追加）；路径不存在返回 `Ok(None)`。
    /// 供 `../` 穿越与 symlink 逃逸的 containment 判定。
    fn fs_canonicalize(&self, path: &str) -> Result<Option<String>, HostError>;

    /// 文件元数据（v19 追加）：`{size, isFile, isDir}`；不存在返回 `Ok(None)`。
    /// 供文件大小上限判定（与宿主 file-content 的 2MB 语义一致）。
    fn fs_stat(&self, path: &str) -> Result<Option<FsStat>, HostError>;
}

/// 目录条目（v19：host-fs read-dir）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsDirEntry {
    pub name: String,
    pub node_type: FsNodeType,
}

/// 节点类型（DirEntry::file_type 语义）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FsNodeType {
    Folder,
    File,
    /// symlink / 特殊条目（宿主 scan_dir 跳过非目录非文件条目）
    Other,
}

/// 文件元数据（v19：host-fs stat）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsStat {
    pub size: u64,
    pub is_file: bool,
    pub is_dir: bool,
}
