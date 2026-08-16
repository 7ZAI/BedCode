//! 图像操作原语：RGB 缓冲、双线性 resize、透视变换（双三次 + BORDER_REPLICATE）、旋转
//!
//! cv2 等价移植：`cv2.resize`（INTER_LINEAR）、`cv2.warpPerspective`（INTER_CUBIC +
//! BORDER_REPLICATE）、`np.rot90` / `cv2.rotate(ROTATE_180)`。
//! 通道序与 RapidOCR 一致：内存为 RGB，模型输入按 BGR 取通道（PaddleOCR 推理链
//! 对 cv2.imread 的 BGR 图直接做 ImageNet 归一化，通道序即 B,G,R）。

/// RGB 三通道图像缓冲（行优先，无 alpha）
#[derive(Debug, Clone)]
pub struct ImgBuf {
    pub pixels: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl ImgBuf {
    pub fn new(pixels: Vec<u8>, w: u32, h: u32) -> Self {
        debug_assert_eq!(pixels.len(), (w * h * 3) as usize);
        Self { pixels, w, h }
    }

    pub fn rgb_at(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.w + x) * 3) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }

    /// 每像素 3 通道索引（x, y → 缓冲偏移）
    fn off(&self, x: u32, y: u32) -> usize {
        ((y * self.w + x) * 3) as usize
    }
}

