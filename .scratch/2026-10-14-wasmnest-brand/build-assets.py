#!/usr/bin/env python3
"""WasmNest 品牌资产生成器（标志几何真源）。

设计意图（用户裁定的构造 + 本次收敛）：
  W 拆成**四根圆柱体棒料**；每根棒料上 3 股右手螺旋（下行）+ 3 股左手螺旋（回程），
  两族在棒料表面交织成细长菱形——这就是「一根很细的线绕下去、再反着绕回来」。
  压/被压不是拍脑袋交替，而是按**三维深度**判定：把棒料当圆柱，股线位置
  P = 轴 + R·(n·cosθ + ẑ·sinθ)，朝向观察者的深度 = R·sinθ，交叉处 sinθ 小的一股在后、
  被压断（几何挖缝，不依赖背景色）。因此每一股的压/被压随绕行自动交替 = 真编织。
  端头**缠扎**（最后 2 圈收紧节距、降噪、幅度收向中轴）= 扎紧的绳头，不是炸开的刷子。

三档降级（同一份骨架）：
  ≥32px  编织态：细股的菱形镂空可见
  24px   编织态临界：并成实心纹理
  ≤24px  实心态：四根实心棒料 + 斜接缝（icon-simple / favicon 用这一档）

真源：MASTER 常量 + VERTS 骨架。改标志只改这里，重跑本脚本。
用法：
    python3 build-assets.py             # 生成全部 SVG（+ ico/icns 骨架位图需先 --png）
    python3 build-assets.py --png       # 同时导出 PNG 阶梯 + .ico + .icns（需 rsvg-convert）
    python3 build-assets.py --wordmark  # 重新转曲字标（rsvg-convert + ghostscript + pdftocairo）
"""
import argparse
import json
import math
import re
import struct
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ASSETS = HERE / "assets"
FONTCONFIG = HERE / "fonts.conf"


# ============================================================================
# 1. 色板（沿用 v1 色阶：品牌色系不随改名变动，保证连续性）
# ============================================================================
INK = "#08090B"          # 主深底
INK_2 = "#16181C"        # 渐变上端 / 抬升面
SURFACE = "#121418"      # 卡片面
BORDER = "#242930"       # 分隔线
EMBER = "#FF9E2C"        # 品牌琥珀 —— 活跃 / 品牌主色
EMBER_700 = "#9F621B"    # 浅色底上的琥珀（保证对比）
EMBER_800 = "#764914"
PAPER = "#F5F7F9"        # 主前景 / 标志线
FOG = "#9AA3AE"          # 次级文字
CANVAS = "#F4F6F8"       # 浅色底


# ============================================================================
# 2. 几何真源
# ============================================================================
# W 骨架顶点（四根棒料的中心线端点）。中峰 47（v1 是 43）：内侧两根棒料夹角更大，
# 两团绕线不再在中脊挤成一坨——这是编织态最要紧的一处骨架调整。
VERTS = [(12.0, 20.0), (28.5, 74.0), (50.0, 47.0), (71.5, 74.0), (88.0, 20.0)]

MASTER = dict(
    R=7.8,          # 外侧棒料半径（棒宽 = 2R）
    R2=7.0,         # 内侧棒料半径（中脊面积小，细一点更透气）
    seam=2.2,       # 斜接缝宽度：棒料之间的等宽细缝 = 隔离边界
    thread=2.0,     # 线宽（细线）
    pitch=26.0,     # 节距：成品周长 ≈ 2πR ≈ 49，节距 26 → 浅角，股线沿棒料走（藤条感）
    gap=1.6,        # 交叉挖缝：被压股两侧的缝
    chaos=0.30,     # 错乱度（节距抖动 + 相位抖动 + 低频摆动；0 = 机械绕线）
    n_strands=3,    # 每个方向的股数（总数 = 2×n）
    whip=2.0,       # 端头缠扎圈数
    whip_tight=0.5, # 缠扎圈的节距倍率（<1 = 更密）
    end_chaos=0.25, # 端头错乱度衰减（扎紧的绳头是整齐的）
    end_amp=0.55,   # 端头幅度收束（避免甩出 V 形钩）
    drift=0.35,     # 节距沿轴线单调漂移（手缠的节距不是常数）
    spread=0.17,    # 反向族相位错开（避免正反两族对称成网格）
    ember_rod=1,    # 1-based：染琥珀的那根棒料 = 正在运行的那一个实例
)


def sub(a, b): return (a[0] - b[0], a[1] - b[1])
def add(a, b): return (a[0] + b[0], a[1] + b[1])
def mul(a, s): return (a[0] * s, a[1] * s)
def dot(a, b): return a[0] * b[0] + a[1] * b[1]


def norm(a):
    length = math.hypot(*a) or 1.0
    return (a[0] / length, a[1] / length)


def perp(a): return (-a[1], a[0])
def dist(a, b): return math.hypot(a[0] - b[0], a[1] - b[1])


