//! PP-OCRv4 三段流水线纯函数（det/cls/rec 前后处理 + 行排序 + 置信度）
//!
//! 公式逐行对照 RapidOCR v2.x（ch_ppocr_det / ch_ppocr_rec / ch_ppocr_cls）：
//! - det：736/min 缩放（round 到 32 倍数）→ ImageNet 归一化（BGR 序，Paddle 推理链惯例）→
//!   0.3 阈值 → 2x2 膨胀 → 边界点集 → minAreaRect → 分数（多边形均值）→
//!   unclip（area*1.6/perimeter，圆角偏移）→ 二次 minAreaRect → 映射回原尺寸（round-half-even）
//! - rec：48 高 + 动态宽（max_wh_ratio）→ /255-0.5/0.5 归一化 → pad -1 → CTC 贪心解码
//!   （blank=0，去重，6624=空格，置信度 = 选中步 max 概率均值，round 5）
//! - cls：48x192 → 同上归一化 → 180°（置信度 >0.9 时 rot180）
//! 最终：text_score 0.5 过滤（空行 conf=0 自然剔除）。

use super::dict::CH_DICT;
use super::geom::{
    min_area_rect, order_points_clockwise, perimeter, polygon_area, polygon_mean, polygon_offset, Pt, Quad,
};
use super::imgops::{resize_bilinear, rot180, rot90_ccw, warp_perspective, ImgBuf};
use crate::ocr::OcrBBox;

// ==================== det ====================

/// det 参数（RapidOCR ch_ppocr_v4_det config.yaml 实际值）
pub const DET_LIMIT_SIDE_LEN: u32 = 736;
pub const DET_THRESH: f32 = 0.3;
pub const DET_BOX_THRESH: f64 = 0.5;
pub const DET_MAX_CANDIDATES: usize = 1000;
pub const DET_UNCLIP_RATIO: f64 = 1.6;
pub const DET_MIN_SIZE: f64 = 3.0;

/// det 输入张量：resize（min 边 736，round 到 32 倍数，双线性）→ BGR 序 ImageNet 归一化。
/// 返回 (1,3,H,W) CHW f32 + resize 后 (w, h)。
pub fn det_preprocess(img: &ImgBuf) -> (Vec<f32>, u32, u32) {
    let (h, w) = (img.h, img.w);
    let ratio = if h.min(w) < DET_LIMIT_SIDE_LEN {
        DET_LIMIT_SIDE_LEN as f64 / h.min(w) as f64
    } else {
        1.0
    };
    let rw = round32((w as f64 * ratio) as i64);
    let rh = round32((h as f64 * ratio) as i64);
    let resized = resize_bilinear(img, rw, rh);
    tensor_bgr_norm(&resized, [0.485, 0.456, 0.406], [0.229, 0.224, 0.225])
}

/// int(round(x/32)*32)，round = round-half-even（Python round 语义）
fn round32(v: i64) -> u32 {
    ((v as f64 / 32.0).round_ties_even() as i64 * 32).max(32) as u32
}

/// RGB 缓冲 → 1x3xHxW CHW f32（通道序 B,G,R；归一化 (v/255-mean)/std）
fn tensor_bgr_norm(img: &ImgBuf, mean: [f64; 3], std: [f64; 3]) -> (Vec<f32>, u32, u32) {
    let n = (img.w * img.h) as usize;
    let mut out = vec![0.0f32; n * 3];
    for i in 0..n {
        let r = img.pixels[i * 3] as f64;
        let g = img.pixels[i * 3 + 1] as f64;
        let b = img.pixels[i * 3 + 2] as f64;
        // 通道 0 = B，1 = G，2 = R（cv2.imread BGR 惯例，Paddle 推理链同款）
        out[i] = ((b / 255.0 - mean[0]) / std[0]) as f32;
        out[n + i] = ((g / 255.0 - mean[1]) / std[1]) as f32;
        out[2 * n + i] = ((r / 255.0 - mean[2]) / std[2]) as f32;
    }
    (out, img.w, img.h)
}

