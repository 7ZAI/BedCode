//! SAF 存储访问抽象（主 seam，唯一新增 seam）
//!
//! 移动端文件传输 SAF 化改造（spec-mobile-saf-storage）的核心接口：
//! Rust 侧全部 SAF 编排逻辑依赖 [`SafIo`] trait，Kotlin [`SafTransferPlugin`]
//! 仅实现 ContentResolver/DocumentsContract 系统调用转发（不可测薄壳）；
//! 宿主测试注入 fake 实现覆盖编排逻辑。
//!
//! 方法与 spec「Implementation Decisions」对应：
//! - [`SafIo::list_tree`]：列目录树条目（替代 std::fs::read_dir）
//! - [`SafIo::read_to_cache`]：中转复制（Relay Copy）——SAF 源 → app 私有
//!   cache 的顺序流复制（512KB 缓冲、OpenableColumns.SIZE 预检）。顺序流
//!   无 offset 语义，不可断点续传；进度/取消经 [`SafIo::copy_status`] /
//!   [`SafIo::cancel_copy`] 轮询式桥接（WASM host fn 同步上下文无法承载
//!   回调通道，轮询是跨桥最简编码）
//! - [`SafIo::check_authorized`]：树授权有效性检测（持久化授权回收检测）
//! - [`SafIo::write_media_downloads`]：MediaStore 落点（M2 落地版）——接收方向
//!   （移动端下载 + 桌面端推送）统一落系统公共下载目录，失败由调用方回退私有目录

use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// SAF 目录树条目（Kotlin listTreeChildren 返回，wire 为 camelCase）
///
/// serde camelCase：经 Tauri command 返回前端，字段名必须匹配 SDK SafEntry
/// 契约（isDir/documentId）——此前缺 rename 导致 is_dir/document_id 序列化为
/// snake_case，前端 isDir/documentId 恒 undefined：目录被当文件（图标全
/// fallback 为通用文件图标）、子目录无法进入
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafEntry {
    /// 条目名
    pub name: String,
    /// 是否目录
    pub is_dir: bool,
    /// 文件大小（字节；目录/未知为 0）
    pub size: i64,
    /// MIME 类型（可空串）
    pub mime: String,
    /// 条目 document URI（content://.../document/...）
    pub uri: String,
    /// 条目 document id（子目录遍历用）
    pub document_id: String,
}

/// 中转复制启动结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafCopyHandle {
    /// 复制句柄 id（copy_status / cancel_copy 用）
    pub copy_id: String,
    /// cache 落盘绝对路径（复制完成后即 enqueue_upload 的 local_path）
    pub dest_path: String,
}

/// 中转复制进度快照
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafCopyStatus {
    /// 复制句柄 id
    pub copy_id: String,
    /// 已复制字节数
    pub done: u64,
    /// 总字节数（OpenableColumns.SIZE 预检；未知大小（流式 provider）为 0）
    pub total: u64,
    /// 复制是否已结束（成功/失败/取消三者其一）
    pub finished: bool,
    /// 是否被用户取消
    pub cancelled: bool,
    /// 失败原因（仅失败时非空）
    pub error: Option<String>,
    /// cache 落盘绝对路径
    pub dest_path: String,
}

/// SAF 流直传句柄（M3：上传 SAF 流直传的 Kotlin safOpen 返回）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafStreamHandle {
    /// 句柄 id（safRead/safSeek/safClose 用；任务内重连复用同一句柄）
    pub handle_id: String,
    /// 实际生效的读取起始偏移（真续传 seek 到 offset；pipe 流从头为 0）
    pub effective_offset: u64,
    /// 是否可 seek（getStatSize()==-1 探测；pipe 流为 false）
    pub seekable: bool,
    /// 文件总大小（statSize；pipe 流/未知为 0）。宿主上传进度用它作为 total，
    /// 插件据此展示真实进度条（插件侧 expected_size 恒为 0）
    #[serde(default)]
    pub size: u64,
}

/// SAF 存储访问能力（主 seam）
///
/// 实现方：
/// - Android：[`KotlinSafIo`]（转发 Kotlin SafTransferPlugin，经
///   run_mobile_plugin_async，见 android_plugins.rs）
/// - 其他平台：[`UnavailableSafIo`]（明确不可用错误，dev 窗口无 SAF 概念）
///
/// 测试注入 fake（见本模块 tests）：验证编排逻辑（JSON 解析、错误映射、
/// 分发）不依赖真实系统。

