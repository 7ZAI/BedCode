//! OcrEngine trait + engine 字段路由 + 引擎状态（spec §4.2/§4.3、§7 接缝）
//!
//! v2 在线适配器实现同一 trait，engine 字段形如 `online:<provider>`；
//! v1 仅 "offline" 可路由，其他值返回明确错误。

use super::models;
use super::preprocess::RgbaImage;
use super::{OcrEngineStatus, OcrOutput, OcrRecognizeInput};
use crate::Result;
use std::path::Path;
use std::time::Instant;
use tauri::Manager;

/// OCR 引擎 trait：v1 仅 offline（ppocr）；v2 追加在线适配器
pub trait OcrEngine: std::fmt::Debug {
    /// 引擎标识（回填响应 engine 字段）
    fn engine_id(&self) -> &'static str;

    /// 识别：image 已由命令层校验（尺寸/文件），max_side 为服务端降采样上限
    fn recognize(&self, image: &RgbaImage, max_side: Option<u32>) -> Result<OcrOutput>;
}

/// engine 字段 → 引擎路由；不支持的 engine 值返回明确错误（接缝已就位，v1 拒绝在线）
pub fn route_engine(engine: &str) -> Result<Box<dyn OcrEngine + Send + Sync>> {
    match engine {
        "offline" => Ok(Box::new(super::ppocr::PpOcrEngine)),
        _ => Err(crate::AppError::InvalidInput(format!(
            "plugin_ocr_recognize: unsupported engine '{}' (supported: offline)",
            engine
        ))),
    }
}

/// `plugin_ocr_recognize` 命令体：路由 → RGBA 校验 → 模型存在性（offline）→ 推理计时
pub async fn recognize(app_handle: &tauri::AppHandle, input: &OcrRecognizeInput) -> Result<OcrOutput> {
    let engine_name = input.engine.clone().unwrap_or_else(|| "offline".to_string());
    let engine = route_engine(&engine_name)?;

    // 图片校验（文件存在、字节数与尺寸匹配、像素数上限）
    let image = RgbaImage::load_from_file(
        Path::new(&input.image.rgba_path),
        input.image.width,
        input.image.height,
    )?;

    // offline 语义：模型未解压 → 明确错误，UI 引导恢复（spec §4.2）
    if engine_name == "offline" {
        let data_dir = app_handle.path().app_data_dir()?;
        if !models::models_present(&data_dir) {
            return Err(crate::AppError::Plugin(format!(
                "plugin_ocr_recognize: models not extracted (restore via plugin_ocr_restore_models, data dir {})",
                data_dir.display()
            )));
        }
    }

    let started = Instant::now();
    let mut output = engine.recognize(&image, input.max_side)?;
    output.duration_ms = started.elapsed().as_millis() as u64;
    Ok(output)
}

/// `plugin_ocr_engine_status` 命令体：v1 恒 available；engine_loaded 待 07 常驻引擎后真实化
pub async fn engine_status(data_dir: &Path) -> OcrEngineStatus {
    OcrEngineStatus {
        available: true,
        models_present: models::models_present(data_dir),
        models_bytes: models::models_bytes(data_dir),
        engine_loaded: false,
        supported_engines: vec!["offline".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 支持的引擎路由到 offline 实现
    #[test]
    fn route_supports_offline() {
        let engine = route_engine("offline").expect("offline must route");
        assert_eq!(engine.engine_id(), "offline");
    }

    /// 未支持/未知 engine 值一律明确拒绝（v1 在线接缝未开放）
    #[test]
    fn route_rejects_unknown_engines() {
        for name in ["online:aws", "tesseract", ""] {
            let err = route_engine(name).unwrap_err().to_string();
            assert!(err.contains("unsupported engine"), "got: {}", err);
            assert!(err.contains(name), "got: {}", err);
        }
    }

    /// 骨架引擎（07 前）：模型就位后仍返回明确未实现错误
    #[test]
    fn offline_engine_is_skeleton_until_ticket_07() {
        let engine = route_engine("offline").unwrap();
        let image = RgbaImage {
            pixels: vec![0u8; 4],
            width: 1,
            height: 1,
        };
        let err = engine.recognize(&image, None).unwrap_err().to_string();
        assert!(err.contains("not implemented"), "got: {}", err);
    }
}
