//! OCR 引擎模块（移动端离线识别，宿主侧）
//!
//! 服务插件 `com.bedcode.ocr`：命令契约见插件规格《移动端 OCR 插件》§4.2，
//! 引擎生命周期 §4.3，模型资源 §4.5。识别数据（RGBA 像素）不经 WASM，
//! 插件前端经宿主命令直调本模块。
//!
//! v1 仅 offline 引擎（PP-OCRv4 ONNX + ort）；v2 在线适配器经
//! [`engine::OcrEngine`] trait 接入（engine 字段路由已就位）。

pub mod engine;
pub mod models;
pub mod ppocr;
pub mod preprocess;

#[cfg(test)]
mod tests {
    use super::*;

    /// OcrImageSource 序列化为 camelCase 契约（前端直接消费）
    #[test]
    fn image_source_serializes_camel_case() {
        let src = OcrImageSource {
            path: "/cache/ocr/ocr_1.rgba".to_string(),
            width: 1000,
            height: 750,
        };
        let v = serde_json::to_value(&src).unwrap();
        assert_eq!(v["path"], "/cache/ocr/ocr_1.rgba");
        assert_eq!(v["width"], 1000);
        assert_eq!(v["height"], 750);
    }
}

use serde::{Deserialize, Serialize};

// ==================== 命令契约类型（spec §4.2）====================

/// Kotlin 桥取图解码产物（相册选图/拍照 → RGBA8 临时文件，spec §4.4/§5）
///
/// `plugin_pick_image` / `plugin_camera_capture` 返回；前端直接将其
/// 作为 `plugin_ocr_recognize` 的 `image.rgbaPath` 参数——全程不经 WASM。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrImageSource {
    /// RGBA8 纯像素文件绝对路径（app cache ocr/ 目录，识别完成后宿主删除）
    pub path: String,
    /// 降采样 + EXIF 旋转后的实际宽（识别坐标系）
    pub width: u32,
    pub height: u32,
}

/// `plugin_ocr_recognize` 请求中的图片（Kotlin 桥已解码降采样，见 §4.4）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrImageInput {
    /// RGBA8 裸字节流文件路径（app cache，无格式头）
    pub rgba_path: String,
    pub width: u32,
    pub height: u32,
}

/// `plugin_ocr_recognize` 请求（JSON，camelCase）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRecognizeInput {
    /// 引擎标识：v1 固定 "offline"（缺省同值）；其他值返回 unsupported engine
    #[serde(default)]
    pub engine: Option<String>,
    pub image: OcrImageInput,
    /// 服务端二次降采样长边上限（默认 1600）
    #[serde(default)]
    pub max_side: Option<u32>,
}

/// 文本行包围盒（原图坐标系，Kotlin 解码尺寸）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 单行识别结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    /// rec 模型逐行置信度（0~1），低置信度行 UI 可弱化
    pub confidence: f32,
    pub bbox: OcrBBox,
}

/// `plugin_ocr_recognize` 响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrOutput {
    pub engine: String,
    pub duration_ms: u64,
    /// 按阅读顺序（自上而下、行内从左到右），空文本行已剔除
    pub lines: Vec<OcrLine>,
}

/// `plugin_ocr_engine_status` 响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrEngineStatus {
    /// 当前 ABI 是否含 onnxruntime + 引擎编译可用（v1 恒 true，为 v2 兜底）
    pub available: bool,
    pub models_present: bool,
    pub models_bytes: u64,
    /// 识别引擎是否已加载常驻（惰性加载见 §4.3）
    pub engine_loaded: bool,
    /// v1 恒 ["offline"]，v2 追加在线 provider id
    pub supported_engines: Vec<String>,
}

/// `plugin_ocr_delete_models` 响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrDeleteModelsOutput {
    pub deleted: bool,
    pub freed_bytes: u64,
}

/// `plugin_ocr_restore_models` 响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRestoreModelsOutput {
    pub restored: bool,
}