def line_inter(p, u, q, v):
    """直线 p+t·u 与 q+s·v 的交点；平行返回 None。"""
    den = u[0] * v[1] - u[1] * v[0]
    if abs(den) < 1e-9:
        return None
    t = ((q[0] - p[0]) * v[1] - (q[1] - p[1]) * v[0]) / den
    return (p[0] + u[0] * t, p[1] + u[1] * t)


def shift_cut(cut, away, d):
    """把斜接切面沿其法线朝远离 away 的一侧平移 d（得到等宽接缝）。

    「沿轴线缩短」会让缝宽从内侧 0 渐变到外侧 2·seam·sin(θ/2)，接点长成箭头缺口；
    沿公共法线平移才是等宽缝。
    """
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    if dot(sub(away, p1), nn) > 0:
        nn = mul(nn, -1.0)
    return (add(p1, mul(nn, d)), add(p2, mul(nn, d)))


def rods(R, R2, seam, verts=VERTS):
    """W → 四根棒料（中心线、方向、法线、两端斜接切面、半径）。"""
    out = []
    for i in range(4):
        A, B = verts[i], verts[i + 1]
        u = norm(sub(B, A))
        n = perp(u)
        L = dist(A, B)
        mid = add(A, mul(u, L * 0.5))
        if i > 0:
            u_prev = norm(sub(A, verts[i - 1]))
            np_ = perp(u_prev)
            p1 = line_inter(add(A, mul(np_, R)), u_prev, add(A, mul(n, R)), u)
            p2 = line_inter(add(A, mul(np_, -R)), u_prev, add(A, mul(n, -R)), u)
            p1, p2 = shift_cut((p1, p2), mid, seam / 2)
        else:
            p1, p2 = A, add(A, mul(n, 1.0))       # 自由端：垂直于轴线的平切
        if i < 3:
            u_next = norm(sub(verts[i + 2], B))
            nn = perp(u_next)
            q1 = line_inter(add(B, mul(n, R)), u, add(B, mul(nn, R)), u_next)
            q2 = line_inter(add(B, mul(n, -R)), u, add(B, mul(nn, -R)), u_next)
            q1, q2 = shift_cut((q1, q2), mid, seam / 2)
        else:
            q1, q2 = B, add(B, mul(n, 1.0))
        out.append(dict(i=i, A=A, B=B, u=u, n=n, L=L, mid=mid,
                        cut0=(p1, p2), cut1=(q1, q2),
                        r=(R if i in (0, 3) else R2)))
    return out


def inside(cut, p, ref):
    """点 p 是否在切面 cut 的保留侧（用 ref 定参考侧，不猜符号）。"""
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    s_ref = dot(sub(ref, p1), nn)
    return dot(sub(p, p1), nn) * s_ref >= 0.0


class Rng:
    """xorshift32：标志资产必须可复现（每次渲染完全一致），不能用系统随机。"""

    def __init__(self, seed):
        self.s = (seed & 0xFFFFFFFF) or 0x9E3779B9

    def nxt(self):
        x = self.s
        x ^= (x << 13) & 0xFFFFFFFF
        x ^= x >> 17
        x ^= (x << 5) & 0xFFFFFFFF
        self.s = x & 0xFFFFFFFF
        return self.s / 0xFFFFFFFF

    def rng(self, a, b):
        return a + (b - a) * self.nxt()


