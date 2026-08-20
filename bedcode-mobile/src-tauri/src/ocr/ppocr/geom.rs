//! DB 后处理几何原语（cv2/pyclipper 等价移植，纯函数可单测）
//!
//! 参考实现：RapidOCR v2.x `ch_ppocr_det/utils.py`（DBPostProcess）——
//! `cv2.findContours`（RETR_LIST）→ `cv2.minAreaRect` → `cv2.fillPoly` 均值 →
//! pyclipper `ClipperOffset`（JT_ROUND）→ 二次 `minAreaRect`。
//!
//! 移植说明：
//! - findContours 用「连通分量 + 边界像素点集」等价替代（Suzuki-Abe 的输出点集
//!   与本实现的点集一致——都是区域边界像素；后续 minAreaRect 只依赖点集的凸包，
//!   与链的起点/走向/疏密无关，因此结果与 cv2 一致）。
//! - 坐标一律 (x, y)，y 向下（图像坐标系），与 cv2 相同。

/// 二维点（图像坐标系，y 向下）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// 四边形（四点序：[tl, tr, br, bl]，见 `order_points_clockwise`）
pub type Quad = [Pt; 4];

/// 有符号面积（y 向下坐标系）：>0 表示顶点按顺时针排列
pub fn signed_area(poly: &[Pt]) -> f64 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        s += a.x * b.y - b.x * a.y;
    }
    s / 2.0
}

/// 多边形周长（闭合）
pub fn perimeter(poly: &[Pt]) -> f64 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        s += ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    }
    s
}

/// 多边形面积（绝对值）
pub fn polygon_area(poly: &[Pt]) -> f64 {
    signed_area(poly).abs()
}

// ==================== 二值图边界点集（findContours RETR_LIST 等价）====================

/// 从二值 mask（0/1）提取全部边界点集：每个 8-连通前景分量的外边界 +
/// 每个 4-连通孔洞的边界（RETR_LIST 语义：所有边界，无层级）。
/// 返回点集列表（每项为该边界的全部边界像素，顺序与 cv2 的链序不同但点集相同）。
pub fn find_border_pixel_sets(mask: &[u8], w: usize, h: usize) -> Vec<Vec<Pt>> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    // 前景 8-连通分量标记
    let mut comp = vec![-1i32; w * h];
    let mut n_comp = 0i32;
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if mask[start] != 0 && comp[start] < 0 {
            comp[start] = n_comp;
            stack.push(start);
            while let Some(p) = stack.pop() {
                let x = p % w;
                let y = p / w;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let q = (ny as usize) * w + nx as usize;
                        if mask[q] != 0 && comp[q] < 0 {
                            comp[q] = n_comp;
                            stack.push(q);
                        }
                    }
                }
            }
            n_comp += 1;
        }
    }

    // 背景 4-连通 flood：从图像边界向内标记「外部背景」
    let mut ext_bg = vec![false; w * h];
    let mut q: Vec<usize> = Vec::new();
    for x in 0..w {
        for &y in &[0usize, h - 1] {
            let p = y * w + x;
            if mask[p] == 0 && !ext_bg[p] {
                ext_bg[p] = true;
                q.push(p);
            }
        }
    }
    for y in 0..h {
        for &x in &[0usize, w - 1] {
            let p = y * w + x;
            if mask[p] == 0 && !ext_bg[p] {
                ext_bg[p] = true;
                q.push(p);
            }
        }
    }
    while let Some(p) = q.pop() {
        let x = p % w;
        let y = p / w;
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue;
            }
            let q2 = (ny as usize) * w + nx as usize;
            if mask[q2] == 0 && !ext_bg[q2] {
                ext_bg[q2] = true;
                q.push(q2);
            }
        }
    }

    // 每个分量的边界点集（外边界）+ 孔洞边界（独立收集：1px 分隔条像素可同属两者）
    let mut borders: Vec<Vec<Pt>> = Vec::new();
    for c in 0..n_comp {
        let mut outer: Vec<Pt> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let p = y * w + x;
                if comp[p] != c {
                    continue;
                }
                // 8 邻域含图像外/外部背景 → 外边界像素
                let mut on_outer = false;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        let outside = nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64;
                        if outside
                            || (!outside
                                && mask[(ny as usize) * w + nx as usize] == 0
                                && ext_bg[(ny as usize) * w + nx as usize])
                        {
                            on_outer = true;
                            break;
                        }
                    }
                    if on_outer {
                        break;
                    }
                }
                if on_outer {
                    outer.push(Pt::new(x as f64, y as f64));
                }
            }
        }
        if !outer.is_empty() {
            borders.push(outer);
        }
        // 洞边界 = 与「洞背景分量」4-邻接的该分量前景像素（每个洞一个点集）
        // 先标记所有洞像素的连通分量
        let mut hole_comp = vec![-1i32; w * h];
        let mut nh = 0i32;
        for start in 0..w * h {
            if mask[start] == 0 && !ext_bg[start] && hole_comp[start] < 0 {
                hole_comp[start] = nh;
                let mut st = vec![start];
                while let Some(p) = st.pop() {
                    let x = p % w;
                    let y = p / w;
                    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let q2 = (ny as usize) * w + nx as usize;
                        if mask[q2] == 0 && !ext_bg[q2] && hole_comp[q2] < 0 {
                            hole_comp[q2] = nh;
                            st.push(q2);
                        }
                    }
                }
                nh += 1;
            }
        }
        for hc in 0..nh {
            let mut pts: Vec<Pt> = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let p = y * w + x;
                    if comp[p] != c {
                        continue;
                    }
                    let mut on_hole = false;
                    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let q2 = (ny as usize) * w + nx as usize;
                        if mask[q2] == 0 && !ext_bg[q2] && hole_comp[q2] == hc {
                            on_hole = true;
                            break;
                        }
                    }
                    if on_hole {
                        pts.push(Pt::new(x as f64, y as f64));
                    }
                }
            }
            if !pts.is_empty() {
                borders.push(pts);
            }
        }
    }
    borders
}

