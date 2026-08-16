//! OcrEngine trait + engine 字段路由 + 引擎状态（spec §4.2/§4.3、§7 接缝）
//!
//! v2 在线适配器实现同一 trait，engine 字段形如 `online:<provider>`；
//! v1 仅 "offline" 可路由，其他值返回明确错误。
//! 识别全程（RGBA 校验 + 引擎推理）在 `spawn_blocking` 后台线程（spec §4.3：
//! ort 推理是阻塞调用，禁止占 async executor）。

use super::models;
use super::preprocess::RgbaImage;
use super::{OcrEngineStatus, OcrOutput, OcrRecognizeInput};
use crate::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::Manager;

/// OCR 引擎 trait：v1 仅 offline（ppocr）；v2 追加在线适配器
pub trait OcrEngine: std::fmt::Debug {
    /// 引擎标识（回填响应 engine 字段）
    fn engine_id(&self) -> &'static str;

    /// 识别：image 已由命令层校验（尺寸/文件），max_side 为服务端降采样上限
    fn recognize(&self, image: &RgbaImage, max_side: Option<u32>) -> Result<OcrOutput>;
}

/// engine 字段 → 引擎路由；offline 引擎携带运行上下文（模型目录 + onnxruntime .so）；
/// 不支持的 engine 值返回明确错误（接缝已就位，v1 拒绝在线）
pub fn route_engine(
    engine: &str,
    data_dir: PathBuf,
    onnxruntime_so: Option<PathBuf>,
) -> Result<Box<dyn OcrEngine + Send + Sync>> {
    match engine {
        "offline" => Ok(Box::new(super::ppocr::PpOcrEngine::new(
            super::ppocr::PpOcrContext {
                data_dir,
                onnxruntime_so,
            },
        ))),
        _ => Err(crate::AppError::InvalidInput(format!(
            "plugin_ocr_recognize: unsupported engine '{}' (supported: offline)",
            engine
        ))),
    }
}

/// `plugin_ocr_recognize` 命令体：路由 → spawn_blocking 全流程（RGBA 校验 + 模型存在性 + 推理）→ 计时
pub async fn recognize(app_handle: &tauri::AppHandle, input: &OcrRecognizeInput) -> Result<OcrOutput> {
    let engine_name = input.engine.clone().unwrap_or_else(|| "offline".to_string());
    let data_dir = app_handle.path().app_data_dir()?;
    let onnx_so = probe_onnxruntime_so(app_handle).await;
    let engine = route_engine(&engine_name, data_dir.clone(), onnx_so)?;
    let is_offline = engine_name == "offline";
    let input = input.clone();

    let started = Instant::now();
    let output = tokio::task::spawn_blocking(move || -> Result<OcrOutput> {
        // 图片校验（文件存在、字节数与尺寸匹配、像素数上限）
        let image = RgbaImage::load_from_file(
            Path::new(&input.image.rgba_path),
            input.image.width,
            input.image.height,
        )?;
        // offline 语义：模型未解压 → 明确错误，UI 引导恢复（spec §4.2）
        if is_offline && !models::models_present(&data_dir) {
            return Err(crate::AppError::Plugin(format!(
                "plugin_ocr_recognize: models not extracted (restore via plugin_ocr_restore_models, data dir {})",
                data_dir.display()
            )));
        }
        engine.recognize(&image, input.max_side)
    })
    .await
    .map_err(|e| {
        crate::AppError::Internal(format!("plugin_ocr_recognize: blocking task failed: {e}"))
    })??;

    let mut output = output;
    output.duration_ms = started.elapsed().as_millis() as u64;
    Ok(output)
}

/// `plugin_ocr_engine_status` 命令体：available 由 onnxruntime .so 存在性决定
/// （非 Android dev 无此文件 → None 恒可用兑底）；engine_loaded 反映常驻引擎真实状态
pub async fn engine_status(data_dir: &Path, onnxruntime_so: Option<&Path>) -> OcrEngineStatus {
    let available = match onnxruntime_so {
        Some(so) => so.is_file(),
        None => true,
    };
    OcrEngineStatus {
        available,
        models_present: models::models_present(data_dir),
        models_bytes: models::models_bytes(data_dir),
        engine_loaded: super::ppocr::resident_loaded(),
        supported_engines: vec!["offline".to_string()],
    }
}

/// onnxruntime .so 完整路径（nativeLibraryDir + 文件名；官方 Android AAR 命名）
pub fn onnxruntime_so_path(native_lib_dir: &str) -> std::path::PathBuf {
    Path::new(native_lib_dir).join("libonnxruntime.so")
}

/// 探测 onnxruntime .so 完整路径：Android 经 Kotlin 桥拿 nativeLibraryDir；
/// 其他平台（桌面 dev）None → 恒可用兑底
pub async fn probe_onnxruntime_so(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let _ = app_handle;
        match crate::plugin::android_plugins::native_library_dir().await {
            Ok(dir) => Some(onnxruntime_so_path(&dir)),
            Err(e) => {
                tracing::warn!("probe onnxruntime .so: native_library_dir failed: {}", e);
                None
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app_handle;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 支持的引擎路由到 offline 实现（带上下文）
    #[test]
    fn route_supports_offline() {
        let engine = route_engine("offline", PathBuf::new(), None).expect("offline must route");
        assert_eq!(engine.engine_id(), "offline");
    }

    /// 未支持/未知 engine 值一律明确拒绝（v1 在线接缝未开放）
    #[test]
    fn route_rejects_unknown_engines() {
        for name in ["online:aws", "tesseract", ""] {
            let err = route_engine(name, PathBuf::new(), None)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unsupported engine"), "got: {}", err);
            assert!(err.contains(name), "got: {}", err);
        }
    }

    /// engine_status：.so 存在 → available；缺失 → false；None（非 Android dev）→ 恒 true
    #[tokio::test]
    async fn status_available_reflects_onnxruntime_so() {
        let dir = std::env::temp_dir().join(format!(
            "bedcode-ocr-status-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let so = dir.join("libonnxruntime.so");

        // .so 未打包：available=false
        let status = engine_status(&dir, Some(&so)).await;
        assert!(!status.available);

        // .so 就位：available=true
        std::fs::write(&so, vec![0u8; 16]).unwrap();
        let status = engine_status(&dir, Some(&so)).await;
        assert!(status.available);

        // 非 Android dev 环境（None）：恒可用兜底
        let status = engine_status(&dir, None).await;
        assert!(status.available);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// onnxruntime_so_path 拼接 nativeLibraryDir
    #[test]
    fn onnx_so_path_joins_lib_dir() {
        let p = onnxruntime_so_path("/data/app/xx/lib/arm64");
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "libonnxruntime.so");
        assert_eq!(
            p.parent().unwrap().to_str().unwrap(),
            "/data/app/xx/lib/arm64"
        );
    }
}
