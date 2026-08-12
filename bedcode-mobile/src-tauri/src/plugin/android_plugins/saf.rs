//! SafIo 桥 — 把 saf_io.rs 的 trait 方法经 run_mobile_plugin_async
//! 转发到 Kotlin SafTransferPlugin（响应解析在 saf_io.rs，可单测）
//!
//! 从 android_plugins.rs 拆分。

// ==================== SafIo 桥（SafTransferPlugin 转发） ====================
//
// 对应 saf_io.rs 的 KotlinSafIo：把 trait 方法经 run_mobile_plugin_async
// 转发到 Kotlin SafTransferPlugin，并把响应解析为 Rust 类型（解析函数在
// saf_io.rs，可单测）。


/// 列出目录树子条目
#[cfg(target_os = "android")]
pub async fn saf_list_tree(tree_uri: &str, document_id: &str) -> crate::Result<Vec<crate::plugin::saf_io::SafEntry>> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "listTreeChildren",
            serde_json::json!({ "treeUri": tree_uri, "documentId": document_id }),
        )
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke listTreeChildren: {}", e))
        })?;
    crate::plugin::saf_io::parse_saf_entries(&response)
}


/// 启动中转复制（SAF 源 → app 私有 cache）
#[cfg(target_os = "android")]
pub async fn saf_copy_start(
    uri: &str,
    dest_name: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyHandle> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safToCache",
            serde_json::json!({ "uri": uri, "destName": dest_name }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safToCache: {}", e)))?;
    crate::plugin::saf_io::parse_saf_copy_handle(&response)
}


/// 轮询中转复制进度
#[cfg(target_os = "android")]
pub async fn saf_copy_status(
    copy_id: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyStatus> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("copyProgress", serde_json::json!({ "copyId": copy_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke copyProgress: {}", e)))?;
    crate::plugin::saf_io::parse_saf_copy_status(&response)
}


/// 取消中转复制
#[cfg(target_os = "android")]
pub async fn saf_copy_cancel(copy_id: &str) -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("cancelCopy", serde_json::json!({ "copyId": copy_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke cancelCopy: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "cancelCopy rejected for copyId {}",
            copy_id
        )))
    }
}


/// 清扫中转复制残留（file-transfer 插件激活时调用，删除 staging 目录全部文件）
#[cfg(target_os = "android")]
pub async fn saf_cleanup_stale_copies() -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    handle
        .run_mobile_plugin_async::<()>("cleanupStaleCopies", serde_json::json!({}))
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke cleanupStaleCopies: {}", e))
        })
}


/// 检测树授权是否仍有效
#[cfg(target_os = "android")]
pub async fn saf_check_authorized(tree_uri: &str) -> crate::Result<bool> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "checkAuthorized",
            serde_json::json!({ "treeUri": tree_uri }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke checkAuthorized: {}", e)))?;
    Ok(response.get("authorized").and_then(|v| v.as_bool()).unwrap_or(false))
}


/// 写入 MediaStore.Downloads 公共下载目录（接收方向统一落点，M2）
///
/// src_path 为 app 私有下载目录中的最终文件；mime_type 为空串时由 Kotlin
/// 按扩展名推断。失败（含 API<29 设备不支持）返回错误，调用方回退私有目录。
#[cfg(target_os = "android")]
pub async fn saf_write_media_downloads(
    src_path: &str,
    display_name: &str,
    mime_type: &str,
) -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "writeMediaDownloads",
            serde_json::json!({
                "srcPath": src_path,
                "displayName": display_name,
                "mimeType": mime_type,
            }),
        )
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke writeMediaDownloads: {}", e))
        })?;
    crate::plugin::saf_io::parse_media_write_response(&response)
}