# ============================================================================
# 3. 股线 / 交织
# ============================================================================
def strand(rod, line, pitch, dirn, phase, seed, chaos, whip, whip_tight,
           end_chaos, end_amp, drift, samples_per_turn=52):
    """一股缠绕线：返回 (点列, 该点处的 θ 列)。θ 用于算前后深度。"""
    A, u, n, L, R = rod["A"], rod["u"], rod["n"], rod["L"], rod["r"]
    rng = Rng(seed * 7919 + rod["i"] * 104729 + int(phase * 1000) + int(pitch * 31))
    n_turn = max(1, int(round(L / pitch)))
    k_whip = max(1, min(int(round(whip)), max(1, n_turn // 3)))
    steps = []
    for k in range(n_turn + 2):
        w = whip_tight if (k < k_whip or k >= n_turn + 2 - k_whip) else 1.0
        d = 1.0 + drift * ((k + 0.5) / (n_turn + 2) - 0.5)
        steps.append(w * d * (1.0 + chaos * rng.rng(-0.24, 0.38)))
    tot = sum(steps)
    marks = [0.0]
    acc = 0.0
    for st in steps:
        acc += st
        marks.append(acc / tot)
    amp = [1.0 + chaos * rng.rng(-0.06, 0.06) for _ in range(n_turn + 3)]
    phj = [chaos * rng.rng(-0.22, 0.22) for _ in range(n_turn + 3)]
    wob = [chaos * rng.rng(-0.14, 0.14) for _ in range(n_turn + 3)]
    wph = [chaos * rng.rng(0, 2 * math.pi) for _ in range(n_turn + 3)]
    Reff = R - line / 2.0
    total = max(48, int(n_turn * samples_per_turn))

    def seg_of(s):
        k = 0
        while k < n_turn + 1 and marks[k + 1] < s:
            k += 1
        k = min(k, n_turn)
        span = (marks[k + 1] - marks[k]) or 1.0
        return k, (s - marks[k]) / span

    def tidy(s):
        return end_chaos + (1.0 - end_chaos) * min(1.0, s * n_turn / 1.4, (1.0 - s) * n_turn / 1.4)

    def theta_at(s):
        k, t = seg_of(s)
        w_ = tidy(s) * ((wob[k] * (1 - t) + wob[k + 1] * t)
                        + (phj[k] * (1 - t) + phj[k + 1] * t)
                        + 0.05 * chaos * math.sin(2 * math.pi * s * 0.9 + wph[k]))
        return 2 * math.pi * dirn * (k + t) + phase + w_

    pts, ths = [], []
    for k in range(total + 1):
        s = k / total
        kk, tt = seg_of(s)
        ampv = Reff * (amp[kk] * (1 - tt) + amp[kk + 1] * tt)
        taper = min(1.0, end_amp + (1.0 - end_amp)
                    * min(1.0, s * n_turn / 0.8, (1.0 - s) * n_turn / 0.8))
        th = theta_at(s)
        off = ampv * math.cos(th) * taper
        pts.append((A[0] + u[0] * L * s + n[0] * off, A[1] + u[1] * L * s + n[1] * off))
        ths.append(th)
    return pts, ths


def split_pair(pts, ths, keep):
    """按 keep(点) 同时切分点列与 θ 列（保证两者下标一一对应）。"""
    rp, rt, cp, ct = [], [], [], []
    for p, t in zip(pts, ths):
        if keep(p):
            cp.append(p)
            ct.append(t)
        else:
            if len(cp) > 1:
                rp.append(cp)
                rt.append(ct)
            cp, ct = [], []
    if len(cp) > 1:
        rp.append(cp)
        rt.append(ct)
    return rp, rt


def arclen(pts):
    s, out = 0.0, [0.0]
    for i in range(1, len(pts)):
        s += dist(pts[i], pts[i - 1])
        out.append(s)
    return out


def seg_cross(a1, a2, b1, b2):
    d1, d2 = sub(a2, a1), sub(b2, b1)
    den = d1[0] * d2[1] - d1[1] * d2[0]
    if abs(den) < 1e-9:
        return None
    t = ((b1[0] - a1[0]) * d2[1] - (b1[1] - a1[1]) * d2[0]) / den
    ss = ((b1[0] - a1[0]) * d1[1] - (b1[1] - a1[1]) * d1[0]) / den
    if 0 <= t <= 1 and 0 <= ss <= 1:
        return (t, ss, abs(math.atan2(den, d1[0] * d2[0] + d1[1] * d2[1])))
    return None


def interp_theta(pts, ths, arc):
    sl = arclen(pts)
    for i in range(len(sl) - 1):
        if sl[i] <= arc <= sl[i + 1]:
            span = (sl[i + 1] - sl[i]) or 1.0
            t = (arc - sl[i]) / span
            return ths[i] + (ths[i + 1] - ths[i]) * t
    return ths[-1]


def cut_spans(runs, cuts, line, gap, min_sin=0.22):
    """在 run 上按 (run 号, 弧长, 交叉角) 挖缝；返回 [[run...], ...]。"""
    out = []
    for ri, pts in enumerate(runs):
        sl = arclen(pts)
        total = sl[-1]
        spans = []
        for (r, pos, ang) in cuts:
            if r != ri or math.sin(ang) < min_sin:
                continue
            half = min(line + 2 * gap, (line / 2 + gap) / math.sin(ang))
            spans.append((max(0.0, pos - half), min(total, pos + half)))
        spans.sort()
        merged = []
        for a, b in spans:
            if merged and a <= merged[-1][1]:
                merged[-1] = (merged[-1][0], max(merged[-1][1], b))
            else:
                merged.append((a, b))

        def at(s):
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
            return (pts[lo][0] + (pts[hi][0] - pts[lo][0]) * t,
                    pts[lo][1] + (pts[hi][1] - pts[lo][1]) * t)

        idx, cur = 0, [pts[0]]
        for a, b in merged:
            while idx < len(pts) and sl[idx] <= a:
                if idx:
                    cur.append(pts[idx])
                idx += 1
            if len(cur) > 1:
                cur.append(at(a))
                out.append(cur)
            while idx < len(pts) - 1 and sl[idx] < b:
                idx += 1
            cur = [at(b)]
        while idx < len(pts):
            cur.append(pts[idx])
            idx += 1
        if len(cur) > 1:
            out.append(cur)
    return out


def weave_cuts(strands, min_sin=0.25, min_depth=0.15):
    """strands = [(sid, runs, run_thetas)]；返回 {(sid, run_idx): [(run_idx, arc, ang)]}。

    交叉处谁在后（sinθ 小）谁被挖缝；近相切（夹角或深度差过小）跳过——
    相切处挖缝会在织面上开大口子。
    """
    cuts = {}
    for a in range(len(strands)):
        for b in range(a + 1, len(strands)):
            sid_a, ra, ta = strands[a]
            sid_b, rb, tb = strands[b]
            for ia, pa in enumerate(ra):
                sa = arclen(pa)
                for ib, pb in enumerate(rb):
                    sb = arclen(pb)
                    for i in range(len(pa) - 1):
                        for j in range(len(pb) - 1):
                            hit = seg_cross(pa[i], pa[i + 1], pb[j], pb[j + 1])
                            if not hit:
                                continue
                            t, ss, ang = hit
                            if math.sin(ang) < min_sin:
                                continue
                            arc_a = sa[i] + (sa[i + 1] - sa[i]) * t
                            arc_b = sb[j] + (sb[j + 1] - sb[j]) * ss
                            da = math.sin(interp_theta(pa, ta[ia], arc_a))
                            db = math.sin(interp_theta(pb, tb[ib], arc_b))
                            if abs(da - db) < min_depth:
                                continue
                            if da < db:
                                cuts.setdefault((sid_a, ia), []).append((ia, arc_a, ang))
                            else:
                                cuts.setdefault((sid_b, ib), []).append((ib, arc_b, ang))
    return cuts


def poly(pts, w, color):
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    return (f'<polyline points="{p}" fill="none" stroke="{color}" stroke-width="{w:.2f}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>')


def rod_solid(rod, color):
    """实心态的一根棒料：两端按斜接切面截断的多边形。"""
    R = rod["r"]
    def endpt(base, sgn, cut):
        p = line_inter(add(base, mul(rod["n"], sgn * R)), rod["u"], cut[0], sub(cut[1], cut[0]))
        return p or add(base, mul(rod["n"], sgn * R))
    pts = [endpt(rod["A"], 1, rod["cut0"]), endpt(rod["B"], 1, rod["cut1"]),
           endpt(rod["B"], -1, rod["cut1"]), endpt(rod["A"], -1, rod["cut0"])]
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    return f'<polygon points="{p}" fill="{color}"/>'


def mark_svg(color=PAPER, ember=EMBER, solid=False, ember_rod=MASTER["ember_rod"],
             verts=VERTS, **over):
    """W 标志本体（100 网格坐标系，不含 viewBox）。

    编织态与实心态共用同一套骨架与半径，因此两档的轮廓完全相同——
    小尺寸换档不会让标志「变形」，只是纹理并实。
    """
    kv = {**MASTER, **over}
    parts = []
    for rod in rods(kv["R"], kv["R2"], kv["seam"], verts):
        col = ember if (ember_rod == rod["i"] + 1) else color
        if solid:
            parts.append(rod_solid(rod, col))
            continue
        keep = (lambda p, rod=rod: (rod["i"] == 0 or inside(rod["cut0"], p, rod["mid"]))
                and (rod["i"] == 3 or inside(rod["cut1"], p, rod["mid"])))
        strands, sid = [], 0
        for k in range(kv["n_strands"]):
            pt, th = strand(rod, kv["thread"], kv["pitch"] * (1 + 0.10 * math.sin(k * 2.4)),
                            1.0, 2 * math.pi * k / kv["n_strands"], 3 + k, kv["chaos"],
                            kv["whip"], kv["whip_tight"], kv["end_chaos"], kv["end_amp"],
                            kv["drift"])
            rp, rt = split_pair(pt, th, keep)
            strands.append((sid, rp, rt))
            sid += 1
        for k in range(kv["n_strands"]):
            pt, th = strand(rod, kv["thread"], kv["pitch"] * (1 + 0.10 * math.sin(k * 1.7 + 1.1)),
                            -1.0, 2 * math.pi * k / kv["n_strands"] + kv["spread"], 11 + k,
                            kv["chaos"], kv["whip"], kv["whip_tight"], kv["end_chaos"],
                            kv["end_amp"], -kv["drift"])
            rp, rt = split_pair(pt, th, keep)
            strands.append((sid, rp, rt))
            sid += 1
        cuts = weave_cuts(strands)
        for s_id, runs, _ in strands:
            for ri, run in enumerate(runs):
                cs = cuts.get((s_id, ri), [])
                for piece in cut_spans([run], cs, kv["thread"], kv["gap"]):
                    parts.append(poly(piece, kv["thread"], col))
    return "".join(parts)


def mark_bbox(solid=False, **over):
    """标志的紧包围盒（棒料外包络 + 线宽余量 + 一点呼吸）。"""
    kv = {**MASTER, **over}
    xs, ys = [], []
    for rod in rods(kv["R"], kv["R2"], kv["seam"]):
        for base, sgn in ((rod["A"], 1), (rod["A"], -1), (rod["B"], 1), (rod["B"], -1)):
            p = add(base, mul(rod["n"], sgn * rod["r"]))
            xs.append(p[0])
            ys.append(p[1])
    pad = kv["thread"] / 2.0 + 0.4
    return (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)


def mark_group(color=PAPER, ember=EMBER, solid=False, tag=None, **over):
    inner = mark_svg(color, ember, solid, **over)
    attr = f' id="{tag}"' if tag else ""
    return f'<g{attr}>{inner}</g>'


# ============================================================================
# 4. 容器：图标 / 锁定组合 / 图案
# ============================================================================
ICON_RX = 22.5      # 圆角（占边长 %）：接近 Apple 超椭圆
MARK_FILL = 58.0    # 标志在图标里的宽度占比（%）

GRAD = (f'<defs><linearGradient id="ibg" x1="0%" y1="0%" x2="100%" y2="100%">'
        f'<stop offset="0%" stop-color="{INK_2}"/><stop offset="100%" stop-color="{INK}"/>'
        f'</linearGradient></defs>'
        f'<rect width="100" height="100" rx="{ICON_RX}" fill="url(#ibg)"/>')


def fit_mark(inner_size, cx=50.0, cy=50.0, solid=False, **over):
    """把标志缩放居中到 (cx, cy)、宽度为 inner_size 的位置，返回 <g transform> 里的内容。"""
    x0, y0, x1, y1 = mark_bbox(solid, **over)
    w, h = x1 - x0, y1 - y0
    s = inner_size / max(w, h)
    tx, ty = cx - (x0 + w / 2) * s, cy - (y0 + h / 2) * s
    return f'<g transform="translate({tx:.3f},{ty:.3f}) scale({s:.5f})">'


def icon_svg(bg, color, ember, solid=False, fill=MARK_FILL, **over):
    """图标：100 网格，bg 为底色片段（已含圆角），标志居中。"""
    return (f'{bg}{fit_mark(fill, 50, 50, solid, **over)}'
            f'{mark_svg(color, ember, solid, **over)}</g>')


def pattern_svg(tile=120.0, band=9.0, color="#1D2027", accent="#23272F", bg=None, step=30.0):
    """可平铺的平纹编织底纹（板式/印刷用）：两族 ±45° 斜带，按奇偶交叉压/被压。

    与标志同一套「压/被压」语言：交叉处按 (i+j) 奇偶决定谁在上——
    这是平面平纹的正确做法（圆柱上才需要按 sinθ 判深度）。
    """
    n = int(round(tile / step)) + 2
    parts = []
    if bg:
        parts.append(f'<rect width="{tile}" height="{tile}" fill="{bg}"/>')
    parts.append(f'<defs><clipPath id="tile"><rect width="{tile}" height="{tile}"/></clipPath></defs>')

    def bands(diag, offset, col):
        segs = []
        for i in range(-n, n + 1):
            if diag > 0:
                x0, y0 = i * step + offset, 0.0
                x1, y1 = x0 + tile, tile
            else:
                x0, y0 = i * step + offset, 0.0
                x1, y1 = x0 - tile, tile
            segs.append(f'<line x1="{x0:.2f}" y1="{y0:.2f}" x2="{x1:.2f}" y2="{y1:.2f}"/>')
        return f'<g clip-path="url(#tile)" stroke="{col}" stroke-width="{band}" ' \
               f'stroke-linecap="butt">{"".join(segs)}</g>'
    # 底族先画、面族后画（面族叠在上面）：平纹底纹按族别区分即得编织感，
    # 不需要像标志那样逐交叉挖缝——底纹是肌理，不是图形。
    parts.append(bands(+1, 0.0, color))
    parts.append(bands(-1, tile / 2, accent))
    return "".join(parts)


# ============================================================================
# 5. 字标：真源 SVG → 转曲（rsvg-convert + ghostscript + pdftocairo）
# ============================================================================
WORDMARK_TEXT = "WasmNest"
WORDMARK_FONT = "Inter Display"
WORDMARK_WEIGHT = 700
WORDMARK_SIZE = 104.0        # 转曲前的字号（px）
WORDMARK_TRACK = -0.03       # 字距（em），负值收紧


def _pct_to_hex(c):
    m = re.match(r"rgb\(\s*([\d.]+)%\s*,\s*([\d.]+)%\s*,\s*([\d.]+)%\s*\)", c)
    if m:
        return "#" + "".join(f"{int(round(float(g) / 100 * 255)):02X}" for g in m.groups())
    m = re.match(r"rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)", c)
    if m:
        return "#" + "".join(f"{int(g):02X}" for g in m.groups())
    return c


def build_wordmark(force=False):
    """把字标转曲成路径，写入 assets/wasmnest-wordmark.svg（幂等，可缓存）。"""
    out = ASSETS / "wasmnest-wordmark.svg"
    if out.exists() and not force:
        return out
    for exe in ("rsvg-convert", "gs", "pdftocairo"):
        if not _which(exe):
            raise SystemExit(f"缺少 {exe}（转曲链路：rsvg-convert → ghostscript → pdftocairo）")
    ASSETS.mkdir(exist_ok=True)
    pads = 20.0
    text_w = WORDMARK_SIZE * 5.6
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{text_w:.0f}" height="160" '
           f'viewBox="0 0 {text_w:.0f} 160">'
           f'<text x="{pads}" y="118" font-family="{WORDMARK_FONT}" font-size="{WORDMARK_SIZE}" '
           f'font-weight="{WORDMARK_WEIGHT}" letter-spacing="{WORDMARK_TRACK * WORDMARK_SIZE:.2f}" '
           f'fill="{PAPER}">Wasm<tspan fill="{EMBER}">Nest</tspan></text></svg>')
    tmp = HERE / ".wordmark-tmp"
    tmp.mkdir(exist_ok=True)
    (tmp / "src.svg").write_text(svg)
    env = {"FONTCONFIG_FILE": str(FONTCONFIG), **__import__("os").environ}
    subprocess.run(["rsvg-convert", "--format=pdf", str(tmp / "src.svg"), "-o", str(tmp / "src.pdf")],
                   check=True, env=env)
    subprocess.run(["gs", "-q", "-sDEVICE=pdfwrite", "-dNoOutputFonts", "-o", str(tmp / "flat.pdf"),
                    str(tmp / "src.pdf")], check=True, env=env)
    subprocess.run(["pdftocairo", "-svg", str(tmp / "flat.pdf"), str(tmp / "flat.svg")],
                   check=True, env=env)
    raw = (tmp / "flat.svg").read_text()
    vb = re.search(r'viewBox="([^"]+)"', raw)
    paths = re.findall(r"<path[^>]*/>", raw)
    paper, ember = [], []
    for p in paths:
        fill = re.search(r'fill="([^"]+)"', p)
        col = _pct_to_hex(fill.group(1)) if fill else PAPER
        # 剥掉路径自带的 fill：颜色由引用方决定（浅底锁定组合要能整体改色）
        # 否则「Wasm」的 paper 白在浅底上会直接消失。
        clean = re.sub(r'\s*fill="[^"]*"', "", p)
        (ember if col.upper().startswith("#FF9E") or col.upper() == EMBER else paper).append(clean)
    body = (f'<g id="wordmark-ink">{"".join(paper)}</g>'
            f'<g id="wordmark-ember">{"".join(ember)}</g>')
    (out).write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb.group(1)}" '
                     f'width="{vb.group(1).split()[2]}" height="{vb.group(1).split()[3]}">{body}</svg>')
    (ASSETS / "wasmnest-wordmark.json").write_text(json.dumps(
        {"viewBox": vb.group(1), "ink": "".join(paper), "ember": "".join(ember),
         "font": WORDMARK_FONT, "weight": WORDMARK_WEIGHT, "size": WORDMARK_SIZE,
         "tracking_em": WORDMARK_TRACK}, indent=1))
    print(f"✓ 字标转曲 {out.name}（{len(paper)} 路径 = Wasm，{len(ember)} 路径 = Nest）")
    return out