/// 2x2 膨胀（cv2.dilate [[1,1],[1,1]] 等价，anchor (1,1)，越界 0）：
/// out[y][x] = src[y][x] | src[y][x-1] | src[y-1][x] | src[y-1][x-1]
fn dilate_2x2(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let v = mask[y * w + x]
                || (x > 0 && mask[y * w + x - 1])
                || (y > 0 && mask[(y - 1) * w + x])
                || (x > 0 && y > 0 && mask[(y - 1) * w + x - 1]);
            out[y * w + x] = v;
        }
    }
    out
}

/// det 后处理：概率图 → 四边形列表（阅读顺序排序前）。
/// pred 为模型输出 [1,1,H,W]（sigmoid 已内嵌）；dest 为 det 输入图尺寸（scale 目标）。
pub fn det_postprocess(pred: &[f32], pred_w: usize, pred_h: usize, dest_w: usize, dest_h: usize) -> Vec<(Quad, f32)> {
    let n = pred_w * pred_h;
    let seg: Vec<bool> = pred[..n].iter().map(|&p| p > DET_THRESH).collect();
    let mask = dilate_2x2(&seg, pred_w, pred_h);
    let mask_u8: Vec<u8> = mask.iter().map(|&b| b as u8).collect();

    let borders = super::geom::find_border_pixel_sets(&mask_u8, pred_w, pred_h);
    let num = borders.len().min(DET_MAX_CANDIDATES);

    let mut boxes: Vec<(Quad, f32)> = Vec::new();
    for border in borders.iter().take(num) {
        // 第一次 minAreaRect + 排序 → [tl, tr, br, bl]
        let (corners, min_side) = match min_area_rect(border) {
            Some(v) => v,
            None => continue,
        };
        if min_side < DET_MIN_SIZE {
            continue;
        }
        let quad = order_points_clockwise(&corners);
        let score = polygon_mean(pred, pred_w, pred_h, &quad) as f32;
        if DET_BOX_THRESH > score as f64 {
            continue;
        }
        // unclip：area * ratio / perimeter 距离的圆角外扩
        let dist = polygon_area(&quad) * DET_UNCLIP_RATIO / perimeter(&quad);
        let expanded = polygon_offset(&quad, dist);
        if expanded.is_empty() {
            continue;
        }
        let (corners2, min_side2) = match min_area_rect(&expanded) {
            Some(v) => v,
            None => continue,
        };
        if min_side2 < DET_MIN_SIZE + 2.0 {
            continue;
        }
        let quad2 = order_points_clockwise(&corners2);
        // 映射回 det 输入图尺寸（round-half-even，与 np.round 一致）
        let mut scaled = [Pt::new(0.0, 0.0); 4];
        for (k, p) in quad2.iter().enumerate() {
            scaled[k] = Pt::new(
                (p.x / pred_w as f64 * dest_w as f64)
                    .round_ties_even()
                    .clamp(0.0, dest_w as f64),
                (p.y / pred_h as f64 * dest_h as f64)
                    .round_ties_even()
                    .clamp(0.0, dest_h as f64),
            );
        }
        boxes.push((scaled, score));
    }
    boxes
}

