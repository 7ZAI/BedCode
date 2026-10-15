#!/usr/bin/env python3
"""WasmNest 标志：缠绕 W（W 拆成圆柱体，细线螺旋绕下、再绕回，形成 X 交织）。

用户裁定的构造（本次按此实现，不再自创）：
  1. W 拆成**四根圆柱体**（每段笔画 = 一根棒料），棒料之间用斜接缝（miter）相接，
     接缝留一道细缝 = 隔离边界（延续 v1「四段分离笔画」的语言）；
  2. 一根**很细的线**沿棒料螺旋绕下去（下行的螺旋）；
  3. 到棒料末端后**反向绕回来**（回程螺旋，相位差半周期），两程在棒料表面交叉成 X；
  4. 交叉处按上下关系挖缝 → 真正的缠绕交织；
  5. 线从一根棒料接着绕到下一根（连续不断），整条 W 由同一根线完成。

读法（三个尺度都对）：
  * 大尺寸：细线的 X 交织清晰可见，棒料之间的斜接缝把 W 分成四根独立的棒料；
  * 中尺寸：绕线并成实心笔画，仍能看出 X 纹理，W 轮廓干净；
  * 小尺寸：并成一根粗实心笔画（退化为 v1 那样的粗 W），不糊。

用法：python3 wound.py > wound.svg && rsvg-convert -z 2 wound.svg -o wound.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"

# W 骨架顶点（四根棒料的中心线端点）
VERTS = [(12.0, 21.0), (31.0, 75.0), (50.0, 43.0), (69.0, 75.0), (88.0, 21.0)]

def sub(a, b): return (a[0] - b[0], a[1] - b[1])
def add(a, b): return (a[0] + b[0], a[1] + b[1])
def mul(a, s): return (a[0] * s, a[1] * s)
def dot(a, b): return a[0] * b[0] + a[1] * b[1]
def norm(a):
    L = math.hypot(*a) or 1.0
    return (a[0] / L, a[1] / L)
def perp(a): return (-a[1], a[0])
def dist(a, b): return math.hypot(a[0] - b[0], a[1] - b[1])

def line_inter(p, u, q, v):
    """直线 p+t·u 与 q+s·v 的交点。"""
    den = u[0] * v[1] - u[1] * v[0]
    if abs(den) < 1e-9:
        return None
    t = ((q[0] - p[0]) * v[1] - (q[1] - p[1]) * v[0]) / den
    return (p[0] + u[0] * t, p[1] + u[1] * t)

def rods(R, seam):
    """把 W 拆成四根棒料。

    每根棒料给出中心线端点、方向、法线，以及两端的**斜接切面**（miter）。
    切面用「沿该切面法线平移 seam/2」形成接缝，而不是沿各自轴线缩短——
    沿轴线缩短会让缝宽从内侧的 0 渐变到外侧的 2·seam·sin(θ/2)，接点会长成
    箭头状的三角缺口（上一版渲染里的箭头就是这么来的）。沿公共法线平移才是等宽缝。
    """
    out = []
    for i in range(4):
        A, B = VERTS[i], VERTS[i + 1]
        u = norm(sub(B, A))
        n = perp(u)
        if i > 0:
            u_prev = norm(sub(A, VERTS[i - 1]))
            np_ = perp(u_prev)
            p1 = line_inter(add(A, mul(np_, R)), u_prev, add(A, mul(n, R)), u)
            p2 = line_inter(add(A, mul(np_, -R)), u_prev, add(A, mul(n, -R)), u)
            p1, p2 = shift(cut=(p1, p2), away=add(A, mul(u, dist(A, B) * 0.5)), d=seam / 2)
        else:
            p1, p2 = A, add(A, mul(n, 1.0))          # 自由端：垂直于轴线的平切
        if i < 3:
            u_next = norm(sub(VERTS[i + 2], B))
            nn = perp(u_next)
            q1 = line_inter(add(B, mul(n, R)), u, add(B, mul(nn, R)), u_next)
            q2 = line_inter(add(B, mul(n, -R)), u, add(B, mul(nn, -R)), u_next)
            q1, q2 = shift(cut=(q1, q2), away=add(A, mul(u, dist(A, B) * 0.5)), d=seam / 2)
        else:
            q1, q2 = B, add(B, mul(n, 1.0))
        out.append(dict(i=i, A=A, B=B, u=u, n=n, L=dist(A, B), cut0=(p1, p2), cut1=(q1, q2)))
    return out

def shift(cut, away, d):
    """把切面沿其法线朝远离 away 的方向平移 d。"""
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    if dot(sub(away, p1), nn) > 0:
        nn = mul(nn, -1.0)
    return (add(p1, mul(nn, d)), add(p2, mul(nn, d)))

def inside(cut, p, ref):
    """点 p 是否在切面 cut 的「保留侧」——用 ref（棒料内部一点）定参考侧，不猜符号。"""
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    s_ref = dot(sub(ref, p1), nn)
    return dot(sub(p, p1), nn) * s_ref >= 0.0

class Rng:
    """极小的确定性 PRNG（xorshift32）：标志资产必须可复现，不能每次渲染都不一样。"""
    def __init__(self, seed):
        self.s = seed & 0xFFFFFFFF or 0x9E3779B9
    def nxt(self):
        x = self.s
        x ^= (x << 13) & 0xFFFFFFFF
        x ^= x >> 17
        x ^= (x << 5) & 0xFFFFFFFF
        self.s = x & 0xFFFFFFFF
        return self.s / 0xFFFFFFFF
    def rng(self, a, b):
        return a + (b - a) * self.nxt()

def helix(rod, turns, phase, line, R, seed=1, chaos=0.5, samples_per_turn=30, travel=1):
    """一程不规则缠绕：节距抖动 + 幅度起伏 + 低频摆动，两程用不同 seed → 交叉角度不齐。

    规则螺旋读作「机械绕线」；真实巢材/线团的特点是**节距不匀、圈有大有小、绕向微抖**。
    这里把三样都参数化，chaos=0 退化为规则螺旋。
    """
    A, u, n, L = rod["A"], rod["u"], rod["n"], rod["L"]
    rng = Rng(seed * 7919 + rod["i"] * 104729 + int(phase * 1000))
    # 每圈节距抖动
    n_turn = max(2, int(round(turns)))
    steps = [1.0 + chaos * rng.rng(-0.34, 0.62) for _ in range(n_turn + 2)]
    tot = sum(steps)
    marks = [0.0]
    acc = 0.0
    for st in steps:
        acc += st
        marks.append(acc / tot)
    # 幅度起伏 + 低频摆动
    amp = [1.0 + chaos * rng.rng(-0.10, 0.10) for _ in range(n_turn + 2)]
    wob = [chaos * rng.rng(-0.28, 0.28) for _ in range(n_turn + 2)]
    wph = [chaos * rng.rng(0, 2 * math.pi) for _ in range(n_turn + 2)]
    Reff = R - line / 2.0
    total = max(40, int(turns * samples_per_turn))
    def at(s):
        # 找到 s 落在第几圈，圈内做参数插值
        k = 0
        while k < n_turn + 1 and marks[k + 1] < s:
            k += 1
        k = min(k, n_turn)
        span = (marks[k + 1] - marks[k]) or 1.0
        t = (s - marks[k]) / span
        # 圈内角度从 k·2π 到 (k+1)·2π；节距不匀体现在 marks 上
        theta = 2 * math.pi * (k + t) + phase
        # 端部收敛（收绕）：线头线尾收进中轴附近，避免端头炸成刷子
        taper = min(1.0, 0.45 + s / 0.10, 0.45 + (1.0 - s) / 0.10)
        a_ = Reff * (amp[k] * (1 - t) + amp[k + 1] * t) * taper
        w_ = (wob[k] * (1 - t) + wob[k + 1] * t) + 0.05 * chaos * math.sin(2 * math.pi * s * 0.8 + wph[k])
        return a_ * math.cos(theta + w_)
    pts = []
    for k in range(total + 1):
        s = travel * k / total
        off = at(s)
        pts.append(add(add(A, mul(u, L * s)), mul(n, off)))
    return pts

def split_run(pts, keep):
    """按 keep(点) 把点列切成若干连续段。"""
    runs, cur = [], []
    for p in pts:
        if keep(p):
            cur.append(p)
        else:
            if len(cur) > 1:
                runs.append(cur)
            cur = []
    if len(cur) > 1:
        runs.append(cur)
    return runs

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
        ang = abs(math.atan2(den, d1[0] * d2[0] + d1[1] * d2[1]))
        return (t, ang)
    return None

def cut_runs(runs, cuts, line, gap, min_sin=0.30):
    """按 (段号, 弧长, 交叉角) 在 run 上挖缝。"""
    out = []
    for ri, pts in enumerate(runs):
        sl = arclen(pts)
        total = sl[-1]
        spans = []
        for (r, pos, ang) in cuts:
            if r != ri:
                continue
            if math.sin(ang) < min_sin:
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
                cur.append(at(a)); out.append(cur)
            while idx < len(pts) - 1 and sl[idx] < b:
                idx += 1
            cur = [at(b)]
        while idx < len(pts):
            cur.append(pts[idx]); idx += 1
        if len(cur) > 1:
            out.append(cur)
    return out

def poly(pts, w, color):
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    return (f'<polyline points="{p}" fill="none" stroke="{color}" stroke-width="{w:.2f}" '
            f'stroke-linecap="butt" stroke-linejoin="round"/>')

def build(R=7.6, pitch=5.4, line=2.0, seam=1.1, gap=0.85, ember_rod=None, chaos=0.55):
    """一根连续的细线依次绕四根棒料：下绕、回绕、再进下一根。"""
    out, defs = [], []
    for rod in rods(R, seam):
        L, turns_f = rod["L"], rod["L"] / pitch
        turns = max(2, round(turns_f * 2) / 2)     # 半圈整数，保证端部对称
        mid = add(rod["A"], mul(rod["u"], rod["L"] * 0.5))
        def keep(p):
            ok = True
            if rod["i"] > 0:
                ok = ok and inside(rod["cut0"], p, mid)
            if rod["i"] < 3:
                ok = ok and inside(rod["cut1"], p, mid)
            return ok
        down = split_run(helix(rod, turns, 0.0, line, R, seed=3, chaos=chaos), keep)
        up = split_run(helix(rod, turns, math.pi, line, R, seed=11, chaos=chaos), keep)
        # 两程交叉 → 按序交替上下
        pairs = []
        for di, d in enumerate(down):
            for ui, uu in enumerate(up):
                sl_d, sl_u = arclen(d), arclen(uu)
                for a in range(len(d) - 1):
                    for b in range(len(uu) - 1):
                        hit = seg_cross(d[a], d[a + 1], uu[b], uu[b + 1])
                        if hit:
                            t, ang = hit
                            pairs.append((sl_d[a] + (sl_d[a + 1] - sl_d[a]) * t, di, ui, b,
                                          sl_u[b] + (sl_u[b + 1] - sl_u[b]) * hit[0], ang))
        pairs.sort()
        d_cuts, u_cuts = [], []
        for k, (s_d, di, ui, b, s_u, ang) in enumerate(pairs):
            if k % 2 == 0:
                d_cuts.append((di, s_d, ang))
            else:
                u_cuts.append((ui, s_u, ang))
        col = EMBER if ember_rod == rod["i"] + 1 else PAPER
        for run in cut_runs(down, d_cuts, line, gap):
            out.append(poly(run, line, col))
        for run in cut_runs(up, u_cuts, line, gap):
            out.append(poly(run, line, col))
    return "".join(out)

CASES = [
    ("H1 距8.5 线2.2 缝1.6 乱0.45 收绕", build(pitch=8.5, line=2.2, seam=1.6, gap=1.6, chaos=0.45)),
    ("H2 距8.5 线2.2 缝1.8 乱0.62 收绕", build(pitch=8.5, line=2.2, seam=1.6, gap=1.8, chaos=0.62)),
    ("H3 距9.6 线2.2 缝1.8 乱0.62", build(pitch=9.6, line=2.2, seam=1.6, gap=1.8, chaos=0.62)),
    ("H4 距7.6 线2.0 缝1.5 乱0.55（细密一点）", build(pitch=7.6, line=2.0, seam=1.5, gap=1.6, chaos=0.55)),
    ("H5 距8.5 线2.4 缝1.8 乱0.7（线更粗）", build(pitch=8.5, line=2.4, seam=1.6, gap=1.8, chaos=0.7)),
    ("H6 距10.5 线2.2 缝1.8 乱0.55（最疏）", build(pitch=10.5, line=2.2, seam=1.6, gap=1.8, chaos=0.55)),
]

PAD, HEAD, CELL, COLS = 22, 78, 470, 3
ROWS = math.ceil(len(CASES) / COLS)
WID = PAD * 2 + COLS * CELL + PAD * (COLS - 1)
H = HEAD + ROWS * (CELL + 96)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WID}" height="{H}" viewBox="0 0 {WID} {H}">',
       f'<rect width="{WID}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — W as wound cylinders</text>',
       f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'W split into 4 cylinders · one thin thread spirals down and back on each · crossings woven over/under · sizes 96/64/48/32/24/16</text>']
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