// ==================== 凸包 + 最小面积外接矩形（cv2.minAreaRect 等价）====================

/// Andrew 单调链凸包（逆时针，不含共线中间点）；点不足 3 个或全共线 → None
pub fn convex_hull(points: &[Pt]) -> Option<Vec<Pt>> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.partial_cmp(&b.y).unwrap()));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
    if pts.len() < 3 {
        return None;
    }
    // 叉积（y 向下坐标系）：>0 为左转（在标准数学系中；这里只用于构造单调链）
    fn cross(o: &Pt, a: &Pt, b: &Pt) -> f64 {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    }
    let mut lower: Vec<Pt> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(&lower[lower.len() - 2], &lower[lower.len() - 1], &p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(&upper[upper.len() - 2], &upper[upper.len() - 1], &p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return None;
    }
    Some(lower)
}

/// 旋转卡尺最小面积外接矩形。
/// 返回 (4 角点任意序, 短边长)；点集退化（<3 点/共线）→ None。
pub fn min_area_rect(points: &[Pt]) -> Option<(Quad, f64)> {
    let hull = convex_hull(points)?;
    let n = hull.len();
    // 最优方向：area, ux, uy, vx, vy, xmin, xmax, ymin, ymax（沿 u/v 投影）
    let mut best: Option<(f64, f64, f64, f64, f64, f64, f64, f64, f64)> = None;
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let ex = b.x - a.x;
        let ey = b.y - a.y;
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (ux, uy) = (ex / len, ey / len);
        let (vx, vy) = (-uy, ux);
        let (mut xmin, mut xmax) = (f64::MAX, f64::MIN);
        let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
        for &p in &hull {
            let xp = p.x * ux + p.y * uy;
            let yp = p.x * vx + p.y * vy;
            xmin = xmin.min(xp);
            xmax = xmax.max(xp);
            ymin = ymin.min(yp);
            ymax = ymax.max(yp);
        }
        let area = (xmax - xmin) * (ymax - ymin);
        let better = match best {
            None => true,
            Some((ba, ..)) => area < ba,
        };
        if better {
            best = Some((area, ux, uy, vx, vy, xmin, xmax, ymin, ymax));
        }
    }
    let (_, ux, uy, vx, vy, xmin, xmax, ymin, ymax) = best?;
    // 角点 = 中心 ± 半宽*u ± 半高*v
    let cx = (xmin + xmax) / 2.0;
    let cy = (ymin + ymax) / 2.0;
    let hw = (xmax - xmin) / 2.0;
    let hh = (ymax - ymin) / 2.0;
    let mut quad = [Pt::new(0.0, 0.0); 4];
    let signs = [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)];
    for (k, (s1, s2)) in signs.iter().enumerate() {
        quad[k] = Pt::new(cx + s1 * hw * ux + s2 * hh * vx, cy + s1 * hw * uy + s2 * hh * vy);
    }
    Some((quad, (xmax - xmin).min(ymax - ymin)))
}