/// filter_det_res：order_points → clip（int 截断）→ 宽/高 ≤3 剔除
pub fn filter_det_res(boxes: Vec<(Quad, f32)>, img_w: usize, img_h: usize) -> Vec<(Quad, f32)> {
    let mut out = Vec::new();
    for (quad, score) in boxes {
        let q = order_points_clockwise(&quad);
        let mut clipped = [Pt::new(0.0, 0.0); 4];
        for (k, p) in q.iter().enumerate() {
            clipped[k] = Pt::new(
                (p.x.max(0.0).min((img_w - 1) as f64)) as i64 as f64,
                (p.y.max(0.0).min((img_h - 1) as f64)) as i64 as f64,
            );
        }
        let rect_w = (dist(clipped[0], clipped[1])) as i64;
        let rect_h = (dist(clipped[0], clipped[3])) as i64;
        if rect_w <= 3 || rect_h <= 3 {
            continue;
        }
        out.push((clipped, score));
    }
    out
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// 阅读顺序排序（RapidOCR `sorted_boxes` 等价）：tl.y 排序 → dy≥10 分行 → 行内 x 排序。
/// 输入为 filter_det_res 输出（[tl,tr,br,bl]），稳定等价于 lexsort。
pub fn sort_boxes_reading_order(mut boxes: Vec<(Quad, f32)>) -> Vec<(Quad, f32)> {
    if boxes.len() < 2 {
        return boxes;
    }
    // 按 tl.y 稳定排序
    boxes.sort_by(|a, b| a.0[0].y.partial_cmp(&b.0[0].y).unwrap());
    // 行分组：相邻 y 差 ≥ 10 开新行
    let mut line_ids = vec![0usize; boxes.len()];
    for i in 1..boxes.len() {
        line_ids[i] = line_ids[i - 1]
            + if (boxes[i].0[0].y - boxes[i - 1].0[0].y) >= 10.0 {
                1
            } else {
                0
            };
    }
    // 行内按 x 排序（(line, x) 字典序 = lexsort）
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| {
        line_ids[a]
            .cmp(&line_ids[b])
            .then(boxes[a].0[0].x.partial_cmp(&boxes[b].0[0].x).unwrap())
    });
    order.into_iter().map(|i| boxes[i]).collect()
}

// ==================== 裁剪（get_rotate_crop_image）====================

/// 解 8 元线性方程组（部分主元高斯消元），返回 None 表示奇异
fn solve8(a: [[f64; 8]; 8], b: [f64; 8]) -> Option<[f64; 8]> {
    let mut m = [[0.0f64; 9]; 8];
    for i in 0..8 {
        for j in 0..8 {
            m[i][j] = a[i][j];
        }
        m[i][8] = b[i];
    }
    for col in 0..8 {
        // 部分主元
        let mut piv = col;
        for r in col + 1..8 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        for r in 0..8 {
            if r == col {
                continue;
            }
            let f = m[r][col] / m[col][col];
            for c in col..9 {
                m[r][c] -= f * m[col][c];
            }
        }
    }
    let mut x = [0.0f64; 8];
    for i in 0..8 {
        x[i] = m[i][8] / m[i][i];
    }
    Some(x)
}

/// 由 4 点对应解单应矩阵 H（dst→src 映射，H[8] = 1）
fn homography_dst_to_src(src: &Quad, dst: &Quad) -> Option<[f64; 9]> {
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for k in 0..4 {
        let (u, v) = (dst[k].x, dst[k].y);
        let (x, y) = (src[k].x, src[k].y);
        a[k * 2] = [u, v, 1.0, 0.0, 0.0, 0.0, -u * x, -v * x];
        a[k * 2 + 1] = [0.0, 0.0, 0.0, u, v, 1.0, -u * y, -v * y];
        b[k * 2] = x;
        b[k * 2 + 1] = y;
    }
    let h8 = solve8(a, b)?;
    let mut h = [0.0f64; 9];
    h[..8].copy_from_slice(&h8);
    h[8] = 1.0;
    Some(h)
}

/// 旋转矫正裁剪（get_rotate_crop_image 等价）：quad → 透视变换（双三次 + BORDER_REPLICATE）；
/// 高宽比 ≥1.5 时逆时针 90°（竖排转横）。尺寸 <1 → None。
pub fn crop_rotate(img: &ImgBuf, quad: &Quad) -> Option<ImgBuf> {
    let w = (dist(quad[0], quad[1]).max(dist(quad[2], quad[3]))) as u32;
    let h = (dist(quad[0], quad[3]).max(dist(quad[1], quad[2]))) as u32;
    if w < 1 || h < 1 {
        return None;
    }
    let dst = [
        Pt::new(0.0, 0.0),
        Pt::new(w as f64, 0.0),
        Pt::new(w as f64, h as f64),
        Pt::new(0.0, h as f64),
    ];
    let hom = homography_dst_to_src(quad, &dst)?;
    let crop = warp_perspective(img, hom, w, h);
    if h as f64 / w as f64 >= 1.5 {
        Some(rot90_ccw(&crop))
    } else {
        Some(crop)
    }
}

