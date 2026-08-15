//! offline 引擎：PP-OCRv4 三段流水线（det/cls/rec）+ ort session
//!
//! 05 阶段为骨架：`recognize` 返回明确「未实现」错误（模型存在性已在命令层校验）；
//! 真实推理（det → cls → rec + 行排序 + 置信度）在票据 07。

use super::engine::OcrEngine;
use super::preprocess::RgbaImage;
use super::OcrOutput;
use crate::Result;

/// PP-OCRv4 离线引擎（unit struct；07 起持有 ort session 常驻实例，惰性加载见 §4.3）
#[derive(Debug)]
pub struct PpOcrEngine;

impl OcrEngine for PpOcrEngine {
    fn engine_id(&self) -> &'static str {
        "offline"
    }

    fn recognize(&self, _image: &RgbaImage, _max_side: Option<u32>) -> Result<OcrOutput> {
        Err(crate::AppError::Internal(
            "plugin_ocr_recognize: offline recognition pipeline not implemented yet (ticket 07)"
                .into(),
        ))
    }
}
