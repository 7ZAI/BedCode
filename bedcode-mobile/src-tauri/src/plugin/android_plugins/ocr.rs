//! OCR 取图 Kotlin 桥（拍照入口 + 取图响应解析）
//!
//! 相册选图（SafPickerPlugin.pickImage）与拍照（CameraPlugin）两条入口产出同一
//! 契约：RGBA8 临时文件 + {path, width, height}（spec §4.4）。解码/降采样全部在
//! Kotlin 完成，Rust 侧只做响应解析与错误上下文包装。
//!
//! 注册：CameraPlugin 使用独立 Builder 名 "ocr-camera"（register_android_plugin
//! 以 Builder 名作 key，同名互相覆盖，见 android_plugins.rs 模块注释与 spec §5.3）；
//! SafPickerPlugin 已在 picker.rs 以 "saf-picker" 注册，pickImage 只是其上新增方法。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 CameraPlugin 句柄（仅 Android 平台使用）
static CAMERA_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 CameraPlugin（ACTION_IMAGE_CAPTURE + FileProvider + CAMERA 运行时权限）
///
/// gen/android 重建恢复清单：CameraPlugin.kt / OcrImageDecoder.kt 须恢复
pub fn camera_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("ocr-camera")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "CameraPlugin")?;
                let _ = CAMERA_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 解析 Kotlin 取图响应：cancelled → Ok(None)；否则校验 path/width/height
///
/// 两种图源（pickImage / capture）共用；缺字段或尺寸非法返回带上下文的明确错误。
pub fn parse_ocr_image_response(
    response: &serde_json::Value,
) -> crate::Result<Option<crate::ocr::OcrImageSource>> {
    if response
        .get("cancelled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let path = response.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        return Err(crate::AppError::Plugin(
            "OCR image capture returned no path".to_string(),
        ));
    }
    let width = response.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
    let height = response.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
    if width == 0 || height == 0 {
        return Err(crate::AppError::Plugin(format!(
            "OCR image capture returned invalid size {}x{} for {}",
            width, height, path
        )));
    }
    Ok(Some(crate::ocr::OcrImageSource {
        path: path.to_string(),
        width: width as u32,
        height: height as u32,
    }))
}

/// 拍照取图：Kotlin 侧启动相机（权限未授予先弹系统权限框），
/// 成功后解码降采样为 RGBA8 临时文件返回；用户取消返回 Ok(None)。
#[cfg(target_os = "android")]
pub async fn camera_capture_android() -> crate::Result<Option<crate::ocr::OcrImageSource>> {
    let handle = CAMERA_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("CameraPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("capture", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke capture: {}", e)))?;
    parse_ocr_image_response(&response)
}

/// 非 Android 平台无相机桥（桌面 dev 下命令返回明确错误）
#[cfg(not(target_os = "android"))]
pub async fn camera_capture_android() -> crate::Result<Option<crate::ocr::OcrImageSource>> {
    Err(crate::AppError::Plugin(
        "camera capture unavailable on this platform".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: serde_json::Value) -> crate::Result<Option<crate::ocr::OcrImageSource>> {
        parse_ocr_image_response(&v)
    }

    /// 用户取消（cancelled=true）→ Ok(None)，与 SAF 选择器语义一致
    #[test]
    fn cancelled_maps_to_none() {
        let r = parse(serde_json::json!({ "cancelled": true })).unwrap();
        assert!(r.is_none());
    }

    /// 正常响应 → 解码产物（path/width/height 透传）
    #[test]
    fn ok_maps_to_source() {
        let r = parse(serde_json::json!({
            "path": "/data/user/0/com.bedcode.mobile/cache/ocr/ocr_1.rgba",
            "width": 1000,
            "height": 750
        }))
        .unwrap()
        .unwrap();
        assert_eq!(r.path, "/data/user/0/com.bedcode.mobile/cache/ocr/ocr_1.rgba");
        assert_eq!(r.width, 1000);
        assert_eq!(r.height, 750);
    }

    /// 缺 path → 明确错误（Kotlin 侧正常路径不会发生，防御解析）
    #[test]
    fn missing_path_rejected() {
        let err = parse(serde_json::json!({ "width": 1, "height": 1 }))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no path"), "got: {}", err);
    }

    /// 尺寸非法（0/缺字段）→ 带尺寸与路径上下文的明确错误
    #[test]
    fn zero_size_rejected() {
        let err = parse(serde_json::json!({ "path": "/x.rgba", "width": 0, "height": 10 }))
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid size"), "got: {}", err);
        assert!(err.contains("0x10"), "got: {}", err);
    }

    /// 非 Android 平台 stub：明确错误而非 panic
    #[cfg(not(target_os = "android"))]
    #[tokio::test]
    async fn camera_stub_errors_on_non_android() {
        let err = camera_capture_android().await.unwrap_err().to_string();
        assert!(err.contains("unavailable"), "got: {}", err);
    }
}