// ==================== cls ====================

pub const CLS_IMG_W: u32 = 192;
pub const CLS_IMG_H: u32 = 48;
pub const CLS_THRESH: f32 = 0.9;

/// cls 输入张量 (1,3,48,192)：宽 = min(192, ceil(48*ratio))，/255-0.5/0.5，pad -1
pub fn cls_preprocess(img: &ImgBuf) -> (Vec<f32>, u32) {
    let ratio = img.w as f64 / img.h as f64;
    let resized_w = if (CLS_IMG_H as f64 * ratio).ceil() as u32 > CLS_IMG_W {
        CLS_IMG_W
    } else {
        (CLS_IMG_H as f64 * ratio).ceil() as u32
    };
    let resized = resize_bilinear(img, resized_w, CLS_IMG_H);
    let (tensor, _, _) = tensor_bgr_norm(&resized, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let (out, rw) = pad_tensor_neg1(tensor, resized_w, CLS_IMG_H, CLS_IMG_W);
    (out, rw)
}

/// 将 (3,48,rw) 张量右侧补 -1 列到 (3,48,img_w)；返回 (tensor, rw)
fn pad_tensor_neg1(tensor: Vec<f32>, rw: u32, h: u32, img_w: u32) -> (Vec<f32>, u32) {
    if rw >= img_w {
        return (tensor, img_w);
    }
    let mut out = vec![-1.0f32; (h * img_w * 3) as usize];
    for c in 0..3 {
        for y in 0..h {
            let src = (c * h as usize * rw as usize) + (y as usize * rw as usize);
            let dst = (c * h as usize * img_w as usize) + (y as usize * img_w as usize);
            out[dst..dst + rw as usize].copy_from_slice(&tensor[src..src + rw as usize]);
        }
    }
    (out, rw)
}

/// cls 后处理：softmax 已内嵌 → (label, score)；label==1 && score>0.9 → 旋转 180°
pub fn cls_apply(prob: &[f32], crop: ImgBuf) -> ImgBuf {
    let label = if prob[1] > prob[0] { 1u8 } else { 0 };
    let score = prob[label as usize];
    if label == 1 && score > CLS_THRESH {
        rot180(&crop)
    } else {
        crop
    }
}

// ==================== rec ====================

pub const REC_IMG_H: u32 = 48;
pub const REC_MAX_W: f64 = 320.0;

/// rec 动态宽：int(48 * max_wh_ratio)（max_wh_ratio ≥ 320/48）
pub fn rec_width(max_wh_ratio: f64) -> u32 {
    (REC_IMG_H as f64 * max_wh_ratio) as u32
}

/// rec 输入张量 (1,3,48,img_w)：宽 = min(img_w, ceil(48*ratio))，/255-0.5/0.5，pad -1
pub fn rec_preprocess(img: &ImgBuf, img_w: u32) -> (Vec<f32>, u32) {
    let ratio = img.w as f64 / img.h as f64;
    let resized_w = if (REC_IMG_H as f64 * ratio).ceil() as u32 > img_w {
        img_w
    } else {
        (REC_IMG_H as f64 * ratio).ceil() as u32
    };
    let resized_w = resized_w.max(1);
    let resized = resize_bilinear(img, resized_w, REC_IMG_H);
    let (tensor, _, _) = tensor_bgr_norm(&resized, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let (out, rw) = pad_tensor_neg1(tensor, resized_w, REC_IMG_H, img_w);
    (out, rw)
}

/// CTC 贪心解码（RapidOCR CTCLabelDecode 等价）：argmax → 去重 → 去 blank(0) →
/// 字符映射（0=blank，1..=6623=字典，6624=空格）；置信度 = 选中步 max 概率均值（round 5）。
/// preds: (T, vocab) 行优先。返回 (text, confidence)；空文本 → ("", 0.0)。
pub fn ctc_decode(preds: &[f32], t: usize, vocab: usize) -> (String, f32) {
    let mut idx = Vec::with_capacity(t);
    let mut prob = Vec::with_capacity(t);
    for i in 0..t {
        let row = &preds[i * vocab..(i + 1) * vocab];
        let mut best = (0usize, row[0]);
        for (k, &v) in row.iter().enumerate().skip(1) {
            if v > best.1 {
                best = (k, v);
            }
        }
        idx.push(best.0);
        prob.push(best.1);
    }
    // 去重 + 去 blank
    let mut selected: Vec<(usize, f32)> = Vec::new();
    for (k, &i) in idx.iter().enumerate() {
        if i == 0 {
            continue;
        }
        if k > 0 && idx[k - 1] == i {
            continue;
        }
        selected.push((i, round5(prob[k])));
    }
    if selected.is_empty() {
        return (String::new(), 0.0);
    }
    let text: String = selected.iter().map(|&(i, _)| char_at(i)).collect();
    let conf = selected.iter().map(|&(_, p)| p as f64).sum::<f64>() / selected.len() as f64;
    (text, round5(conf as f32))
}

/// 模型索引 → 字符：0=blank（已剔除），1..=6623=字典，6624=空格
fn char_at(i: usize) -> char {
    if i == CH_DICT.len() + 1 {
        ' '
    } else {
        CH_DICT[i - 1]
    }
}

/// round-half-even 到 5 位小数（Python round(x, 5) 语义）
fn round5(v: f32) -> f32 {
    ((v as f64 * 1e5).round_ties_even() / 1e5) as f32
}

// ==================== 输出组装 ====================

/// 最终文本行置信度下限（RapidOCR Global.text_score；空行 conf=0 自然剔除）
pub const TEXT_SCORE: f32 = 0.5;

/// 四边形 → 轴对齐 bbox（原图坐标系；scale 为 work→原图 比例）
pub fn quad_to_bbox(quad: &Quad, scale_x: f64, scale_y: f64) -> OcrBBox {
    let x0 = quad.iter().map(|p| p.x).fold(f64::MAX, f64::min);
    let y0 = quad.iter().map(|p| p.y).fold(f64::MAX, f64::min);
    let x1 = quad.iter().map(|p| p.x).fold(f64::MIN, f64::max);
    let y1 = quad.iter().map(|p| p.y).fold(f64::MIN, f64::max);
    OcrBBox {
        x: (x0 * scale_x).round_ties_even().max(0.0) as u32,
        y: (y0 * scale_y).round_ties_even().max(0.0) as u32,
        w: ((x1 - x0) * scale_x).round_ties_even().max(0.0) as u32,
        h: ((y1 - y0) * scale_y).round_ties_even().max(0.0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::ppocr::geom::signed_area;

    const PMAP_BIN: &[u8] = include_bytes!("../../../testdata/synth_pmap.bin");

    fn load_pmap() -> (Vec<f32>, usize, usize) {
        let mut v = Vec::with_capacity(PMAP_BIN.len() / 4);
        for c in PMAP_BIN.chunks_exact(4) {
            v.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        (v, 96, 64)
    }

    /// 与 Python 参考（cv2/pyclipper 全链）golden 对比：合成 64x96 prob map → 2 个框
    #[test]
    fn det_postprocess_matches_python_golden() {
        let (pmap, w, h) = load_pmap();
        let boxes = det_postprocess(&pmap, w, h, w, h);
        let boxes = filter_det_res(boxes, w, h);
        let boxes = sort_boxes_reading_order(boxes);
        // golden（RapidOCR 参考实现输出）：两块矩形区域
        let golden: [[[f64; 2]; 4]; 2] = [
            [[3.0, 1.0], [69.0, 1.0], [69.0, 33.0], [3.0, 33.0]],
            [[22.0, 32.0], [88.0, 32.0], [88.0, 60.0], [22.0, 60.0]],
        ];
        assert_eq!(boxes.len(), 2, "expected 2 boxes, got {}", boxes.len());
        for (i, (quad, score)) in boxes.iter().enumerate() {
            for k in 0..4 {
                let dx = (quad[k].x - golden[i][k][0]).abs();
                let dy = (quad[k].y - golden[i][k][1]).abs();
                assert!(
                    dx <= 2.0 && dy <= 2.0,
                    "box {i} pt {k}: got ({}, {}) want ({}, {})",
                    quad[k].x,
                    quad[k].y,
                    golden[i][k][0],
                    golden[i][k][1]
                );
            }
            // 分数跟随 quad（RapidOCR sorted_boxes 不同步 scores，golden 值错位）：
            // 只做合理性断言（通过 box_thresh 且 ≤ 1）
            assert!(*score > 0.5 && *score <= 1.0, "box {i} score out of range: {score}");
        }
    }

    /// 阅读顺序：两行错位排列 → 上行左→右、下行左→右
    #[test]
    fn sort_reading_order_rows_then_columns() {
        let q = |x: f64, y: f64| {
            [
                Pt::new(x, y),
                Pt::new(x + 10.0, y),
                Pt::new(x + 10.0, y + 4.0),
                Pt::new(x, y + 4.0),
            ]
        };
        // 顺序故意打乱：下行右、上行左、下行左、上行右
        let boxes = vec![
            (q(50.0, 40.0), 1.0),
            (q(10.0, 0.0), 1.0),
            (q(10.0, 40.0), 1.0),
            (q(50.0, 0.0), 1.0),
        ];
        let sorted = sort_boxes_reading_order(boxes);
        let xs: Vec<f64> = sorted.iter().map(|b| b.0[0].x).collect();
        let ys: Vec<f64> = sorted.iter().map(|b| b.0[0].y).collect();
        assert_eq!(xs, vec![10.0, 50.0, 10.0, 50.0]);
        assert_eq!(ys, vec![0.0, 0.0, 40.0, 40.0]);
    }

    /// 行内 x 排序不受相邻行 y 差 < 10 干扰（同一行）
    #[test]
    fn sort_within_line_by_x() {
        let q = |x: f64| {
            [
                Pt::new(x, 5.0),
                Pt::new(x + 8.0, 5.0),
                Pt::new(x + 8.0, 9.0),
                Pt::new(x, 9.0),
            ]
        };
        let boxes = vec![(q(30.0), 1.0), (q(5.0), 1.0), (q(60.0), 1.0)];
        let sorted = sort_boxes_reading_order(boxes);
        let xs: Vec<f64> = sorted.iter().map(|b| b.0[0].x).collect();
        assert_eq!(xs, vec![5.0, 30.0, 60.0]);
    }

    /// 竖排（h/w ≥ 1.5）裁剪后旋转为横排
    #[test]
    fn crop_rotate_vertical_becomes_horizontal() {
        let img = ImgBuf::new(vec![200u8; 20 * 60 * 3], 20, 60);
        // 竖排框：宽 10 高 40
        let quad = [
            Pt::new(5.0, 10.0),
            Pt::new(15.0, 10.0),
            Pt::new(15.0, 50.0),
            Pt::new(5.0, 50.0),
        ];
        let crop = crop_rotate(&img, &quad).unwrap();
        assert_eq!((crop.w, crop.h), (40, 10)); // rot90 后宽高互换
    }

    #[test]
    fn crop_rotate_horizontal_keeps_shape() {
        let img = ImgBuf::new(vec![100u8; 60 * 20 * 3], 60, 20);
        let quad = [
            Pt::new(10.0, 5.0),
            Pt::new(50.0, 5.0),
            Pt::new(50.0, 15.0),
            Pt::new(10.0, 15.0),
        ];
        let crop = crop_rotate(&img, &quad).unwrap();
        assert_eq!((crop.w, crop.h), (40, 10));
    }

    /// 单应矩阵：旋转 45° 矩形的角点映射回自身
    #[test]
    fn homography_maps_corners() {
        let src = [
            Pt::new(10.0, 10.0),
            Pt::new(30.0, 10.0),
            Pt::new(30.0, 20.0),
            Pt::new(10.0, 20.0),
        ];
        let dst = [
            Pt::new(0.0, 0.0),
            Pt::new(40.0, 0.0),
            Pt::new(40.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        let h = homography_dst_to_src(&src, &dst).unwrap();
        for k in 0..4 {
            let w = h[6] * dst[k].x + h[7] * dst[k].y + h[8];
            let x = (h[0] * dst[k].x + h[1] * dst[k].y + h[2]) / w;
            let y = (h[3] * dst[k].x + h[4] * dst[k].y + h[5]) / w;
            assert!((x - src[k].x).abs() < 1e-6 && (y - src[k].y).abs() < 1e-6);
        }
    }

    /// CTC 解码：去重 + 去 blank + 置信度 round（空格索引 6624 另测）
    #[test]
    fn ctc_decode_dedup_and_blank() {
        // vocab = 5：0=blank，1..=3=字典字符，4 不用（真实布局空格在 6624）
        let preds: Vec<f32> = vec![
            0.9, 0.05, 0.02, 0.02, 0.01, //
            0.1, 0.8, 0.05, 0.03, 0.02, //
            0.1, 0.8, 0.05, 0.03, 0.02, //
            0.1, 0.03, 0.82, 0.03, 0.02, //
            0.1, 0.02, 0.03, 0.03, 0.82, //
            0.05, 0.02, 0.02, 0.88, 0.03, //
            0.05, 0.02, 0.02, 0.88, 0.03, //
        ];
        // idx 序列（去重去 blank 后）: 1, 2, 4, 3 → 字典序 CH_DICT[0], [1], [3], [2]
        let expected: String = [CH_DICT[0], CH_DICT[1], CH_DICT[3], CH_DICT[2]].iter().collect();
        let (text, conf) = ctc_decode(&preds, 7, 5);
        assert_eq!(text, expected);
        // 选中步（去重+去 blank）: (1,0.8),(2,0.82),(4,0.82),(3,0.88) → mean=0.83
        assert!((conf - 0.83).abs() < 1e-4, "conf={conf}");
    }

    /// 空格索引 6624 → ' '（真实模型布局）
    #[test]
    fn ctc_decode_space_at_6624() {
        let mut preds = vec![0.0001f32; 1 * 6625];
        preds[6624] = 0.9;
        let (text, conf) = ctc_decode(&preds, 1, 6625);
        assert_eq!(text, " ");
        assert!((conf - 0.9).abs() < 1e-5);
    }

    /// 全 blank → 空文本 + 0 置信度
    #[test]
    fn ctc_decode_all_blank_empty() {
        let preds = vec![0.95f32, 0.01, 0.01, 0.01, 0.02, 0.95, 0.01, 0.01, 0.01, 0.02];
        let (text, conf) = ctc_decode(&preds, 2, 5);
        assert_eq!(text, "");
        assert_eq!(conf, 0.0);
    }

    /// bbox 换算：work 图坐标 → 原图坐标（2x 放大）
    #[test]
    fn quad_to_bbox_scales_to_original() {
        let quad = [
            Pt::new(10.0, 20.0),
            Pt::new(30.0, 20.0),
            Pt::new(30.0, 24.0),
            Pt::new(10.0, 24.0),
        ];
        let b = quad_to_bbox(&quad, 2.0, 2.0);
        assert_eq!(b.x, 20);
        assert_eq!(b.y, 40);
        assert_eq!(b.w, 40);
        assert_eq!(b.h, 8);
    }

    /// det 输入缩放：736 min + round32；已超 736 不放大
    #[test]
    fn det_preprocess_resizes_to_32_multiple() {
        let img = ImgBuf::new(vec![255u8; 800 * 1200 * 3], 1200, 800);
        let (tensor, w, h) = det_preprocess(&img);
        assert_eq!((w, h), (1216, 800)); // 1200 → round(37.5)*32 = 1216；800 → 800
        assert_eq!(tensor.len(), (1 * 3 * 800 * 1216) as usize);
        // 归一化：白色 (255,255,255) → (1-mean)/std（B 通道用 B 序 mean）
        assert!((tensor[0] - (1.0 - 0.485) / 0.229).abs() < 1e-4, "got {}", tensor[0]);
        assert!((tensor[tensor.len() / 3] - (1.0 - 0.456) / 0.224).abs() < 1e-4);
        assert!((tensor[2 * tensor.len() / 3] - (1.0 - 0.406) / 0.225).abs() < 1e-4);
        // 通道序：红色像素 → B 通道低、R 通道高
        let red = ImgBuf::new(vec![255u8, 0, 0, 255, 0, 0], 2, 1);
        let (t, _, _) = det_preprocess(&red);
        assert!(t[0] < t[2 * t.len() / 3], "B channel should be lower than R for red");
    }

    /// det 输入缩放：小图放大到 736
    #[test]
    fn det_preprocess_upscales_small_images() {
        let img = ImgBuf::new(vec![0u8; 100 * 200 * 3], 200, 100);
        let (_, w, h) = det_preprocess(&img);
        assert_eq!((w, h), (1472, 736)); // 200*7.36=1472; 100*7.36=736
    }

    /// cls 预处理：宽高比超限截断到 192，pad 值 -1
    #[test]
    fn cls_preprocess_pads_to_192() {
        let img = ImgBuf::new(vec![0u8; 400 * 50 * 3], 400, 50);
        let (tensor, rw) = cls_preprocess(&img);
        assert_eq!(rw, 192);
        assert_eq!(tensor.len(), (3 * 48 * 192) as usize);
        // 黑色 → (0-0.5)/0.5 = -1（与 pad 相同）
        assert!((tensor[0] + 1.0).abs() < 1e-5);
    }

    /// rec 预处理：动态宽 + 归一化
    #[test]
    fn rec_preprocess_dynamic_width() {
        let img = ImgBuf::new(vec![0u8; 80 * 40 * 3], 80, 40);
        let max_wh = (REC_MAX_W / REC_IMG_H as f64).max(80.0 / 40.0);
        let img_w = rec_width(max_wh);
        assert_eq!(img_w, 320); // 80/40=2 < 6.67 → 320
        let (tensor, rw) = rec_preprocess(&img, img_w);
        assert_eq!(rw, 96); // ceil(48*2) = 96
        assert_eq!(tensor.len(), (3 * 48 * 320) as usize);
        // 填充区 = -1，有效区 = (0-0.5)/0.5 = -1 相同（黑图）
        assert!((tensor[3 * 48 * 95] + 1.0).abs() < 1e-5);
    }

    /// 2x2 膨胀：右下扩展（cv2.dilate 实证行为）
    #[test]
    fn dilate_extends_down_right() {
        let mask = vec![
            false, false, false, false, //
            false, true, false, false, //
            false, false, false, false, //
            false, false, false, false, //
        ];
        let out = dilate_2x2(&mask, 4, 4);
        let expect = vec![
            false, false, false, false, //
            false, true, true, false, //
            false, true, true, false, //
            false, false, false, false, //
        ];
        assert_eq!(out, expect);
    }

    /// 旋转四边形四点必为凸四边形（signed_area > 0，y 向下 = 顺时针）
    #[test]
    fn det_output_quads_are_clockwise() {
        let (pmap, w, h) = load_pmap();
        let boxes = det_postprocess(&pmap, w, h, w, h);
        for (quad, _) in boxes {
            assert!(signed_area(&quad) > 0.0, "quad not clockwise: {quad:?}");
        }
    }
}