/// 生成「SAF 不可用」错误 impl 块（非 Android 平台的 KotlinSafIo 与
/// UnavailableSafIo 共用；错误消息参数化，消除 28 个方法体逐字重复）
macro_rules! saf_unavailable_impl {
    ($err:expr) => {
        fn list_tree(&self, _tree_uri: &str, _document_id: &str) -> Result<Vec<SafEntry>> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn read_to_cache(&self, _uri: &str, _dest_name: &str) -> Result<SafCopyHandle> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn copy_status(&self, _copy_id: &str) -> Result<SafCopyStatus> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn cancel_copy(&self, _copy_id: &str) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn cleanup_stale_copies(&self) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn check_authorized(&self, _tree_uri: &str) -> Result<bool> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn write_media_downloads(
            &self,
            _src_path: &str,
            _display_name: &str,
            _mime_type: &str,
        ) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn open_stream(&self, _uri: &str, _offset: u64) -> Result<SafStreamHandle> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn read_stream(&self, _handle_id: &str, _len: usize) -> Result<Vec<u8>> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn seek_stream(&self, _handle_id: &str, _offset: u64) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn close_stream(&self, _handle_id: &str) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn stream_seekable(&self, _uri: &str) -> Result<bool> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
        fn save_to_document(
            &self,
            _src_path: &str,
            _suggested_name: &str,
            _mime_type: &str,
        ) -> Result<()> {
            Err(crate::AppError::Plugin($err.to_string()))
        }
    };
}

pub trait SafIo: Send + Sync {
    /// 列出目录树子条目（tree_uri + document_id 定位目录）
    fn list_tree(&self, tree_uri: &str, document_id: &str) -> Result<Vec<SafEntry>>;

    /// 启动中转复制（SAF 源 → app 私有 cache），立即返回句柄
    fn read_to_cache(&self, uri: &str, dest_name: &str) -> Result<SafCopyHandle>;

    /// 轮询中转复制进度
    fn copy_status(&self, copy_id: &str) -> Result<SafCopyStatus>;

    /// 取消中转复制（复制方删除半成品后结束）
    fn cancel_copy(&self, copy_id: &str) -> Result<()>;

    /// 清扫中转复制残留（插件激活时调用，删除 staging 目录全部文件）
    fn cleanup_stale_copies(&self) -> Result<()>;

    /// 检测树授权是否仍有效（用户回收持久化授权后返回 false）
    fn check_authorized(&self, tree_uri: &str) -> Result<bool>;

    /// 写入 MediaStore.Downloads 公共下载目录（接收方向统一落点，M2）
    ///
    /// src_path 为 app 私有下载目录中的最终文件；流拷贝到公共 Download 目录
    /// （API 29+ 零权限）。失败（含 API<29 设备不支持）由调用方回退私有目录
    /// （调用方保留 src_path 副本即完成回退）。
    fn write_media_downloads(
        &self,
        src_path: &str,
        display_name: &str,
        mime_type: &str,
    ) -> Result<()>;

    /// 打开 SAF 源为可流读句柄（M3 上传流直传）
    ///
    /// offset 语义（spec M3 续传策略）：可 seek（文件流）直接 seek 到 offset
    /// 真续传；pipe 流（不可 seek）只能从头读，effective_offset 返回 0，
    /// 调用方（transfer.rs upload）发现与请求 offset 不一致时回报
    /// not-seekable-resume 触发全量重传。任务内断线重连重复打开同一 uri
    /// 时 Kotlin 侧复用既有句柄（fd 保留），effective_offset 为当前流位置
    /// （顺序续读不重读）。
    fn open_stream(&self, uri: &str, offset: u64) -> Result<SafStreamHandle>;

    /// 从流句柄读取至多 len 字节（EOF 返回空 Vec）
    fn read_stream(&self, handle_id: &str, len: usize) -> Result<Vec<u8>>;

    /// 移动流句柄到指定偏移（仅可 seek 句柄）
    fn seek_stream(&self, handle_id: &str, offset: u64) -> Result<()>;