def wordmark_groups():
    """读转曲缓存，返回 (ink 路径组, ember 路径组, viewBox)。"""
    cache = ASSETS / "wasmnest-wordmark.json"
    if not cache.exists():
        build_wordmark()
    d = json.loads(cache.read_text())
    return d["ink"], d["ember"], d["viewBox"]


def lockup_svg(layout="horizontal", target=60.0, color=PAPER,
               ember=EMBER, bg=None, solid=False):
    """锁定组合：标识 + 字标。horizontal = 并排；compact = 上下叠放。

    比例不是拍脑袋：字标 cap 高 = 0.63 × 转曲 viewBox 高（Inter 的 cap ≈ 0.727 em，
    转曲后 viewBox 高 ≈ 1.154 em），标识高取 1.45 × cap。字标的缩放由这个约束
    解析求出（下面的 s_w），不靠试参数。
    """
    ink, emb, vb = wordmark_groups()
    wx = [float(v) for v in vb.split()]
    ink_g = f'<g fill="{color}">{ink}</g>'
    emb_g = f'<g fill="{ember}">{emb}</g>'
    x0, y0, x1, y1 = mark_bbox(solid)
    mw, mh = x1 - x0, y1 - y0
    word_g = (f'<g transform="translate({-wx[0]:.2f},{-wx[1]:.2f})">{ink_g}{emb_g}</g>')
    CAP = 0.63        # cap 高 / viewBox 高
    MARK_H = 1.45     # 标识高 / cap 高
    if layout == "horizontal":
        gap_ratio = 0.50      # 标距 / 标识高
        denom = wx[2] + mw * CAP * MARK_H * wx[3] / mh + CAP * MARK_H * gap_ratio * wx[3]
        s_w = target / denom
        mark_h = MARK_H * CAP * wx[3] * s_w
        s_m = mark_h / mh
        mark_w = mw * s_m
        gap = gap_ratio * mark_h
        word_h = wx[3] * s_w
        h = max(mark_h, word_h)
        body = (f'<g transform="translate(0,{(h - mark_h) / 2:.2f}) scale({s_m:.5f}) '
                f'translate({-x0:.2f},{-y0:.2f})">{mark_svg(color, ember, solid)}</g>'
                f'<g transform="translate({mark_w + gap:.2f},{(h - word_h) / 2:.2f}) '
                f'scale({s_w:.5f})">{word_g}</g>')
        return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {target:.2f} {h:.2f}" '
                f'width="{target:.0f}" height="{h:.0f}">{bg or ""}{body}</svg>')
    # compact：标识在上、字标在下
    s_w = (target * 0.78) / wx[2]
    word_h = wx[3] * s_w
    mark_h = target * 0.50
    s_m = mark_h / mh
    gap_v = target * 0.07
    total_h = mark_h + gap_v + word_h
    body = (f'<g transform="translate({(target - mw * s_m) / 2:.2f},0) scale({s_m:.5f}) '
            f'translate({-x0:.2f},{-y0:.2f})">{mark_svg(color, ember, solid)}</g>'
            f'<g transform="translate({(target - wx[2] * s_w) / 2:.2f},{mark_h + gap_v:.2f}) '
            f'scale({s_w:.5f})">{word_g}</g>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {target:.2f} {total_h:.2f}" '
            f'width="{target:.0f}" height="{total_h:.0f}">{bg or ""}{body}</svg>')


