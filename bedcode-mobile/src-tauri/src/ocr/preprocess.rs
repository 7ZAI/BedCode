//! RGBA 输入 → 引擎预处理（spec §4.2/§4.4）
//!
//! 解码在 Kotlin 桥完成（BitmapFactory 原生支持 JPEG/PNG/WebP/HEIC），
//! 本模块消费 RGBA8 裸字节流。05 阶段实现输入校验 + 纯函数预处理
//! （灰度/归一化/服务端降采样），07 接入 det/cls/rec 推理管线。

use crate::Result;
use std::path::Path;

/// 像素数上限防护：防畸形请求撑爆内存。
/// Kotlin 桥已降采样 ≤1600 长边（正常 ≤ ~2.5MP），此为服务端兜底（40MP）。
pub const MAX_PIXELS: u64 = 40_000_000;

/// RGBA8 解码图像（行优先，无格式头）
#[derive(Debug)]
pub struct RgbaImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl RgbaImage {
    /// 从裸 RGBA8 文件加载并校验：宽高 > 0、像素数 ≤ 上限、字节数恰为 width*height*4
    pub fn load_from_file(path: &Path, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(crate::AppError::InvalidInput(format!(
                "plugin_ocr_recognize: invalid image size {}x{} (must be > 0)",
                width, height
            )));
        }
        let pixels = (width as u64) * (height as u64);
        if pixels > MAX_PIXELS {
            return Err(crate::AppError::InvalidInput(format!(
                "plugin_ocr_recognize: image too large {}x{} ({} px, limit {})",
                width, height, pixels, MAX_PIXELS
            )));
        }
        let expected = pixels * 4;
        let data = std::fs::read(path).map_err(|e| {
            crate::AppError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "plugin_ocr_recognize: failed to read RGBA file {}: {}",
                    path.display(),
                    e
                ),
            ))
        })?;
        if data.len() as u64 != expected {
            return Err(crate::AppError::InvalidInput(format!(
                "plugin_ocr_recognize: RGBA file {} size mismatch: {} bytes, expected {} ({}x{}x4)",
                path.display(),
                data.len(),
                expected,
                width,
                height
            )));
        }
        Ok(Self {
            pixels: data,
            width,
            height,
        })
    }

    /// RGB 视图（去 alpha）：每像素取前 3 通道，供 OCR 流水线（模型输入按 BGR 序取通道）
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() / 4 * 3);
        for p in self.pixels.chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
        out
    }

    /// 灰度化（BT.601 加权平均，alpha 忽略）
    pub fn to_gray(&self) -> Vec<u8> {
        self.pixels
            .chunks_exact(4)
            .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).round() as u8)
            .collect()
    }

    /// 归一化到 [0,1] f32（逐通道）。
    /// PP-OCR 的 ImageNet mean/std 归一化在 07 按导出模型预处理参数确定。
    pub fn to_normalized_f32(&self) -> Vec<f32> {
        self.pixels.iter().map(|&b| b as f32 / 255.0).collect()
    }

    /// 服务端二次降采样：长边 ≤ max_side（最近邻；保持 RGBA8 布局）；
    /// 长边未超限则原样返回（克隆）。07 按精度需求可换块平均。
    pub fn downscale_max_side(&self, max_side: u32) -> (Vec<u8>, u32, u32) {
        let max_side = max_side.max(1);
        let long_side = self.width.max(self.height) as f32;
        let scale = ((max_side as f32) / long_side).min(1.0);
        if scale >= 1.0 {
            return (self.pixels.clone(), self.width, self.height);
        }
        let dst_w = ((self.width as f32) * scale).floor().max(1.0) as u32;
        let dst_h = ((self.height as f32) * scale).floor().max(1.0) as u32;
        let mut out = Vec::with_capacity((dst_w * dst_h * 4) as usize);
        for y in 0..dst_h {
            let sy = (y as f32 / scale) as u32;
            for x in 0..dst_w {
                let sx = (x as f32 / scale) as u32;
                let src = ((sy * self.width + sx) * 4) as usize;
                out.extend_from_slice(&self.pixels[src..src + 4]);
            }
        }
        (out, dst_w, dst_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(tag: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bedcode-ocr-preprocess-test-{}-{}.rgba",
            tag,
            std::process::id()
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn load_ok_when_size_matches() {
        let path = temp_file("ok", &[0u8; 4]);
        let img = RgbaImage::load_from_file(&path, 1, 1).unwrap();
        assert_eq!(img.pixels.len(), 4);
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
    }

    /// 字节数与 width*height*4 不符 → 带尺寸上下文的错误
    #[test]
    fn load_rejects_size_mismatch() {
        let path = temp_file("mismatch", &[0u8; 3]);
        let err = RgbaImage::load_from_file(&path, 1, 1).unwrap_err().to_string();
        assert!(err.contains("size mismatch"), "got: {}", err);
        assert!(err.contains("3 bytes"), "got: {}", err);
        assert!(err.contains("4"), "got: {}", err);
    }

    #[test]
    fn load_rejects_zero_dimension() {
        let path = temp_file("zero-dim", &[]);
        let err = RgbaImage::load_from_file(&path, 0, 10).unwrap_err().to_string();
        assert!(err.contains("invalid image size"), "got: {}", err);
    }

    /// 像素数超上限 → 拒绝（服务端兜底）
    #[test]
    fn load_rejects_huge_pixel_count() {
        let path = temp_file("huge", &[]);
        // 文件不会真读：先触发像素数校验（9_000_000 x 9 = 81MP > 40MP）
        let err = RgbaImage::load_from_file(&path, 9_000_000, 9).unwrap_err().to_string();
        assert!(err.contains("too large"), "got: {}", err);
    }

    #[test]
    fn load_rejects_missing_file() {
        let err = RgbaImage::load_from_file(Path::new("/nonexistent/bedcode-ocr-xyz.rgba"), 1, 1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("failed to read"), "got: {}", err);
    }

    /// BT.601 灰度：纯红 → 76（0.299*255≈76.245 取整），白/黑 → 255/0
    #[test]
    fn gray_uses_weighted_average() {
        let path = temp_file("gray", &[255, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255]);
        let img = RgbaImage::load_from_file(&path, 3, 1).unwrap();
        let gray = img.to_gray();
        assert_eq!(gray, vec![76, 255, 0]);
    }

    /// 归一化：0 → 0.0，255 → 1.0，128 → 128/255
    #[test]
    fn normalize_maps_0_255_to_0_1() {
        let path = temp_file("norm", &[0, 128, 255, 255]);
        let img = RgbaImage::load_from_file(&path, 1, 1).unwrap();
        let norm = img.to_normalized_f32();
        assert_eq!(norm.len(), 4);
        assert_eq!(norm[0], 0.0);
        assert!((norm[1] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(norm[2], 1.0);
    }

    /// RGB 视图去 alpha：4 通道 → 3 通道
    #[test]
    fn to_rgb_drops_alpha() {
        let path = temp_file("rgb", &[255, 0, 0, 9, 0, 255, 0, 8, 0, 0, 255, 7]);
        let img = RgbaImage::load_from_file(&path, 3, 1).unwrap();
        assert_eq!(img.to_rgb(), vec![255, 0, 0, 0, 255, 0, 0, 0, 255]);
    }

    /// 降采样：8x4 → 长边 4 → 4x2，RGBA 布局不变
    #[test]
    fn downscale_reduces_long_side() {
        // 8x4 单色红图（每像素 RGBA = 255,0,0,255）
        let mut bytes = Vec::new();
        for _ in 0..(8 * 4) {
            bytes.extend_from_slice(&[255, 0, 0, 255]);
        }
        let path = temp_file("downscale", &bytes);
        let img = RgbaImage::load_from_file(&path, 8, 4).unwrap();
        let (out, w, h) = img.downscale_max_side(4);
        assert_eq!((w, h), (4, 2));
        assert_eq!(out.len(), 4 * 2 * 4);
        assert!(out.iter().all(|&b| b == 0 || b == 255));
    }

    /// 长边未超限 → 原样返回（尺寸不变、像素不变）
    #[test]
    fn downscale_noop_when_within_limit() {
        let path = temp_file("downscale-noop", &[1, 2, 3, 4]);
        let img = RgbaImage::load_from_file(&path, 1, 1).unwrap();
        let (out, w, h) = img.downscale_max_side(1600);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![1, 2, 3, 4]);
    }
}