    /// 关闭流句柄（任务终态后调用；任务内恢复不调用，fd 保留续读）
    fn close_stream(&self, handle_id: &str) -> Result<()>;

    /// 探测 SAF 源是否可 seek（getStatSize()==-1 为 pipe 流）
    fn stream_seekable(&self, uri: &str) -> Result<bool>;

    /// 「保存到…」（M3）：弹出 ACTION_CREATE_DOCUMENT 单文件对话框（用户选
    /// 位置）并把 src_path 流拷贝到所选位置（写完即达）
    ///
    /// 用户取消对话框视为失败（保留私有副本回退）；suggested_name 为
    /// 对话框默认文件名（远端文件名），mime_type 为空串时按扩展名推断。
    fn save_to_document(
        &self,
        src_path: &str,
        suggested_name: &str,
        mime_type: &str,
    ) -> Result<()>;
}

/// Tauri 托管状态（命令经 `app_handle.state::<SafIoState>()` 取实现；
/// 测试可 manage 一个注入 fake 的实例）
pub struct SafIoState(pub Arc<dyn SafIo>);

impl std::ops::Deref for SafIoState {
    type Target = dyn SafIo;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// Android 实现：转发 Kotlin SafTransferPlugin（薄壳，仅系统调用转发）
#[derive(Debug, Clone, Copy, Default)]
pub struct KotlinSafIo;

/// 非 Android 实现：明确不可用错误（dev 窗口无 SAF 概念）
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableSafIo;

/// 按平台构造默认实现（setup 时 manage 进 Tauri state）
pub fn default_saf_io() -> Arc<dyn SafIo> {
    #[cfg(target_os = "android")]
    {
        Arc::new(KotlinSafIo)
    }
    #[cfg(not(target_os = "android"))]
    {
        Arc::new(UnavailableSafIo)
    }
}

#[cfg(target_os = "android")]
impl SafIo for KotlinSafIo {
    fn list_tree(&self, tree_uri: &str, document_id: &str) -> Result<Vec<SafEntry>> {
        // 命令已处于 Tokio 异步上下文：block_in_place + block_on 桥接
        // run_mobile_plugin_async（与 wasm_runtime host fn 同模式，见下）
        block_on_plugin(|| crate::plugin::android_plugins::saf_list_tree(tree_uri, document_id))
    }

    fn read_to_cache(&self, uri: &str, dest_name: &str) -> Result<SafCopyHandle> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_copy_start(uri, dest_name))
    }

    fn copy_status(&self, copy_id: &str) -> Result<SafCopyStatus> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_copy_status(copy_id))
    }

    fn cancel_copy(&self, copy_id: &str) -> Result<()> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_copy_cancel(copy_id))
    }

    fn cleanup_stale_copies(&self) -> Result<()> {
        block_on_plugin(crate::plugin::android_plugins::saf_cleanup_stale_copies)
    }

    fn check_authorized(&self, tree_uri: &str) -> Result<bool> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_check_authorized(tree_uri))
    }

    fn write_media_downloads(
        &self,
        src_path: &str,
        display_name: &str,
        mime_type: &str,
    ) -> Result<()> {
        block_on_plugin(|| {
            crate::plugin::android_plugins::saf_write_media_downloads(
                src_path, display_name, mime_type,
            )
        })
    }

    fn open_stream(&self, uri: &str, offset: u64) -> Result<SafStreamHandle> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_stream_open(uri, offset))
    }

    fn read_stream(&self, handle_id: &str, len: usize) -> Result<Vec<u8>> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_stream_read(handle_id, len))
    }

    fn seek_stream(&self, handle_id: &str, offset: u64) -> Result<()> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_stream_seek(handle_id, offset))
    }

    fn close_stream(&self, handle_id: &str) -> Result<()> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_stream_close(handle_id))
    }

    fn stream_seekable(&self, uri: &str) -> Result<bool> {
        block_on_plugin(|| crate::plugin::android_plugins::saf_stream_seekable(uri))
    }

    fn save_to_document(
        &self,
        src_path: &str,
        suggested_name: &str,
        mime_type: &str,
    ) -> Result<()> {
        block_on_plugin(|| {
            crate::plugin::android_plugins::saf_save_to_document(
                src_path, suggested_name, mime_type,
            )
        })
    }
}