def _which(exe):
    from shutil import which
    return which(exe)


# ============================================================================
# 6. 打包：SVG 写出 + PNG 阶梯 + ico/icns（纯 Python，无外部工具）
# ============================================================================
def wrap(content, viewbox, w, h, extra=""):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{viewbox}" width="{w}" '
            f'height="{h}"{extra}>{content}</svg>')


def write_svg(name, content):
    ASSETS.mkdir(exist_ok=True)
    (ASSETS / name).write_text(content + "\n")
    return ASSETS / name


def build_ico(png_paths, out):
    """纯 Python 打包 ICO：ICONDIR + 若干 PNG 载荷（Vista+ 支持 PNG 内嵌）。"""
    imgs = []
    for p in png_paths:
        data = Path(p).read_bytes()
        w, h = struct.unpack(">II", data[16:24])
        imgs.append((w, h, data))
    hdr = struct.pack("<HHH", 0, 1, len(imgs))
    offset = 6 + 16 * len(imgs)
    entries, blobs = b"", b""
    for w, h, data in imgs:
        entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        blobs += data
    out.write_bytes(hdr + entries + blobs)
    return out


def build_icns(png_paths, out):
    """纯 Python 打包 ICNS。

    类型码与像素尺寸必须严格对应，否则 macOS 读到的图标会串档：
      icp4=16 · icp5=32 · icp6=64 · ic07=128 · ic08=256 · ic09=512 · ic10=1024
      ic11/ic12/ic13/ic14 = 32/64/256/512 的 @2x 档（同一张位图，供 Retina 取用）
    载荷用 PNG（ICNS 允许 PNG，Vista 之后的 ICO 也允许）。
    """
    by_size = {16: [b"icp4"], 32: [b"icp5", b"ic11"], 64: [b"icp6", b"ic12"],
               128: [b"ic07"], 256: [b"ic08", b"ic13"], 512: [b"ic09", b"ic14"],
               1024: [b"ic10"]}
    chunks = b""
    for p in png_paths:
        data = Path(p).read_bytes()
        w = struct.unpack(">II", data[16:24])[0]
        if w not in by_size:
            raise SystemExit(f"ICNS：没有 {w}px 对应的类型码")
        for t in by_size[w]:
            chunks += t + struct.pack(">I", len(data) + 8) + data
    out.write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--png", action="store_true", help="导出 PNG 阶梯 + ico/icns")
    ap.add_argument("--wordmark", action="store_true", help="强制重新转曲字标")
    args = ap.parse_args()
    ASSETS.mkdir(exist_ok=True)

    # --- 字标 ---
    build_wordmark(force=args.wordmark)

    x0, y0, x1, y1 = mark_bbox()
    mw, mh = x1 - x0, y1 - y0
    bbox = f"{x0:.2f} {y0:.2f} {mw:.2f} {mh:.2f}"
    xs0, ys0, xs1, ys1 = mark_bbox(solid=True)
    sbbox = f"{xs0:.2f} {ys0:.2f} {xs1 - xs0:.2f} {ys1 - ys0:.2f}"

    # --- 裸标志 ---
    write_svg("wasmnest-mark.svg", wrap(mark_svg(PAPER, EMBER), bbox, 512, round(512 * mh / mw)))
    write_svg("wasmnest-mark-mono.svg", wrap(mark_svg(PAPER, PAPER), bbox, 512, round(512 * mh / mw)))
    write_svg("wasmnest-mark-ember.svg", wrap(mark_svg(EMBER, EMBER), bbox, 512, round(512 * mh / mw)))
    write_svg("wasmnest-mark-ink.svg", wrap(mark_svg(INK, EMBER_700), bbox, 512, round(512 * mh / mw)))
    write_svg("wasmnest-mark-solid.svg", wrap(mark_svg(PAPER, EMBER, solid=True), sbbox, 512,
                                              round(512 * (ys1 - ys0) / (xs1 - xs0))))
    write_svg("wasmnest-mark-solid-ink.svg", wrap(mark_svg(INK, EMBER, solid=True), sbbox, 512,
                                                  round(512 * (ys1 - ys0) / (xs1 - xs0))))

    # --- 应用图标 ---
    plain = f'<rect width="100" height="100" rx="{ICON_RX}" fill="{INK}"/>'
    light_bg = f'<rect width="100" height="100" rx="{ICON_RX}" fill="{CANVAS}"/>'
    ember_bg = f'<rect width="100" height="100" rx="{ICON_RX}" fill="{EMBER}"/>'
    write_svg("wasmnest-icon.svg", wrap(icon_svg(GRAD, PAPER, EMBER), "0 0 100 100", 1024, 1024))
    write_svg("wasmnest-icon-simple.svg",
              wrap(icon_svg(GRAD, PAPER, EMBER, solid=True, fill=MARK_FILL + 6), "0 0 100 100", 1024, 1024))
    write_svg("wasmnest-icon-light.svg",
              wrap(icon_svg(light_bg, INK, EMBER, fill=MARK_FILL - 2), "0 0 100 100", 1024, 1024))
    write_svg("wasmnest-icon-mono.svg",
              wrap(icon_svg(ember_bg, INK, INK, solid=True, fill=MARK_FILL + 2), "0 0 100 100", 1024, 1024))
    write_svg("wasmnest-icon-ios.svg", wrap(icon_svg(GRAD.replace(f' rx="{ICON_RX}"', ""), PAPER, EMBER),
                                            "0 0 100 100", 1024, 1024))
    write_svg("wasmnest-favicon.svg",
              wrap(icon_svg(GRAD, PAPER, EMBER, solid=True, fill=MARK_FILL + 10), "0 0 100 100", 64, 64))
    # Android 自适应图标：108dp 画布，安全区 = 中心 66dp
    ad_bg = (f'<defs><linearGradient id="abg" x1="0%" y1="0%" x2="100%" y2="100%">'
             f'<stop offset="0%" stop-color="{INK_2}"/><stop offset="100%" stop-color="{INK}"/>'
             f'</linearGradient></defs><rect width="108" height="108" fill="url(#abg)"/>')
    write_svg("wasmnest-android-background.svg", wrap(ad_bg, "0 0 108 108", 432, 432))
    write_svg("wasmnest-android-foreground.svg",
              wrap(f'{fit_mark(58.0, 54, 54)}{mark_svg(PAPER, EMBER)}</g>', "0 0 108 108", 432, 432))
    write_svg("wasmnest-android-monochrome.svg",
              wrap(f'{fit_mark(58.0, 54, 54, solid=True)}{mark_svg("#000000", "#000000", solid=True)}</g>',
                   "0 0 108 108", 432, 432))

    # --- 锁定组合 ---
    write_svg("wasmnest-lockup.svg", lockup_svg("horizontal", 60.0))
    write_svg("wasmnest-lockup-ink.svg", lockup_svg("horizontal", 60.0, color=INK, ember=EMBER_700))
    write_svg("wasmnest-lockup-compact.svg", lockup_svg("compact", 30.0))

    # --- 底纹 ---
    write_svg("wasmnest-pattern.svg", wrap(pattern_svg(bg=INK), "0 0 120 120", 480, 480))
    write_svg("wasmnest-pattern-light.svg",
              wrap(pattern_svg(color="#DFE4EA", accent="#EDF0F4", bg=CANVAS), "0 0 120 120", 480, 480))

    print(f"✓ SVG 资产写入 {ASSETS}")

    if args.png:
        if not _which("rsvg-convert"):
            raise SystemExit("缺少 rsvg-convert（PNG 导出）")
        png = ASSETS / "png"
        png.mkdir(exist_ok=True)

        def rsvg(src, size, name):
            subprocess.run(["rsvg-convert", "-w", str(size), str(ASSETS / src),
                            "-o", str(png / name)], check=True)
        ladder = [(16, "wasmnest-icon-simple.svg"), (24, "wasmnest-icon-simple.svg"),
                  (32, "wasmnest-icon.svg"), (48, "wasmnest-icon.svg"), (64, "wasmnest-icon.svg"),
                  (128, "wasmnest-icon.svg"), (256, "wasmnest-icon.svg"), (512, "wasmnest-icon.svg"),
                  (1024, "wasmnest-icon.svg")]
        for size, src in ladder:
            rsvg(src, size, f"wasmnest-icon-{size}.png")
        for name, size in (("wasmnest-mark", 1024), ("wasmnest-mark-mono", 1024),
                           ("wasmnest-mark-ink", 1024), ("wasmnest-mark-solid", 1024),
                           ("wasmnest-icon-light", 512), ("wasmnest-icon-mono", 512),
                           ("wasmnest-lockup", 1024), ("wasmnest-lockup-ink", 1024),
                           ("wasmnest-lockup-compact", 512), ("wasmnest-pattern", 480)):
            rsvg(f"{name}.svg", size, f"{name}-{size}.png")
        # Android / iOS 交付尺寸
        for name, size in (("wasmnest-android-foreground", 432), ("wasmnest-android-background", 432),
                           ("wasmnest-android-monochrome", 432)):
            rsvg(f"{name}.svg", size, f"{name}-{size}.png")
        ico_src = [png / f"wasmnest-icon-{s}.png" for s in (16, 24, 32, 48, 64, 128, 256)]
        build_ico(ico_src, ASSETS / "wasmnest.ico")
        icns_src = [png / f"wasmnest-icon-{s}.png" for s in (16, 32, 64, 128, 256, 512, 1024)]
        build_icns(icns_src, ASSETS / "wasmnest.icns")
        print(f"✓ PNG 阶梯 + wasmnest.ico + wasmnest.icns 写入 {png}")


if __name__ == "__main__":
    main()
