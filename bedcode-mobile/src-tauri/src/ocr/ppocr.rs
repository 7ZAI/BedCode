//! offline 引擎：PP-OCRv4 三段流水线（det → cls → rec）+ ort session 常驻实例
//!
//! 生命周期（spec §4.3）：首次 `recognize` 惰性初始化（3 个 ort session 建图），
//! 成功后常驻缓存；识别全程在 `spawn_blocking` 后台线程（engine.rs 包办）；
//! 并发请求经互斥锁单飞串行（session run 需 &mut，锁天然串行）；
//! `plugin_ocr_delete_models` 先 `reset_resident()` 释放 session 再删模型文件。
//!
//! 纯逻辑（几何/图像/前后处理/解码）在 `ppocr::{geom,imgops,pipeline,dict}`，
//! Windows `cargo test` 可单测；本文件 ort 相关代码仅 `target_os = "android"` 编译。

pub mod dict;
pub mod geom;
pub mod imgops;
pub mod pipeline;

use std::path::PathBuf;
#[cfg(target_os = "android")]
use std::sync::{Mutex, OnceLock};

use super::engine::OcrEngine;
use super::preprocess::RgbaImage;
#[cfg(target_os = "android")]
use super::OcrLine;
use super::OcrOutput;
use crate::Result;

/// 模型文件名（与 APK assets `resources/ocr_models/` 及 Kotlin 解压器一致）
pub const DET_MODEL: &str = "ch_PP-OCRv4_det_infer.onnx";
pub const CLS_MODEL: &str = "ch_ppocr_mobile_v2.0_cls_infer.onnx";
pub const REC_MODEL: &str = "ch_PP-OCRv4_rec_infer.onnx";

/// 引擎构造上下文（命令层提供：模型目录 + onnxruntime .so 路径）
#[derive(Debug, Clone)]
pub struct PpOcrContext {
    /// app 数据目录（`ocr_models/` 位于其下）
    pub data_dir: PathBuf,
    /// libonnxruntime.so 完整路径（Android nativeLibraryDir；其他平台 None）
    pub onnxruntime_so: Option<PathBuf>,
}

/// PP-OCRv4 离线引擎；状态在模块级常驻缓存（跨命令实例共享）
#[derive(Debug)]
pub struct PpOcrEngine {
    /// Android 平台下在 recognize_android 通过 load_engine(&self.ctx) 初始化,
    /// 跨平台编译时此字段不被读取（无 ort 编译）
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    ctx: PpOcrContext,
}

impl PpOcrEngine {
    pub fn new(ctx: PpOcrContext) -> Self {
        Self { ctx }
    }
}

/// 常驻引擎（android）：加载成功后缓存，App 存活期间不主动卸载（spec §4.3）。
/// Mutex 同时充当单飞队列：run 需要 &mut Session，识别全程持锁 → 并发请求串行。
#[cfg(target_os = "android")]
struct LoadedEngine {
    det: ort::session::Session,
    cls: ort::session::Session,
    rec: ort::session::Session,
}

#[cfg(target_os = "android")]
static RESIDENT: OnceLock<Mutex<Option<LoadedEngine>>> = OnceLock::new();

