//! OCR 模型资源解压（OcrModelExtractorPlugin）
//!
//! 从 APK assets `resources/ocr_models/` 惰性解压 PP-OCRv4 三模型到
//! app_data_dir/ocr_models/（带 `.bedcode-source` 版本标记，同 PluginAssetExtractor 思路，
//! 见规格 .scratch/ocr-plugin/spec.md §4.5）。
//! 另提供 nativeLibraryDir 查询，供 onnxruntime .so 存在性探测（engine_status.available）。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 OcrModelExtractorPlugin 句柄（仅 Android 平台使用）
static PLUGIN_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 OcrModelExtractorPlugin（OCR 模型惰性解压；Builder 名独立，勿与其他插件同名覆盖）
pub fn ocr_model_extractor_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("ocr-model-extractor")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "OcrModelExtractorPlugin")?;
                let _ = PLUGIN_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 调用 Kotlin OcrModelExtractorPlugin 惰性解压 OCR 模型（幂等：标记匹配且文件齐全则跳过）
#[cfg(target_os = "android")]
pub async fn extract_ocr_models(app_version: &str) -> crate::Result<u32> {
    let handle = PLUGIN_HANDLE
        .get()
        .ok_or_else(|| crate::AppError::Plugin("OcrModelExtractorPlugin not registered".to_string()))?;
    let payload = serde_json::json!({ "appVersion": app_version });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("extractOcrModels", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke extractOcrModels: {}", e)))?;
    let count = response.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as u32;
    tracing::info!(count, "Extracted OCR model(s) from APK assets");
    Ok(count)
}

/// nativeLibraryDir（onnxruntime .so 所在目录，dlopen 路径）
#[cfg(target_os = "android")]
pub async fn native_library_dir() -> crate::Result<String> {
    let handle = PLUGIN_HANDLE
        .get()
        .ok_or_else(|| crate::AppError::Plugin("OcrModelExtractorPlugin not registered".to_string()))?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("getNativeLibraryDir", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke getNativeLibraryDir: {}", e)))?;
    response
        .get("dir")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| crate::AppError::Plugin("getNativeLibraryDir: missing 'dir' in response".to_string()))
}

/// 非 Android 平台：无 APK assets 与 native lib 目录（桌面 dev 由源码资源路径替代）
#[cfg(not(target_os = "android"))]
pub async fn extract_ocr_models(_app_version: &str) -> crate::Result<u32> {
    Ok(0)
}

/// 确保 libonnxruntime.so 在文件系统上可 dlopen：调 Kotlin 从 APK 提取副本到 dataDir。
///
/// extractNativeLibs=false（targetSdk 31+ 默认）时 nativeLibraryDir 下没有解压文件，
/// 而 Rust 侧 ort::init_from 走原生 dlopen 需要真实路径；提取幂等，返回最终可用路径。
#[cfg(target_os = "android")]
pub async fn ensure_native_lib() -> crate::Result<std::path::PathBuf> {
    let handle = PLUGIN_HANDLE
        .get()
        .ok_or_else(|| crate::AppError::Plugin("OcrModelExtractorPlugin not registered".to_string()))?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("ensureNativeLib", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke ensureNativeLib: {}", e)))?;
    response
        .get("path")
        .and_then(|p| p.as_str())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| crate::AppError::Plugin("ensureNativeLib: missing 'path' in response".to_string()))
}

#[cfg(not(target_os = "android"))]
pub async fn ensure_native_lib() -> crate::Result<std::path::PathBuf> {
    Err(crate::AppError::Plugin("ensure_native_lib is android-only".to_string()))
}

#[cfg(not(target_os = "android"))]
pub async fn native_library_dir() -> crate::Result<String> {
    Err(crate::AppError::Plugin(
        "native_library_dir is android-only".to_string(),
    ))
}