/// 在同步 trait 方法内阻塞等待 Kotlin 插件异步调用
///
/// 调用方有两类线程上下文：
/// - tauri 全局多线程 runtime（wasm host 函数，见 wasm_runtime.rs 的
///   guarded_host_call 模式）——`block_in_place` 可用；
/// - actix file service worker（actix-rt 2.x 默认 current_thread runtime）
///   ——`block_in_place` 会 panic（「can call blocking only when running on
///   the multi-threaded runtime」），panic 掐断连接，对端表现为连接错误
///   （桌面端浏览 SAF 根目录即触发）。
///
/// 统一改法：起 scoped 线程（无任何 runtime 上下文）经 tauri 全局
/// 多线程 runtime 驱动 future，本线程阻塞等待结果。scoped 线程可借用
/// 调用方栈上的非 'static 数据，各调用点无需改动。驱动线程 panic 时
/// catch_unwind 兜底为错误（scope join 不再向 actix worker 传播 panic），
/// 对端拿到 500 而非连接被掐断。
#[cfg(target_os = "android")]
fn block_on_plugin<F, Fut, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T>> + Send,
    T: Send,
{
    std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::channel();
        scope.spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tauri::async_runtime::block_on(f())
            }));
            // None = 驱动线程 panic（连接接缝 fail-soft）
            let _ = tx.send(result.ok());
        });
        match rx.recv() {
            // 正常：透传插件调用结果（Ok/Err 均来自 Kotlin 侧）
            Ok(Some(result)) => result,
            // 驱动线程 panic：catch_unwind 兜底，不给 actix worker 传播 panic
            Ok(None) => Err(crate::AppError::Plugin(
                "SAF plugin call panicked in bridge thread".to_string(),
            )),
            // channel 断开：驱动线程异常退出
            Err(_) => Err(crate::AppError::Plugin(
                "SAF plugin task dropped before completion".to_string(),
            )),
        }
    })
}

#[cfg(not(target_os = "android"))]
impl SafIo for KotlinSafIo {
    saf_unavailable_impl!("SafIo unavailable on this platform (SAF is Android-only)");
}

impl SafIo for UnavailableSafIo {
    saf_unavailable_impl!("SAF storage is not available on this platform");
}

// ==================== Kotlin 响应解析（可单测） ====================