/// 打开 SAF 源为可流读句柄（M3 上传流直传；offset 语义见 SafIo::open_stream）
#[cfg(target_os = "android")]
pub async fn saf_stream_open(
    uri: &str,
    offset: u64,
) -> crate::Result<crate::plugin::saf_io::SafStreamHandle> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safOpen",
            serde_json::json!({ "uri": uri, "mode": "r", "offset": offset }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safOpen: {}", e)))?;
    crate::plugin::saf_io::parse_saf_stream_handle(&response)
}


/// 从流句柄读取至多 len 字节（EOF 返回空；base64 跨桥传输）
#[cfg(target_os = "android")]
pub async fn saf_stream_read(handle_id: &str, len: usize) -> crate::Result<Vec<u8>> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safRead",
            serde_json::json!({ "handleId": handle_id, "len": len }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safRead: {}", e)))?;
    crate::plugin::saf_io::parse_saf_read_response(&response)
}


/// 移动流句柄到指定偏移（仅可 seek 句柄）
#[cfg(target_os = "android")]
pub async fn saf_stream_seek(handle_id: &str, offset: u64) -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safSeek",
            serde_json::json!({ "handleId": handle_id, "offset": offset }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safSeek: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "safSeek rejected for handle {}",
            handle_id
        )))
    }
}


/// 关闭流句柄（任务终态后调用；任务内恢复不调用，fd 保留续读）
#[cfg(target_os = "android")]
pub async fn saf_stream_close(handle_id: &str) -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("safClose", serde_json::json!({ "handleId": handle_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safClose: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "safClose rejected for handle {}",
            handle_id
        )))
    }
}


/// 探测 SAF 源是否可 seek（getStatSize()==-1 为 pipe 流）
#[cfg(target_os = "android")]
pub async fn saf_stream_seekable(uri: &str) -> crate::Result<bool> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("safSeekable", serde_json::json!({ "uri": uri }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safSeekable: {}", e)))?;
    crate::plugin::saf_io::parse_saf_seekable_response(&response)
}


/// 「保存到…」（M3）：弹 ACTION_CREATE_DOCUMENT 对话框并流拷贝到用户选择的位置
///
/// 用户取消视为失败（保留私有副本回退）。suggested_name 为对话框默认文件名，
/// mime_type 为空串时按扩展名推断。
#[cfg(target_os = "android")]
pub async fn saf_save_to_document(
    src_path: &str,
    suggested_name: &str,
    mime_type: &str,
) -> crate::Result<()> {
    let handle = super::picker::SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "saveToDocument",
            serde_json::json!({
                "srcPath": src_path,
                "suggestedName": suggested_name,
                "mimeType": mime_type,
            }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke saveToDocument: {}", e)))?;
    crate::plugin::saf_io::parse_save_to_document_response(&response)
}


/// 非 Android 平台 SafIo 不可用（dev 窗口无 SAF 概念；调用方展示明确提示）
#[cfg(not(target_os = "android"))]
pub async fn saf_cleanup_stale_copies() -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_list_tree(
    _tree_uri: &str,
    _document_id: &str,
) -> crate::Result<Vec<crate::plugin::saf_io::SafEntry>> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_copy_start(
    _uri: &str,
    _dest_name: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyHandle> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_copy_status(
    _copy_id: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyStatus> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_copy_cancel(_copy_id: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_check_authorized(_tree_uri: &str) -> crate::Result<bool> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_write_media_downloads(
    _src_path: &str,
    _display_name: &str,
    _mime_type: &str,
) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_stream_open(
    _uri: &str,
    _offset: u64,
) -> crate::Result<crate::plugin::saf_io::SafStreamHandle> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_stream_read(_handle_id: &str, _len: usize) -> crate::Result<Vec<u8>> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_stream_seek(_handle_id: &str, _offset: u64) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_stream_close(_handle_id: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_stream_seekable(_uri: &str) -> crate::Result<bool> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}


#[cfg(not(target_os = "android"))]
pub async fn saf_save_to_document(
    _src_path: &str,
    _suggested_name: &str,
    _mime_type: &str,
) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}