// ==================== 多边形偏移（pyclipper ClipperOffset JT_ROUND 等价）====================

/// 多边形向外偏移 `dist`（圆角连接，采样步长 ~0.1rad）。
/// 凸角用绕顶点圆弧采样，凹角退化用两偏移边端点；结果仅用于二次 minAreaRect，
/// 对凸四边形（unclip 输入恒为 minAreaRect 四点）与 pyclipper 结果在凸包层面一致。
pub fn polygon_offset(poly: &[Pt], dist: f64) -> Vec<Pt> {
    if poly.len() < 3 || dist <= 0.0 {
        return poly.to_vec();
    }
    let n = poly.len();
    let cw = signed_area(poly) > 0.0; // y 向下：面积>0 = 顺时针
                                      // 外法线（归一化）：顺时针多边形外侧在边右侧（图像坐标系）
    let outer_normal = |d: (f64, f64)| -> (f64, f64) {
        let l = (d.0 * d.0 + d.1 * d.1).sqrt();
        if l < 1e-12 {
            (0.0, 0.0)
        } else if cw {
            (d.1 / l, -d.0 / l)
        } else {
            (-d.1 / l, d.0 / l)
        }
    };
    let mut out: Vec<Pt> = Vec::new();
    for i in 0..n {
        let v = poly[i];
        let prev = poly[(i + n - 1) % n];
        let next = poly[(i + 1) % n];
        let in_d = normalize(prev.x - v.x, prev.y - v.y); // 指向 v 的入方向
        let out_d = normalize(next.x - v.x, next.y - v.y); // 出方向
        let n_in = outer_normal((v.x - prev.x, v.y - prev.y));
        let n_out = outer_normal((next.x - v.x, next.y - v.y));
        // 偏移边：过 v + t*n 且沿边方向
        let p_in = Pt::new(v.x + dist * n_in.0, v.y + dist * n_in.1);
        let p_out = Pt::new(v.x + dist * n_out.0, v.y + dist * n_out.1);
        // 交点（miter）：p_in + λ*in_d == p_out + μ*out_d
        let det = in_d.0 * (-out_d.1) - in_d.1 * (-out_d.0);
        let lam = if det.abs() > 1e-12 {
            ((p_out.x - p_in.x) * (-out_d.1) - (p_out.y - p_in.y) * (-out_d.0)) / det
        } else {
            f64::NAN
        };
        if lam.is_finite() && lam > 0.0 {
            // 凸角：miter 点 + 圆弧采样（法线 n_in → n_out 扫过外角）
            let miter = Pt::new(p_in.x + lam * in_d.0, p_in.y + lam * in_d.1);
            let a0 = n_in.1.atan2(n_in.0);
            let a1 = n_out.1.atan2(n_out.0);
            // 取短弧方向（±π 内）
            let mut d_ang = a1 - a0;
            while d_ang > std::f64::consts::PI {
                d_ang -= 2.0 * std::f64::consts::PI;
            }
            while d_ang < -std::f64::consts::PI {
                d_ang += 2.0 * std::f64::consts::PI;
            }
            // 外角弧 = 包含 miter 方向的那条弧。判据：短弧中点若在多边形内，
            // 说明短弧是内角弧，需取补弧（反向旋转到 a1）。
            let mid_ang = a0 + d_ang / 2.0;
            let mid_pt = Pt::new(v.x + dist * mid_ang.cos(), v.y + dist * mid_ang.sin());
            if point_in_polygon(&mid_pt, poly) {
                d_ang = if d_ang >= 0.0 {
                    d_ang - 2.0 * std::f64::consts::PI
                } else {
                    d_ang + 2.0 * std::f64::consts::PI
                };
            }
            let steps = ((d_ang.abs() / 0.1).ceil() as usize).max(1);
            for k in 1..steps {
                let t = a0 + d_ang * (k as f64) / (steps as f64);
                out.push(Pt::new(v.x + dist * t.cos(), v.y + dist * t.sin()));
            }
            out.push(miter);
        } else {
            // 偏移边平行/退化：两端点都保留
            out.push(p_in);
            out.push(p_out);
        }
    }
    out
}