/// 解析 Kotlin listTreeChildren 的 entries 数组（wire camelCase → SafEntry）
///
/// Kotlin JSObject 遵循 camelCase 约定（isDir/size/mime/uri/documentId），
/// 与 Rust serde snake_case 约定不一致，集中在此解析并单测。
pub fn parse_saf_entries(value: &serde_json::Value) -> Result<Vec<SafEntry>> {
    let arr = value
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            crate::AppError::Plugin("saf_list_tree: missing 'entries' array in response".to_string())
        })?;
    arr.iter()
        .map(|e| {
            Ok(SafEntry {
                name: e.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                is_dir: e.get("isDir").and_then(|v| v.as_bool()).unwrap_or(false),
                size: e.get("size").and_then(|v| v.as_i64()).unwrap_or(0),
                mime: e.get("mime").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                uri: e.get("uri").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                document_id: e
                    .get("documentId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// 解析 Kotlin safToCache 响应（copyId/destPath）
pub fn parse_saf_copy_handle(value: &serde_json::Value) -> Result<SafCopyHandle> {
    let copy_id = value
        .get("copyId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let dest_path = value
        .get("destPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if copy_id.is_empty() || dest_path.is_empty() {
        return Err(crate::AppError::Plugin(format!(
            "saf_copy_start: invalid response (copyId={}, destPath={})",
            copy_id, dest_path
        )));
    }
    Ok(SafCopyHandle { copy_id, dest_path })
}

/// 解析 Kotlin copyProgress 响应
pub fn parse_saf_copy_status(value: &serde_json::Value) -> Result<SafCopyStatus> {
    let copy_id = value
        .get("copyId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if copy_id.is_empty() {
        return Err(crate::AppError::Plugin(
            "saf_copy_status: missing copyId in response".to_string(),
        ));
    }
    let done = value.get("done").and_then(|v| v.as_u64()).unwrap_or(0);
    // Kotlin 未知大小编码为 -1，这里归一为 0（前端进度按 0 总大小处理为不确定进度）
    let total = value
        .get("total")
        .and_then(|v| v.as_i64())
        .map(|t| if t < 0 { 0 } else { t as u64 })
        .unwrap_or(0);
    Ok(SafCopyStatus {
        copy_id,
        done,
        total,
        finished: value.get("finished").and_then(|v| v.as_bool()).unwrap_or(false),
        cancelled: value.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false),
        error: value.get("error").and_then(|v| v.as_str()).map(String::from),
        dest_path: value
            .get("destPath")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// 解析 Kotlin writeMediaDownloads 响应（{ok: true} 成功；其余视为失败）
pub fn parse_media_write_response(value: &serde_json::Value) -> Result<()> {
    if value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        let err = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        Err(crate::AppError::Plugin(format!(
            "saf_write_media_downloads failed: {}",
            err
        )))
    }
}

/// 解析 Kotlin safOpen 响应（handleId/effectiveOffset/seekable）
pub fn parse_saf_stream_handle(value: &serde_json::Value) -> Result<SafStreamHandle> {
    let handle_id = value
        .get("handleId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if handle_id.is_empty() {
        return Err(crate::AppError::Plugin(
            "saf_stream_open: missing handleId in response".to_string(),
        ));
    }
    Ok(SafStreamHandle {
        handle_id,
        effective_offset: value.get("effectiveOffset").and_then(|v| v.as_u64()).unwrap_or(0),
        seekable: value.get("seekable").and_then(|v| v.as_bool()).unwrap_or(false),
        size: value.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
    })
}

/// 解析 Kotlin safRead 响应（data 为 base64 编码字节；EOF 为空串）
///
/// 传输格式权衡（spec M3）：JSON 数组数字（每字节 4 字符）与 hex（2 字符）
/// 均劣于 base64（4/3 字符、无填充换行），选 base64 为跨桥最省空间格式。
pub fn parse_saf_read_response(value: &serde_json::Value) -> Result<Vec<u8>> {
    use base64::Engine as _;
    let data = value
        .get("data")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if data.is_empty() {
        // 空串 = EOF 或空块（read 返回 0 不可能：请求 len≥1 且 Kotlin 侧
        // read 阻塞直到有数据或 EOF）
        return Ok(Vec::new());
    }
    base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| {
            crate::AppError::Plugin(format!(
                "saf_stream_read: invalid base64 payload from Kotlin: {}",
                e
            ))
        })
}

/// 解析 Kotlin safSeekable 响应（{seekable: bool}）
pub fn parse_saf_seekable_response(value: &serde_json::Value) -> Result<bool> {
    Ok(value.get("seekable").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// 解析 Kotlin saveToDocument 响应
///
/// {ok:true} 成功；{ok:false, cancelled:true} 用户取消（保留私有副本回退）；
/// 其余视为失败。
pub fn parse_save_to_document_response(value: &serde_json::Value) -> Result<()> {
    if value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else if value.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false) {
        Err(crate::AppError::Plugin(
            "saf_save_to_document cancelled by user".to_string(),
        ))
    } else {
        let err = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        Err(crate::AppError::Plugin(format!(
            "saf_save_to_document failed: {}",
            err
        )))
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约：SafEntry 序列化必须为 camelCase（前端 SDK SafEntry 期望
    /// isDir/documentId）——缺 rename 时 is_dir/document_id 泄漏为 snake_case，
    /// 目录被当文件（图标全 fallback）、子目录无法进入（回归防护）
    #[test]
    fn saf_entry_serializes_camel_case() {
        let e = SafEntry {
            name: "photo.jpg".to_string(),
            is_dir: false,
            size: 10,
            mime: "image/jpeg".to_string(),
            uri: "content://x".to_string(),
            document_id: "primary:DCIM".to_string(),
        };
        let json = serde_json::to_value(&e).unwrap();
        assert!(json.get("isDir").is_some(), "missing isDir");
        assert!(json.get("documentId").is_some(), "missing documentId");
        assert!(json.get("is_dir").is_none(), "snake_case leaked");
        assert!(json.get("document_id").is_none(), "snake_case leaked");
        assert_eq!(json["name"], "photo.jpg");
    }

    /// fake 实现：记录调用并返回固定结果（编排逻辑测试注入用）
    struct FakeSafIo {
        entries: Vec<SafEntry>,
        handle: SafCopyHandle,
        status: SafCopyStatus,
        authorized: bool,
        cancelled: std::sync::Mutex<Vec<String>>,
    }

    impl SafIo for FakeSafIo {
        fn list_tree(&self, tree_uri: &str, document_id: &str) -> Result<Vec<SafEntry>> {
            assert_eq!(tree_uri, "content://tree/root");
            assert_eq!(document_id, "primary%3ADownload");
            Ok(self.entries.clone())
        }

        fn read_to_cache(&self, uri: &str, dest_name: &str) -> Result<SafCopyHandle> {
            assert_eq!(uri, "content://tree/root/document/f1");
            assert_eq!(dest_name, "a.txt");
            Ok(self.handle.clone())
        }

        fn copy_status(&self, copy_id: &str) -> Result<SafCopyStatus> {
            assert_eq!(copy_id, "copy-1");
            Ok(self.status.clone())
        }

        fn cancel_copy(&self, copy_id: &str) -> Result<()> {
            self.cancelled.lock().unwrap().push(copy_id.to_string());
            Ok(())
        }

        fn cleanup_stale_copies(&self) -> Result<()> {
            Ok(())
        }

        fn check_authorized(&self, tree_uri: &str) -> Result<bool> {
            assert_eq!(tree_uri, "content://tree/root");
            Ok(self.authorized)
        }

        fn write_media_downloads(
            &self,
            src_path: &str,
            display_name: &str,
            mime_type: &str,
        ) -> Result<()> {
            assert_eq!(src_path, "/data/downloads/a.txt");
            assert_eq!(display_name, "a.txt");
            assert_eq!(mime_type, "");
            Ok(())
        }

        fn open_stream(&self, uri: &str, offset: u64) -> Result<SafStreamHandle> {
            assert_eq!(uri, "content://tree/root/document/f1");
            assert_eq!(offset, 0);
            Ok(SafStreamHandle {
                handle_id: "stream-1".to_string(),
                effective_offset: offset,
                seekable: true,
                size: 0,
            })
        }

        fn read_stream(&self, handle_id: &str, _len: usize) -> Result<Vec<u8>> {
            assert_eq!(handle_id, "stream-1");
            Ok(b"abc".to_vec())
        }

        fn seek_stream(&self, handle_id: &str, offset: u64) -> Result<()> {
            assert_eq!(handle_id, "stream-1");
            assert_eq!(offset, 0);
            Ok(())
        }

        fn close_stream(&self, handle_id: &str) -> Result<()> {
            assert_eq!(handle_id, "stream-1");
            Ok(())
        }

        fn stream_seekable(&self, uri: &str) -> Result<bool> {
            assert_eq!(uri, "content://tree/root/document/f1");
            Ok(true)
        }

        fn save_to_document(
            &self,
            src_path: &str,
            suggested_name: &str,
            mime_type: &str,
        ) -> Result<()> {
            assert_eq!(src_path, "/data/downloads/a.txt");
            assert_eq!(suggested_name, "a.txt");
            assert_eq!(mime_type, "");
            Ok(())
        }
    }

    #[test]
    fn parse_entries_handles_camelcase_wire() {
        let json = serde_json::json!({
            "entries": [
                {
                    "name": "照片",
                    "isDir": true,
                    "size": 0,
                    "mime": "application/vnd.google-apps.folder",
                    "uri": "content://tree/root/document/d1",
                    "documentId": "d1"
                },
                {
                    "name": "a.txt",
                    "isDir": false,
                    "size": 1024,
                    "mime": "text/plain",
                    "uri": "content://tree/root/document/f1",
                    "documentId": "f1"
                }
            ]
        });
        let entries = parse_saf_entries(&json).expect("entries should parse");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "照片");
        assert_eq!(entries[0].document_id, "d1");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].size, 1024);
        assert_eq!(entries[1].uri, "content://tree/root/document/f1");
    }

    #[test]
    fn parse_entries_rejects_missing_array() {
        let err = parse_saf_entries(&serde_json::json!({ "foo": 1 })).unwrap_err();
        assert!(err.to_string().contains("entries"));
    }

    #[test]
    fn parse_copy_handle_roundtrip() {
        let json = serde_json::json!({
            "copyId": "copy-1",
            "destPath": "/data/user/0/com.bedcode.mobile/cache/bedcode_uploads/a.txt"
        });
        let handle = parse_saf_copy_handle(&json).expect("handle should parse");
        assert_eq!(handle.copy_id, "copy-1");
        assert!(handle.dest_path.ends_with("bedcode_uploads/a.txt"));
    }

    #[test]
    fn parse_copy_handle_rejects_incomplete() {
        let err = parse_saf_copy_handle(&serde_json::json!({ "copyId": "" })).unwrap_err();
        assert!(err.to_string().contains("saf_copy_start"));
    }

    #[test]
    fn parse_copy_status_normalizes_unknown_total() {
        let json = serde_json::json!({
            "copyId": "copy-1",
            "done": 500,
            "total": -1,
            "finished": false,
            "cancelled": false,
            "error": null,
            "destPath": "/cache/bedcode_uploads/a.txt"
        });
        let status = parse_saf_copy_status(&json).expect("status should parse");
        assert_eq!(status.done, 500);
        assert_eq!(status.total, 0);
        assert!(!status.finished);
        assert!(status.error.is_none());
    }

    #[test]
    fn parse_copy_status_keeps_error() {
        let json = serde_json::json!({
            "copyId": "copy-1",
            "done": 100,
            "total": 1000,
            "finished": true,
            "cancelled": false,
            "error": "EACCES",
            "destPath": "/cache/bedcode_uploads/a.txt"
        });
        let status = parse_saf_copy_status(&json).expect("status should parse");
        assert!(status.finished);
        assert_eq!(status.error.as_deref(), Some("EACCES"));
    }

    #[test]
    fn trait_dispatch_forwards_to_fake() {
        let fake = FakeSafIo {
            entries: vec![SafEntry {
                name: "a.txt".to_string(),
                is_dir: false,
                size: 1,
                mime: "text/plain".to_string(),
                uri: "content://tree/root/document/f1".to_string(),
                document_id: "f1".to_string(),
            }],
            handle: SafCopyHandle {
                copy_id: "copy-1".to_string(),
                dest_path: "/cache/a.txt".to_string(),
            },
            status: SafCopyStatus {
                copy_id: "copy-1".to_string(),
                done: 0,
                total: 10,
                finished: true,
                cancelled: false,
                error: None,
                dest_path: "/cache/a.txt".to_string(),
            },
            authorized: true,
            cancelled: std::sync::Mutex::new(Vec::new()),
        };

        let list = fake
            .list_tree("content://tree/root", "primary%3ADownload")
            .expect("list should succeed");
        assert_eq!(list.len(), 1);

        let handle = fake
            .read_to_cache("content://tree/root/document/f1", "a.txt")
            .expect("copy start should succeed");
        assert_eq!(handle.dest_path, "/cache/a.txt");

        let status = fake.copy_status("copy-1").expect("status should succeed");
        assert!(status.finished);

        fake.cancel_copy("copy-1").expect("cancel should succeed");
        assert_eq!(fake.cancelled.lock().unwrap().as_slice(), &["copy-1".to_string()]);

        let authorized = fake.check_authorized("content://tree/root").expect("check should succeed");
        assert!(authorized);
    }

    #[test]
    fn parse_media_write_response_ok_and_error() {
        assert!(parse_media_write_response(&serde_json::json!({ "ok": true })).is_ok());
        let err = parse_media_write_response(&serde_json::json!({ "ok": false, "error": "requires API 29+" }))
            .unwrap_err();
        assert!(err.to_string().contains("requires API 29+"));
        let err2 = parse_media_write_response(&serde_json::json!({})).unwrap_err();
        assert!(err2.to_string().contains("unknown error"));
    }

    #[test]
    fn parse_stream_handle_roundtrip() {
        let json = serde_json::json!({
            "handleId": "stream-1",
            "effectiveOffset": 2048,
            "seekable": false,
            "size": 4096,
        });
        let handle = parse_saf_stream_handle(&json).expect("handle should parse");
        assert_eq!(handle.handle_id, "stream-1");
        assert_eq!(handle.effective_offset, 2048);
        assert!(!handle.seekable);
        assert_eq!(handle.size, 4096);
    }

    #[test]
    fn parse_stream_handle_rejects_missing_id() {
        let err = parse_saf_stream_handle(&serde_json::json!({ "effectiveOffset": 0 })).unwrap_err();
        assert!(err.to_string().contains("handleId"));
    }

    #[test]
    fn parse_read_response_decodes_base64_and_eof() {
        use base64::Engine as _;
        let payload = b"\x00\x01\x02saf-data".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
        let decoded = parse_saf_read_response(&serde_json::json!({ "data": b64 }))
            .expect("base64 should decode");
        assert_eq!(decoded, payload);
        // 空串 = EOF
        assert!(parse_saf_read_response(&serde_json::json!({ "data": "" }))
            .expect("eof should parse")
            .is_empty());
        // 非法 base64 必须显式报错（数据损坏不能静默吞掉）
        let err = parse_saf_read_response(&serde_json::json!({ "data": "!!!not-base64!!!" }))
            .unwrap_err();
        assert!(err.to_string().contains("base64"));
    }

    #[test]
    fn parse_seekable_and_save_to_document_responses() {
        assert!(parse_saf_seekable_response(&serde_json::json!({ "seekable": true })).expect("ok"));
        assert!(!parse_saf_seekable_response(&serde_json::json!({ "seekable": false })).expect("ok"));

        assert!(parse_save_to_document_response(&serde_json::json!({ "ok": true })).is_ok());
        let cancelled = parse_save_to_document_response(&serde_json::json!({ "ok": false, "cancelled": true }))
            .unwrap_err();
        assert!(cancelled.to_string().contains("cancelled by user"));
        let err = parse_save_to_document_response(&serde_json::json!({ "ok": false, "error": "EACCES" }))
            .unwrap_err();
        assert!(err.to_string().contains("EACCES"));
    }

    #[test]
    fn stream_methods_dispatch_to_fake() {
        let fake = FakeSafIo {
            entries: vec![],
            handle: SafCopyHandle {
                copy_id: "copy-1".to_string(),
                dest_path: "/cache/a.txt".to_string(),
            },
            status: SafCopyStatus {
                copy_id: "copy-1".to_string(),
                done: 0,
                total: 10,
                finished: true,
                cancelled: false,
                error: None,
                dest_path: "/cache/a.txt".to_string(),
            },
            authorized: true,
            cancelled: std::sync::Mutex::new(Vec::new()),
        };
        let handle = fake
            .open_stream("content://tree/root/document/f1", 0)
            .expect("open should succeed");
        assert_eq!(handle.handle_id, "stream-1");
        assert_eq!(
            fake.read_stream("stream-1", 1024).expect("read should succeed"),
            b"abc"
        );
        fake.seek_stream("stream-1", 0).expect("seek should succeed");
        fake.close_stream("stream-1").expect("close should succeed");
        assert!(fake
            .stream_seekable("content://tree/root/document/f1")
            .expect("seekable should succeed"));
        fake.save_to_document("/data/downloads/a.txt", "a.txt", "")
            .expect("save should succeed");
    }

    #[test]
    fn unavailable_impl_returns_contextual_error() {
        let io = UnavailableSafIo;
        let err = io.list_tree("content://tree/x", "doc").unwrap_err();
        assert!(err.to_string().contains("SAF storage is not available"));
    }

    #[test]
    fn write_media_dispatches_to_fake() {
        let fake = FakeSafIo {
            entries: vec![],
            handle: SafCopyHandle {
                copy_id: "copy-1".to_string(),
                dest_path: "/cache/a.txt".to_string(),
            },
            status: SafCopyStatus {
                copy_id: "copy-1".to_string(),
                done: 0,
                total: 10,
                finished: true,
                cancelled: false,
                error: None,
                dest_path: "/cache/a.txt".to_string(),
            },
            authorized: true,
            cancelled: std::sync::Mutex::new(Vec::new()),
        };
        fake.write_media_downloads("/data/downloads/a.txt", "a.txt", "")
            .expect("media write should succeed");
    }
}