/// 双线性缩放（cv2.resize INTER_LINEAR 等价；cv2 采样：src = (dst+0.5)*scale - 0.5，越界 clamp）
pub fn resize_bilinear(img: &ImgBuf, dst_w: u32, dst_h: u32) -> ImgBuf {
    if dst_w == 0 || dst_h == 0 {
        return ImgBuf::new(Vec::new(), 0, 0);
    }
    if img.w == dst_w && img.h == dst_h {
        return img.clone();
    }
    let scale_x = img.w as f64 / dst_w as f64;
    let scale_y = img.h as f64 / dst_h as f64;
    let mut out = vec![0u8; (dst_w * dst_h * 3) as usize];
    for dy in 0..dst_h {
        let sy = ((dy as f64 + 0.5) * scale_y - 0.5).clamp(0.0, img.h as f64 - 1.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(img.h - 1);
        let ty = sy - y0 as f64;
        for dx in 0..dst_w {
            let sx = ((dx as f64 + 0.5) * scale_x - 0.5).clamp(0.0, img.w as f64 - 1.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(img.w - 1);
            let tx = sx - x0 as f64;
            let i00 = img.off(x0, y0);
            let i10 = img.off(x1, y0);
            let i01 = img.off(x0, y1);
            let i11 = img.off(x1, y1);
            let o = ((dy * dst_w + dx) * 3) as usize;
            for c in 0..3 {
                let v = (1.0 - tx) * (1.0 - ty) * img.pixels[i00 + c] as f64
                    + tx * (1.0 - ty) * img.pixels[i10 + c] as f64
                    + (1.0 - tx) * ty * img.pixels[i01 + c] as f64
                    + tx * ty * img.pixels[i11 + c] as f64;
                out[o + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    ImgBuf::new(out, dst_w, dst_h)
}

/// Keys 双三次核（a = -0.75，与 cv2 INTER_CUBIC 一致）
fn cubic_kernel(x: f64) -> f64 {
    let a = -0.75;
    let x = x.abs();
    if x <= 1.0 {
        (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

/// 双三次采样（BORDER_REPLICATE 越界复制边缘像素，与 cv2 warpPerspective 一致）
fn sample_bicubic(img: &ImgBuf, sx: f64, sy: f64) -> [u8; 3] {
    let x0 = sx.floor() as i64;
    let y0 = sy.floor() as i64;
    let clamp_x = |v: i64| v.clamp(0, img.w as i64 - 1) as u32;
    let clamp_y = |v: i64| v.clamp(0, img.h as i64 - 1) as u32;
    let mut out = [0.0f64; 3];
    for j in -1i64..=2 {
        let wy = cubic_kernel(sy - (y0 + j) as f64);
        if wy == 0.0 {
            continue;
        }
        for i in -1i64..=2 {
            let wx = cubic_kernel(sx - (x0 + i) as f64);
            if wx == 0.0 {
                continue;
            }
            let px = clamp_x(x0 + i);
            let py = clamp_y(y0 + j);
            let idx = img.off(px, py);
            let w = wx * wy;
            for c in 0..3 {
                out[c] += img.pixels[idx + c] as f64 * w;
            }
        }
    }
    [
        out[0].round().clamp(0.0, 255.0) as u8,
        out[1].round().clamp(0.0, 255.0) as u8,
        out[2].round().clamp(0.0, 255.0) as u8,
    ]
}

/// 透视变换（dst→src 的单应矩阵 H 已解好）：warpPerspective + INTER_CUBIC + BORDER_REPLICATE 等价。
/// h = [h00, h01, h02, h10, h11, h12, h20, h21, h22]，采样 src = H * (dx, dy, 1)（齐次归一）。
pub fn warp_perspective(src: &ImgBuf, h: [f64; 9], dst_w: u32, dst_h: u32) -> ImgBuf {
    let mut out = vec![0u8; (dst_w * dst_h * 3) as usize];
    for dy in 0..dst_h {
        let yd = dy as f64;
        for dx in 0..dst_w {
            let xd = dx as f64;
            let w = h[6] * xd + h[7] * yd + h[8];
            let sx = (h[0] * xd + h[1] * yd + h[2]) / w;
            let sy = (h[3] * xd + h[4] * yd + h[5]) / w;
            let px = sample_bicubic(src, sx, sy);
            let o = ((dy * dst_w + dx) * 3) as usize;
            out[o..o + 3].copy_from_slice(&px);
        }
    }
    ImgBuf::new(out, dst_w, dst_h)
}

/// 逆时针旋转 90°（np.rot90 k=1 等价）：out[y][x] = src[x][w-1-y]，输出尺寸 (src.h, src.w)
pub fn rot90_ccw(img: &ImgBuf) -> ImgBuf {
    let (w, h) = (img.w, img.h);
    let mut out = vec![0u8; (w * h * 3) as usize];
    for y in 0..w {
        for x in 0..h {
            let src = img.off(w - 1 - y, x); // src 列 = w-1-y，src 行 = x
            let dst = ((y * h + x) * 3) as usize;
            out[dst..dst + 3].copy_from_slice(&img.pixels[src..src + 3]);
        }
    }
    ImgBuf::new(out, h, w)
}

/// 旋转 180°（cv2.rotate ROTATE_180 等价）：dst[y][x] = src[h-1-y][w-1-x]
pub fn rot180(img: &ImgBuf) -> ImgBuf {
    let (w, h) = (img.w, img.h);
    let mut out = vec![0u8; (w * h * 3) as usize];
    for y in 0..h {
        for x in 0..w {
            let src = img.off(x, y);
            let dst = (((h - 1 - y) * w + (w - 1 - x)) * 3) as usize;
            out[dst..dst + 3].copy_from_slice(&img.pixels[src..src + 3]);
        }
    }
    ImgBuf::new(out, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 恒等缩放：尺寸不变
    #[test]
    fn resize_identity() {
        let img = ImgBuf::new(vec![1, 2, 3, 4, 5, 6], 2, 1);
        let out = resize_bilinear(&img, 2, 1);
        assert_eq!(out.pixels, img.pixels);
    }

    /// 2x1 → 4x1：双线性插值（纯水平），0.5 处取中点
    #[test]
    fn resize_bilinear_horizontal() {
        // 像素 A(0,0)=[0,0,0] B(1,0)=[100,100,100] → 放大 2 倍
        let img = ImgBuf::new(vec![0, 0, 0, 100, 100, 100], 2, 1);
        let out = resize_bilinear(&img, 4, 1);
        // cv2 采样: sx = (dx+0.5)*0.5-0.5: dx=0 → -0.25→clamp 0 → 0; dx=1 → 0.25; dx=2 → 0.75; dx=3 → 1.25→clamp 1
        assert_eq!(out.rgb_at(0, 0), [0, 0, 0]);
        assert_eq!(out.rgb_at(1, 0), [25, 25, 25]);
        assert_eq!(out.rgb_at(2, 0), [75, 75, 75]);
        assert_eq!(out.rgb_at(3, 0), [100, 100, 100]);
    }

    /// 双三次核：距离 0 → 1，距离 1 → 0，距离 2 → 0
    #[test]
    fn cubic_kernel_basics() {
        assert!((cubic_kernel(0.0) - 1.0).abs() < 1e-9);
        assert!(cubic_kernel(1.0).abs() < 1e-9);
        assert!(cubic_kernel(2.0).abs() < 1e-9);
        assert!(cubic_kernel(0.5) > 0.0 && cubic_kernel(0.5) < 1.0);
    }

    /// warpPerspective 恒等矩阵 = 平移复制
    #[test]
    fn warp_identity_copy() {
        let img = ImgBuf::new(vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120], 2, 2);
        let h = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let out = warp_perspective(&img, h, 2, 2);
        assert_eq!(out.pixels, img.pixels);
    }

    /// warp 平移 (+1, 0)：dst(0,0) 取 src(1,0)
    #[test]
    fn warp_translate() {
        let img = ImgBuf::new(vec![1, 1, 1, 2, 2, 2], 2, 1);
        // dst→src: sx = dx + 1
        let h = [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let out = warp_perspective(&img, h, 2, 1);
        assert_eq!(out.rgb_at(0, 0), [2, 2, 2]); // 取到 src(1,0)
        assert_eq!(out.rgb_at(1, 0), [2, 2, 2]); // src(2,0) 越界 → replicate 边缘
    }

    /// rot90 逆时针：2x1 → 1x2
    #[test]
    fn rot90_ccw_basic() {
        let img = ImgBuf::new(vec![1, 0, 0, 2, 0, 0], 2, 1);
        let out = rot90_ccw(&img);
        assert_eq!((out.w, out.h), (1, 2));
        assert_eq!(out.rgb_at(0, 0), [2, 0, 0]);
        assert_eq!(out.rgb_at(0, 1), [1, 0, 0]);
    }

    /// rot180：像素镜像
    #[test]
    fn rot180_basic() {
        let img = ImgBuf::new(vec![1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0], 2, 2);
        let out = rot180(&img);
        assert_eq!(out.rgb_at(0, 0), [4, 0, 0]);
        assert_eq!(out.rgb_at(1, 1), [1, 0, 0]);
        assert_eq!(out.rgb_at(1, 0), [3, 0, 0]);
        assert_eq!(out.rgb_at(0, 1), [2, 0, 0]);
    }
}