fn normalize(x: f64, y: f64) -> (f64, f64) {
    let l = (x * x + y * y).sqrt();
    if l < 1e-12 {
        (0.0, 0.0)
    } else {
        (x / l, y / l)
    }
}

/// 点在多边形内判定（even-odd 射线法）
fn point_in_polygon(p: &Pt, poly: &[Pt]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            let x_cross = a.x + t * (b.x - a.x);
            if p.x < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

// ==================== 多边形填充均值（cv2.fillPoly + cv2.mean 等价）====================

/// 用多边形填充掩码求 bitmap（f32 概率图）的掩码内均值；掩码全空 → 0.0。
/// 扫描线规则与 cv2.fillPoly 一致：整数 y 扫描线，边在 y ∈ [ceil(lo.y), floor(hi.y)]
/// 求交（水平边跳过），交点排序配对，填充 [round(x0), round(x1)] 闭区间。
pub fn polygon_mean(bitmap: &[f32], bw: usize, bh: usize, box_: &[Pt]) -> f64 {
    if bw == 0 || bh == 0 || box_.len() < 3 {
        return 0.0;
    }
    let xmin = box_.iter().map(|p| p.x.floor() as i64).fold(i64::MAX, i64::min).max(0);
    let xmax = box_
        .iter()
        .map(|p| p.x.ceil() as i64)
        .fold(i64::MIN, i64::max)
        .min(bw as i64 - 1);
    let ymin = box_.iter().map(|p| p.y.floor() as i64).fold(i64::MAX, i64::min).max(0);
    let ymax = box_
        .iter()
        .map(|p| p.y.ceil() as i64)
        .fold(i64::MIN, i64::max)
        .min(bh as i64 - 1);
    if xmin > xmax || ymin > ymax {
        return 0.0;
    }
    let mut sum = 0.0f64;
    let mut cnt = 0usize;
    for py in ymin..=ymax {
        let mut xs: Vec<f64> = Vec::with_capacity(8);
        for i in 0..box_.len() {
            let a = box_[i];
            let b = box_[(i + 1) % box_.len()];
            let (lo, hi) = if a.y < b.y { (a, b) } else { (b, a) };
            if (hi.y - lo.y).abs() < 1e-9 {
                continue; // 水平边不产生交点（cv2 ScanEdge 同）
            }
            let y0 = lo.y.ceil() as i64;
            let y1 = hi.y.floor() as i64;
            if py >= y0 && py <= y1 {
                let t = (py as f64 - lo.y) / (hi.y - lo.y);
                xs.push(lo.x + t * (hi.x - lo.x));
            }
        }
        xs.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let mut k = 0;
        while k + 1 < xs.len() {
            let x0 = xs[k].round() as i64;
            let x1 = xs[k + 1].round() as i64;
            for px in x0.max(xmin)..=x1.min(xmax) {
                sum += bitmap[(py * bw as i64 + px) as usize] as f64;
                cnt += 1;
            }
            k += 2;
        }
    }
    if cnt == 0 {
        0.0
    } else {
        sum / cnt as f64
    }
}

// ==================== 四点排序（order_points_clockwise 等价）====================

/// 四点按 x 排序再按 y 分列 → [tl, tr, br, bl]（RapidOCR `order_points_clockwise`）。
pub fn order_points_clockwise(pts: &[Pt; 4]) -> Quad {
    let mut s = *pts;
    s.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    let (left, right) = s.split_at_mut(2);
    left.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    right.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    [left[0], right[0], right[1], left[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_area_positive_is_clockwise_y_down() {
        // y 向下坐标：该点序为顺时针
        let poly = [
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        assert!(signed_area(&poly) > 0.0);
        assert!((polygon_area(&poly) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn min_area_rect_axis_aligned() {
        let pts = [
            Pt::new(1.0, 2.0),
            Pt::new(9.0, 2.0),
            Pt::new(9.0, 6.0),
            Pt::new(1.0, 6.0),
        ];
        let (quad, min_side) = min_area_rect(&pts).unwrap();
        assert!((min_side - 4.0).abs() < 1e-9);
        // 角点集合（任意序）应覆盖原矩形角点
        let mut corners: Vec<(f64, f64)> = quad.iter().map(|p| (p.x, p.y)).collect();
        corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(corners
            .iter()
            .all(|(x, y)| (x - 1.0).abs() < 1e-6 || (x - 9.0).abs() < 1e-6));
        assert!(corners
            .iter()
            .all(|(x, y)| (y - 2.0).abs() < 1e-6 || (y - 6.0).abs() < 1e-6));
    }

    #[test]
    fn min_area_rect_rotated() {
        // 45° 旋转矩形（中心 0,0，边长 ~2√2 x √2）
        let pts = [
            Pt::new(2.0, 0.0),
            Pt::new(0.0, 2.0),
            Pt::new(-2.0, 0.0),
            Pt::new(0.0, -2.0),
        ];
        let (_, min_side) = min_area_rect(&pts).unwrap();
        // 最小外接矩形边长 4 x 2.828，短边 = 2.828
        assert!((min_side - 2.0 * 2.0f64.sqrt()).abs() < 1e-6, "got {min_side}");
    }

    #[test]
    fn min_area_rect_degenerate_returns_none() {
        let pts = [Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 2.0)];
        assert!(min_area_rect(&pts).is_none());
        assert!(min_area_rect(&[]).is_none());
    }

    #[test]
    fn offset_square_grows_evenly() {
        // 10x10 正方形 offset 2 → 各边外扩 2 → 14x14（圆角近似下角点外扩）
        let poly = [
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        let out = polygon_offset(&poly, 2.0);
        let (rect, min_side) = min_area_rect(&out).unwrap();
        assert!((min_side - 14.0).abs() < 0.5, "min_side={min_side}, out={out:?}");
        let _ = rect;
        // 所有点应在原矩形外
        for p in &out {
            assert!(
                p.x < -1.9 || p.x > 11.9 || p.y < -1.9 || p.y > 11.9,
                "point inside: {p:?}"
            );
        }
    }

    #[test]
    fn offset_distance_matches_area_ratio_usage() {
        // unclip 距离 = area*ratio/perimeter：10x10, ratio 1.6 → 100*1.6/40 = 4.0
        let poly = [
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        let dist = polygon_area(&poly) * 1.6 / perimeter(&poly);
        assert!((dist - 4.0).abs() < 1e-9);
        let out = polygon_offset(&poly, dist);
        let (_, min_side) = min_area_rect(&out).unwrap();
        assert!((min_side - 18.0).abs() < 0.5, "min_side={min_side}");
    }

    #[test]
    fn polygon_mean_rect_exact() {
        // 3x3 概率图，矩形 [1,1]-[2,2]：cv2.fillPoly 填 2x2 块（整数扫描线闭区间）
        let bmp = vec![
            1.0, 2.0, 3.0, //
            4.0, 9.0, 10.0, //
            5.0, 13.0, 14.0, //
        ];
        let box_ = [
            Pt::new(1.0, 1.0),
            Pt::new(2.0, 1.0),
            Pt::new(2.0, 2.0),
            Pt::new(1.0, 2.0),
        ];
        let m = polygon_mean(&bmp, 3, 3, &box_);
        assert!((m - (9.0 + 10.0 + 13.0 + 14.0) / 4.0).abs() < 1e-9, "got {m}");
    }

    #[test]
    fn polygon_mean_outside_clipped() {
        // 矩形超出图像 → 裁剪到图像内
        let bmp = vec![0.0, 1.0, 0.0];
        let box_ = [
            Pt::new(-5.0, -5.0),
            Pt::new(10.0, -5.0),
            Pt::new(10.0, 10.0),
            Pt::new(-5.0, 10.0),
        ];
        let m = polygon_mean(&bmp, 3, 1, &box_);
        assert!((m - 1.0 / 3.0).abs() < 1e-9, "got {m}");
    }

    #[test]
    fn polygon_mean_empty_mask_zero() {
        let bmp = vec![0.0f32; 9];
        let box_ = [
            Pt::new(0.0, 0.0),
            Pt::new(1.0, 0.0),
            Pt::new(1.0, 1.0),
            Pt::new(0.0, 1.0),
        ];
        // 概率图全 0 → 均值 0
        assert_eq!(polygon_mean(&bmp, 3, 3, &box_), 0.0);
    }

    #[test]
    fn order_points_clockwise_sorts_tl_tr_br_bl() {
        // 旋转矩形四点（乱序输入）
        let pts = [
            Pt::new(9.0, 3.0),
            Pt::new(3.0, 1.0),
            Pt::new(1.0, 9.0),
            Pt::new(7.0, 11.0),
        ];
        let q = order_points_clockwise(&pts);
        // 按 x 分左右列，每列按 y 升序：左列 [tl, bl]，右列 [tr, br]
        assert_eq!(q[0], Pt::new(3.0, 1.0)); // tl
        assert_eq!(q[1], Pt::new(9.0, 3.0)); // tr
        assert_eq!(q[2], Pt::new(7.0, 11.0)); // br
        assert_eq!(q[3], Pt::new(1.0, 9.0)); // bl
    }

    #[test]
    fn find_borders_rectangle_and_hole() {
        // 6x6：外矩形 + 内部洞（RETR_LIST = 外边界 + 洞边界）
        let mut mask = vec![0u8; 36];
        for y in 1..5 {
            for x in 1..5 {
                mask[y * 6 + x] = 1;
            }
        }
        for y in 2..4 {
            for x in 2..4 {
                mask[y * 6 + x] = 0;
            }
        }
        let borders = find_border_pixel_sets(&mask, 6, 6);
        assert_eq!(borders.len(), 2, "expected outer + hole, got {}", borders.len());
        // 外边界 12 像素（4x4 方块四边，去角）
        assert_eq!(borders[0].len(), 12, "outer border pixels: {}", borders[0].len());
        // 洞边界 8 像素（2x2 洞）
        assert_eq!(borders[1].len(), 8, "hole border pixels: {}", borders[1].len());
        // 洞边界点都在洞周围
        assert!(borders[1]
            .iter()
            .all(|p| p.x >= 1.0 && p.x <= 4.0 && p.y >= 1.0 && p.y <= 4.0));
    }

    #[test]
    fn find_borders_two_components() {
        let mut mask = vec![0u8; 60]; // 10x6
        mask[1 * 10 + 1] = 1;
        mask[1 * 10 + 2] = 1;
        mask[5 * 10 + 5] = 1;
        let borders = find_border_pixel_sets(&mask, 10, 6);
        assert_eq!(borders.len(), 2);
        assert_eq!(borders[0].len(), 2);
        assert_eq!(borders[1].len(), 1);
    }

    #[test]
    fn find_borders_empty_mask() {
        assert!(find_border_pixel_sets(&[], 0, 0).is_empty());
        assert!(find_border_pixel_sets(&[0u8; 16], 4, 4).is_empty());
    }
}