/// 常驻引擎是否已加载（engine_status.engine_loaded）
pub fn resident_loaded() -> bool {
    #[cfg(target_os = "android")]
    {
        RESIDENT
            .get()
            .and_then(|m| m.lock().ok())
            .map(|g| g.is_some())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

/// 释放常驻引擎（plugin_ocr_delete_models 调用；先释放 session 再删模型文件）
pub fn reset_resident() {
    #[cfg(target_os = "android")]
    {
        if let Some(m) = RESIDENT.get() {
            if let Ok(mut g) = m.lock() {
                *g = None;
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        // 非 Android 无常驻引擎（无 ort 编译）
    }
}

impl OcrEngine for PpOcrEngine {
    fn engine_id(&self) -> &'static str {
        "offline"
    }

    fn recognize(&self, image: &RgbaImage, max_side: Option<u32>) -> Result<OcrOutput> {
        #[cfg(target_os = "android")]
        {
            self.recognize_android(image, max_side)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (image, max_side);
            Err(crate::AppError::Internal(
                "plugin_ocr_recognize: offline engine requires Android (onnxruntime .so is only packaged for android targets)"
                    .into(),
            ))
        }
    }
}

#[cfg(target_os = "android")]
impl PpOcrEngine {
    fn resident() -> &'static Mutex<Option<LoadedEngine>> {
        RESIDENT.get_or_init(|| Mutex::new(None))
    }

    /// 识别主流程：降采样 → det → 排序 → crop → cls → rec → 行过滤 → bbox 换算原图
    fn recognize_android(&self, image: &RgbaImage, max_side: Option<u32>) -> Result<OcrOutput> {
        // 服务端二次降采样（Kotlin 已 ≤1600；此处兜底 maxSide）
        let (work_px, work_w, work_h) = {
            let limit = max_side.unwrap_or(1600);
            if image.width.max(image.height) > limit {
                image.downscale_max_side(limit)
            } else {
                (image.to_rgb(), image.width, image.height)
            }
        };
        let work = imgops::ImgBuf::new(work_px, work_w, work_h);
        let scale_x = image.width as f64 / work_w as f64;
        let scale_y = image.height as f64 / work_h as f64;

        // 单飞：锁覆盖「惰性加载 + 识别全程」，并发 recognize 串行执行
        let mut guard = Self::resident()
            .lock()
            .map_err(|_| crate::AppError::Internal("plugin_ocr_recognize: ocr engine mutex poisoned".into()))?;
        if guard.is_none() {
            *guard = Some(load_engine(&self.ctx)?);
        }
        let eng = guard.as_mut().expect("loaded above");

        // ---------- det ----------
        let (det_t, det_w, det_h) = pipeline::det_preprocess(&work);
        let det_arr = ndarray::Array4::from_shape_vec((1usize, 3usize, det_h as usize, det_w as usize), det_t)
            .map_err(|e| crate::AppError::Internal(format!("plugin_ocr_recognize: det tensor build: {e}")))?;
        let det_value = ort::value::Tensor::from_array(det_arr).map_err(ort_err("det input"))?;
        let det_out = eng.det.run([det_value.into()]).map_err(ort_err("det session run"))?;
        let det_prob = det_out[0].try_extract_array::<f32>().map_err(ort_err("det output"))?;
        let pred_h = det_prob.shape()[2];
        let pred_w = det_prob.shape()[3];
        let pred: Vec<f32> = det_prob.iter().copied().collect();
        drop(det_out);

        let boxes = pipeline::det_postprocess(&pred, pred_w, pred_h, work_w as usize, work_h as usize);
        let boxes = pipeline::filter_det_res(boxes, work_w as usize, work_h as usize);
        let boxes = pipeline::sort_boxes_reading_order(boxes);

        // ---------- crop → cls → rec ----------
        let mut lines: Vec<OcrLine> = Vec::new();
        for (quad, _score) in &boxes {
            let crop = match pipeline::crop_rotate(&work, quad) {
                Some(c) => c,
                None => continue, // 退化框（宽或高 <1）
            };

            // cls：180° 旋转矫正（置信度 >0.9）
            let (cls_t, _) = pipeline::cls_preprocess(&crop);
            let cls_arr = ndarray::Array4::from_shape_vec(
                (
                    1usize,
                    3usize,
                    pipeline::CLS_IMG_H as usize,
                    pipeline::CLS_IMG_W as usize,
                ),
                cls_t,
            )
            .map_err(|e| crate::AppError::Internal(format!("plugin_ocr_recognize: cls tensor build: {e}")))?;
            let cls_value = ort::value::Tensor::from_array(cls_arr).map_err(ort_err("cls input"))?;
            let cls_out = eng.cls.run([cls_value.into()]).map_err(ort_err("cls session run"))?;
            let cls_prob = cls_out[0].try_extract_array::<f32>().map_err(ort_err("cls output"))?;
            let crop = pipeline::cls_apply(&[cls_prob[[0, 0]], cls_prob[[0, 1]]], crop);
            drop(cls_out);

            // rec：动态宽（max_wh_ratio 取模型基线 320/48 与行宽高比之大者）
            let wh = crop.w as f64 / crop.h as f64;
            let max_wh = (pipeline::REC_MAX_W / pipeline::REC_IMG_H as f64).max(wh);
            let rec_w = pipeline::rec_width(max_wh);
            let (rec_t, _) = pipeline::rec_preprocess(&crop, rec_w);
            let rec_arr =
                ndarray::Array4::from_shape_vec((1usize, 3usize, pipeline::REC_IMG_H as usize, rec_w as usize), rec_t)
                    .map_err(|e| crate::AppError::Internal(format!("plugin_ocr_recognize: rec tensor build: {e}")))?;
            let rec_value = ort::value::Tensor::from_array(rec_arr).map_err(ort_err("rec input"))?;
            let rec_out = eng.rec.run([rec_value.into()]).map_err(ort_err("rec session run"))?;
            let rec_probs = rec_out[0].try_extract_array::<f32>().map_err(ort_err("rec output"))?;
            let t = rec_probs.shape()[1];
            let vocab = rec_probs.shape()[2];
            let probs: Vec<f32> = rec_probs.iter().copied().collect();
            drop(rec_out);

            let (text, conf) = pipeline::ctc_decode(&probs, t, vocab);
            if conf < pipeline::TEXT_SCORE {
                continue; // 低置信度 + 空文本行剔除（空行 conf=0）
            }
            lines.push(OcrLine {
                text,
                confidence: conf,
                bbox: pipeline::quad_to_bbox(quad, scale_x, scale_y),
            });
        }

        Ok(OcrOutput {
            engine: "offline".into(),
            duration_ms: 0, // 命令层回填
            lines,
        })
    }
}

/// 惰性加载：dlopen onnxruntime + 建 3 个 session；失败返回带原因错误（下次调用重试）
#[cfg(target_os = "android")]
fn load_engine(ctx: &PpOcrContext) -> Result<LoadedEngine> {
    let so = ctx.onnxruntime_so.as_ref().ok_or_else(|| {
        crate::AppError::Plugin(
            "plugin_ocr_recognize: onnxruntime .so path unavailable (nativeLibraryDir probe failed)".into(),
        )
    })?;
    if !so.is_file() {
        return Err(crate::AppError::Plugin(format!(
            "plugin_ocr_recognize: libonnxruntime.so not found at {} (re-run scripts/fetch-ort-android.sh)",
            so.display()
        )));
    }
    // dlopen 全局环境（幂等：commit 返回 false 表示已被本进程初始化过）
    ort::init_from(so)
        .map_err(|e| {
            crate::AppError::Plugin(format!(
                "plugin_ocr_recognize: failed to dlopen onnxruntime at {}: {e}",
                so.display()
            ))
        })?
        .with_name("bedcode-ocr")
        .with_telemetry(false)
        .commit();

    let dir = super::models::models_dir(&ctx.data_dir);
    let build = |name: &str| -> Result<ort::session::Session> {
        let path = dir.join(name);
        ort::session::Session::builder()
            .map_err(|e| {
                crate::AppError::Internal(format!(
                    "plugin_ocr_recognize: session builder for {} failed: {e}",
                    path.display()
                ))
            })?
            .commit_from_file(&path)
            .map_err(|e| {
                crate::AppError::Plugin(format!(
                    "plugin_ocr_recognize: failed to load model {}: {e}",
                    path.display()
                ))
            })
    };
    let det = build(DET_MODEL)?;
    let cls = build(CLS_MODEL)?;
    let rec = build(REC_MODEL)?;
    tracing::info!("OCR engine loaded (det/cls/rec sessions) from {}", dir.display());
    Ok(LoadedEngine { det, cls, rec })
}

/// ort 错误 → 带操作上下文的 AppError
#[cfg(target_os = "android")]
fn ort_err(op: &'static str) -> impl Fn(ort::Error) -> crate::AppError + 'static {
    move |e| crate::AppError::Plugin(format!("plugin_ocr_recognize: {op} failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows（非 Android）上 recognize 返回明确错误（ort 不编译，推理仅 android）
    #[test]
    fn offline_engine_rejects_off_android() {
        let engine = PpOcrEngine::new(PpOcrContext {
            data_dir: PathBuf::from("/tmp"),
            onnxruntime_so: None,
        });
        let image = RgbaImage {
            pixels: vec![0u8; 4],
            width: 1,
            height: 1,
        };
        let err = engine.recognize(&image, None).unwrap_err().to_string();
        assert!(err.contains("requires Android"), "got: {}", err);
    }

    /// 常驻缓存：非 Android 恒未加载；reset 幂等
    #[test]
    fn resident_never_loaded_off_android() {
        assert!(!resident_loaded());
        reset_resident();
        assert!(!resident_loaded());
    }
}
