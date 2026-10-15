#!/usr/bin/env python3
"""WasmNest 探索 7：细线缠绕 W（细线沿 W 路径不断绕圈；交叉处按几何真挖缝）。

与探索 6 同为 trochoid 缠绕，但把「上下关系」从 SVG mask 改成**几何切缝**：
  1) 求交：两条 polyline 的线段相交处求交点；
  2) 沿下方 item 的弧长在交点两侧各切掉 (线宽/2 + gap)/sinθ；
  3) 下方 item 输出为多段 polyline。
好处：不依赖背景色（图标渐变底 / 透明底 / splash 都能用）、SVG 小、渲染快，
且缝宽随交叉角度自适应（斜穿时缝更宽，视觉上与上方线等宽）。

用法：python3 weave6.py > weave6.svg && rsvg-convert -z 2 weave6.svg -o weave6.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"

T1 = (12.0, 21.0)
V1 = (31.0, 75.0)
P = (50.0, 43.0)
V2 = (69.0, 75.0)
T2 = (88.0, 21.0)

def sub(a, b): return (a[0] - b[0], a[1] - b[1])
def add(a, b): return (a[0] + b[0], a[1] + b[1])
def mul(a, s): return (a[0] * s, a[1] * s)
def norm(a):
    L = math.hypot(*a) or 1.0
    return (a[0] / L, a[1] / L)
def dist(a, b): return math.hypot(a[0] - b[0], a[1] - b[1])

def fillet_path(corner_r, step=0.6):
    verts = [T1, V1, P, V2, T2]
    pts = []
    def straight(a, b):
        L = dist(a, b)
        n = max(2, int(L / step))
        for k in range(1, n + 1):
            t = k / n
            pts.append(((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t), norm(sub(b, a))))
    pts.append((verts[0], norm(sub(verts[1], verts[0]))))
    for i in range(1, len(verts)):
        v, prev = verts[i], verts[i - 1]
        d_in = norm(sub(v, prev))
        if i < len(verts) - 1:
            d_out = norm(sub(verts[i + 1], v))
            cosang = max(-1.0, min(1.0, d_in[0] * d_out[0] + d_in[1] * d_out[1]))
            acute = math.acos(cosang)
            half = (math.pi - acute) / 2.0
            if half < 1e-3:
                straight(pts[-1][0], v)
                continue
            t = corner_r / math.tan(half)
            bis = norm(sub(d_out, d_in))
            c = add(v, mul(bis, corner_r / math.sin(half)))
            pa = sub(v, mul(d_in, t))
            pb = add(v, mul(d_out, t))
            straight(pts[-1][0], pa)
            a0 = math.atan2(pa[1] - c[1], pa[0] - c[0])
            a1 = math.atan2(pb[1] - c[1], pb[0] - c[0])
            da = (a1 - a0) % (2 * math.pi)
            if da > math.pi:
                da -= 2 * math.pi
            n = max(4, int(abs(da) * corner_r / step))
            for k in range(1, n + 1):
                ang = a0 + da * k / n
                pts.append(((c[0] + corner_r * math.cos(ang), c[1] + corner_r * math.sin(ang)),
                            norm((-math.sin(ang), math.cos(ang)))))
        else:
            straight(pts[-1][0], v)
    return pts

def arclen(pts):
    """累积弧长；同时接受 [(点, 切线)] 与 [点] 两种点列。"""
    def p(x):
        return x[0] if isinstance(x[0], (tuple, list)) else x
    s, out = 0.0, [0.0]
    for i in range(1, len(pts)):
        s += dist(p(pts[i]), p(pts[i - 1]))
        out.append(s)
    return out

def coil_items(pts, b, pitch, flip=False, per_item=12):
    """沿路径做 trochoid 缠绕（细线不断绕圈）。

    x_adv = a·φ − b·sinφ 是**非单调**的（b > a 时回退成圈），因此线在路径两端必然
    会「冲出」路径——冲出部分必须丢弃，不能让路径查找钳位（钳位会在端点拉出一段
    直杠，上一轮图片里的直线残段就是它）。故采样函数同时返回推进量，越界即判无效，
    整段丢弃后由有效样本组成 items（线头停在圈的任意位置上，读作自然断头）。
    """
    sl = arclen(pts)
    L = sl[-1]
    a_nom = pitch / (2 * math.pi)
    n_half = max(4, round(L / (a_nom * math.pi)))
    if n_half % 2:
        n_half += 1                 # 收在 2π 整数倍，两端对称
    phi_end = n_half * math.pi
    a = L / phi_end
    sgn = -1.0 if flip else 1.0

    def at(phi):
        adv = a * phi - b * math.sin(phi)
        if adv < 0.0 or adv > L:
            return None
        lo, hi = 0, len(sl) - 1
        while lo < hi - 1:
            mid = (lo + hi) // 2
            if sl[mid] <= adv:
                lo = mid
            else:
                hi = mid
        seg_len = (sl[hi] - sl[lo]) or 1.0
        t = (adv - sl[lo]) / seg_len
        p0, u0 = pts[lo][0], pts[lo][1]
        p1, u1 = pts[hi][0], pts[hi][1]
        bp = (p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t)
        uu = norm((u0[0] + (u1[0] - u0[0]) * t, u0[1] + (u1[1] - u0[1]) * t))
        nn = (-uu[1], uu[0])
        off = sgn * b * math.sin(phi)      # 相位取 sin：φ=0 与 φ=φ_end 时落在中心线上
        return (bp[0] + nn[0] * off, bp[1] + nn[1] * off)

    samples = [at(phi_end * k / (n_half * per_item)) for k in range(n_half * per_item + 1)]
    per = per_item
    items = []
    for i in range(n_half):
        chunk = samples[i * per:(i + 1) * per + 1]
        chunk = [c for c in chunk if c is not None]
        if len(chunk) >= 2:
            items.append(chunk)
    return items

def seg_cross(a1, a2, b1, b2):
    d1, d2 = sub(a2, a1), sub(b2, b1)
    den = d1[0] * d2[1] - d1[1] * d2[0]
    if abs(den) < 1e-9:
        return None
    t = ((b1[0] - a1[0]) * d2[1] - (b1[1] - a1[1]) * d2[0]) / den
    u = ((b1[0] - a1[0]) * d1[1] - (b1[1] - a1[1]) * d1[0]) / den
    if 0 <= t <= 1 and 0 <= u <= 1:
        ang = abs(math.atan2(den, d1[0] * d2[0] + d1[1] * d2[1]))
        return (t, u, ang, (a1[0] + d1[0] * t, a1[1] + d1[1] * t))
    return None

def cut_item(item, cuts, line, gap, min_sin=0.42, max_half=None):
    """把下方 item 在交点处切开；cuts = [(弧长位置, 交叉角)]。

    两条线**近平行相触**时（sin 角太小）不该挖缝——真实绳股在那里是并拢而非交错；
    硬挖会把线切成虚线。故 min_sin 以下的交叉直接忽略，且缝长有上限 max_half。
    """
    sl = arclen(item)
    lim = max_half if max_half is not None else line + 2 * gap
    spans = []
    for pos, ang in cuts:
        s_ang = math.sin(ang)
        if s_ang < min_sin:
            continue
        half = min(lim, (line / 2 + gap) / s_ang)
        spans.append((pos - half, pos + half))
    spans.sort()
    merged = []
    for a, b in spans:
        if merged and a <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], b))
        else:
            merged.append((a, b))
    out, cur, k = [], [item[0]], 0
    total = sl[-1]
    def point_at(s):
        s = max(0.0, min(total, s))
        lo, hi = 0, len(sl) - 1
        while lo < hi - 1:
            mid = (lo + hi) // 2
            if sl[mid] <= s:
                lo = mid
            else:
                hi = mid
        seg = (sl[hi] - sl[lo]) or 1.0
        t = (s - sl[lo]) / seg
        return (item[lo][0] + (item[hi][0] - item[lo][0]) * t,
                item[lo][1] + (item[hi][1] - item[lo][1]) * t)
    idx, s = 1, sl[1]
    for a, b in merged:
        # 走到 a 前：收集 s <= a 的点
        while idx < len(item) and sl[idx] <= a:
            cur.append(item[idx]); idx += 1; s = sl[idx] if idx < len(sl) else total
        if len(cur) > 1:
            cur.append(point_at(a))
            out.append(cur)
        # 跳过 [a, b]
        while idx < len(item) - 1 and sl[idx] < b:
            idx += 1
        cur = []
        if idx < len(item):
            cur = [point_at(b)]
    while idx < len(item):
        cur.append(item[idx]); idx += 1
    if len(cur) > 1:
        out.append(cur)
    return out

def poly(points, w, color):
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in points)
    return (f'<polyline points="{p}" fill="none" stroke="{color}" stroke-width="{w:.2f}" '
            f'stroke-linecap="butt" stroke-linejoin="round"/>')

def bbox(points):
    xs = [p[0] for p in points]; ys = [p[1] for p in points]
    return (min(xs), min(ys), max(xs), max(ys))

def build(line=2.6, b=5.0, pitch=8.0, corner_r=8.0, gap=1.15, pattern="later",
          flip=False, ember_turns=(), min_sin=0.42, max_half=None):
    items = coil_items(fillet_path(corner_r), b, pitch, flip=flip)
    boxes = [bbox(it) for it in items]
    def over(i, j):
        return i > j if pattern == "later" else (i + j) % 2 == 0
    # 收集每条 item 需被切掉的位置
    cuts = {i: [] for i in range(len(items))}
    for i in range(len(items)):
        for j in range(len(items)):
            if i == j:
                continue
            A, B = boxes[i], boxes[j]
            if A[2] < B[0] or B[2] < A[0] or A[3] < B[1] or B[3] < A[1]:
                continue
            if over(i, j):
                continue
            sl = arclen(items[i])
            for u in range(len(items[i]) - 1):
                for v in range(len(items[j]) - 1):
                    hit = seg_cross(items[i][u], items[i][u + 1], items[j][v], items[j][v + 1])
                    if hit:
                        t, _, ang, _ = hit
                        cuts[i].append((sl[u] + (sl[u + 1] - sl[u]) * t, ang))
    out = []
    for i, it in enumerate(items):
        col = EMBER if i in ember_turns else PAPER
        pieces = cut_item(it, cuts[i], line, gap, min_sin, max_half) if cuts[i] else [it]
        out += [poly(p, line, col) for p in pieces]
    return "".join(out)

CASES = [
    ("M1 线2.2 圈5.0 距9.5 r10", build(line=2.2, b=5.0, pitch=9.5, corner_r=10, gap=0.9)),
    ("M2 线2.0 圈5.5 距11 r10", build(line=2.0, b=5.5, pitch=11.0, corner_r=10, gap=0.85)),
    ("M3 线2.4 圈5.5 距10 r10", build(line=2.4, b=5.5, pitch=10.0, corner_r=10, gap=1.0)),
    ("M4 线2.2 圈5.0 距12 r10", build(line=2.2, b=5.0, pitch=12.0, corner_r=10, gap=0.9)),
    ("M5 线2.0 圈4.6 距9.5 r9·交替", build(line=2.0, b=4.6, pitch=9.5, corner_r=9, gap=0.85, pattern="alternate")),
    ("M6 线2.2 圈5.0 距9.5 · 琥珀首圈", build(line=2.2, b=5.0, pitch=9.5, corner_r=10, gap=0.9, ember_turns={0, 1})),
]

# ---- 版式：3 列 × 2 行，每格大图 430px + 一行小尺寸 96/64/48/32/24/16 ----
PAD, HEAD = 22, 78
CELL, COLS = 470, 3
ROWS = math.ceil(len(CASES) / COLS)
WID = PAD * 2 + COLS * CELL + PAD * (COLS - 1)
H = HEAD + ROWS * (CELL + 96)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WID}" height="{H}" viewBox="0 0 {WID} {H}">',
       f'<rect width="{WID}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — thin thread wound along the W</text>',
       f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'one continuous thin line · big loops · over/under gaps cut geometrically · small sizes 96/64/48/32/24/16</text>']

for i, (label, art) in enumerate(CASES):
    r, c = divmod(i, COLS)
    x0 = PAD + c * (CELL + PAD)
    y0 = HEAD + r * (CELL + 96)
    svg.append(f'<rect x="{x0}" y="{y0}" width="{CELL}" height="{CELL}" rx="14" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0},{y0}) scale({CELL/100})">{art}</g>')
    sx = x0 + 8
    for sz in (96, 64, 48, 32, 24, 16):
        svg.append(f'<g transform="translate({sx},{y0+CELL+16}) scale({sz/100})">{art}</g>')
        sx += sz + 18
    svg.append(f'<text x="{x0+8}" y="{y0+CELL+88}" font-family="Inter" font-size="12.5" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
