#!/usr/bin/env python3
"""WasmNest 标志：缠绕 W —— 四根棒料 + 一根细线的双向交织（真镂空巢材感）。

与 wound.py 的差别（本次按用户「非常细的线 / 交织 / 空隙大 / 错乱」重做）：

  1. **两程反向绕**（下行右手螺旋 + 回程左手螺旋，即 dirn=+1 / −1），
     而不是同向异相位——同向异相位只会得到「羽状 / 人字纹」，
     反向才是真正的 X 交织（篮编 / 中国指套的织法），交叉角大、空隙成菱形。
  2. **线更细**（1.5–1.8）、**节距更大**（10–14）→ 每个交叉之间的空隙是
     线宽的 6–8 倍，镂空真正看得见；线只占棒料表面约 25%，
     靠「包络」而不是「填满」来定义圆柱。
  3. 错乱来自**节距抖动 + 逐圈相位抖动 + 低频摆动**，而不是幅度乱抖——
     幅度乱抖会让棒料轮廓起球（wound3 的端头就是那样炸开的）。
  4. 端头**收绕**到中轴附近（幂型 taper），W 的两端因此呈收尖锐形，不再起刷子。

用法：
    python3 nest.py > nest.svg && rsvg-convert -z 2 nest.svg -o nest.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"
DIM = "#8B939E"

# W 骨架顶点（四根棒料的中心线端点）——沿用 v1 的比例
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


def shift(cut, away, d):
    """把切面沿其法线朝远离 away 的一侧平移 d（等宽接缝）。"""
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    if dot(sub(away, p1), nn) > 0:
        nn = mul(nn, -1.0)
    return (add(p1, mul(nn, d)), add(p2, mul(nn, d)))


def rods(R, seam):
    """W → 四根棒料（中心线、方向、法线、两端斜接切面）。

    接缝用「沿切面法线平移」而不是「沿轴线缩短」：后者缝宽从内侧 0 渐变到
    外侧 2·seam·sin(θ/2)，接点会长成箭头状缺口。
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
            p1, p2 = shift((p1, p2), add(A, mul(u, dist(A, B) * 0.5)), seam / 2)
        else:
            p1, p2 = A, add(A, mul(n, 1.0))          # 自由端：垂直于轴线的平切
        if i < 3:
            u_next = norm(sub(VERTS[i + 2], B))
            nn = perp(u_next)
            q1 = line_inter(add(B, mul(n, R)), u, add(B, mul(nn, R)), u_next)
            q2 = line_inter(add(B, mul(n, -R)), u, add(B, mul(nn, -R)), u_next)
            q1, q2 = shift((q1, q2), add(A, mul(u, dist(A, B) * 0.5)), seam / 2)
        else:
            q1, q2 = B, add(B, mul(n, 1.0))
        out.append(dict(i=i, A=A, B=B, u=u, n=n, L=dist(A, B), mid=add(A, mul(u, dist(A, B) * 0.5)),
                        cut0=(p1, p2), cut1=(q1, q2)))
    return out


def inside(cut, p, ref):
    """点 p 是否在切面 cut 的保留侧（用 ref 定参考侧，不猜符号）。"""
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    s_ref = dot(sub(ref, p1), nn)
    return dot(sub(p, p1), nn) * s_ref >= 0.0


class Rng:
    """xorshift32：标志资产必须可复现，不能每次渲染都不同。"""
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


def winding(rod, line, R, pitch, chaos, seed, dirn=1.0, phase=0.0, samples_per_turn=44):
    """一程缠绕。dirn=+1 右手螺旋（下行），dirn=−1 左手螺旋（回程）。

    错乱的三味调料（chaos=0 退化为规整螺旋）：
      * 逐圈节距抖动 steps  → 交叉位置不排成整齐一行
      * 逐圈相位抖动 phj    → 圈与圈之间的「接线头」不在同一母线
      * 低频摆动 wob        → 整段的绕向微微斜，交叉角不齐
    端部 taper 用幂型收敛到中轴（线头线尾收绕），避免端头炸成刷子。
    """
    A, u, n, L = rod["A"], rod["u"], rod["n"], rod["L"]
    rng = Rng(seed * 7919 + rod["i"] * 104729 + int(phase * 1000))
    n_turn = max(2, int(round(L / pitch)))
    steps = [1.0 + chaos * rng.rng(-0.30, 0.55) for _ in range(n_turn + 2)]
    tot = sum(steps)
    marks = [0.0]
    acc = 0.0
    for st in steps:
        acc += st
        marks.append(acc / tot)
    amp = [1.0 + chaos * rng.rng(-0.08, 0.08) for _ in range(n_turn + 3)]
    phj = [chaos * rng.rng(-0.30, 0.30) for _ in range(n_turn + 3)]
    wob = [chaos * rng.rng(-0.20, 0.20) for _ in range(n_turn + 3)]
    wph = [chaos * rng.rng(0, 2 * math.pi) for _ in range(n_turn + 3)]
    Reff = R - line / 2.0
    total = max(48, int(n_turn * samples_per_turn))
    tl = min(0.30, 1.3 / n_turn)      # 两端的收绕长度（占棒料长度的比例）

    def off_at(s):
        k = 0
        while k < n_turn + 1 and marks[k + 1] < s:
            k += 1
        k = min(k, n_turn)
        span = (marks[k + 1] - marks[k]) or 1.0
        t = (s - marks[k]) / span
        theta = 2 * math.pi * dirn * (k + t) + phase
        a_ = Reff * (amp[k] * (1 - t) + amp[k + 1] * t)
        w_ = ((wob[k] * (1 - t) + wob[k + 1] * t)
              + (phj[k] * (1 - t) + phj[k + 1] * t)
              + 0.05 * chaos * math.sin(2 * math.pi * s * 0.9 + wph[k]))
        # 幂型收绕：贴到端头附近才收进中轴，且不收到 0（留一点点，避免「针尖」）
        taper = 0.22 + 0.78 * min(1.0, max(0.0, s / tl), max(0.0, (1.0 - s) / tl)) ** 0.75
        return a_ * math.cos(theta + w_) * taper

    return [add(add(A, mul(u, L * (k / total))), mul(n, off_at(k / total))) for k in range(total + 1)]


def split_run(pts, keep):
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
        return (t, ss, ang)
    return None


def cut_spans(runs, cuts, line, gap, min_sin=0.22):
    """把每个 run 按 (run 号, 弧长, 交叉角) 挖成多段；返回 [[run...], ...] 的段列表。"""
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


def poly(pts, w, color):
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    return (f'<polyline points="{p}" fill="none" stroke="{color}" stroke-width="{w:.2f}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>')


def rod_solid(rod, R, color):
    """实心态的一根棒料：两端按斜接切面截断的多边形。"""
    A, u, n = rod["A"], rod["u"], rod["n"]
    def endpt(base, sgn, cut):
        d = sub(cut[1], cut[0])
        p = line_inter(add(base, mul(n, sgn * R)), u, cut[0], d)
        return p or add(base, mul(n, sgn * R))
    poly_pts = [endpt(A, 1, rod["cut0"]), endpt(rod["B"], 1, rod["cut1"]),
                endpt(rod["B"], -1, rod["cut1"]), endpt(A, -1, rod["cut0"])]
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in poly_pts)
    return f'<polygon points="{p}" fill="{color}"/>'


def build(R=7.4, pitch=12.0, line=1.7, seam=1.6, gap=1.4, chaos=0.40,
          ember_rod=None, solid=False, samples_per_turn=44):
    """整条 W：一根线依次绕四根棒料，下绕（右手）+ 回绕（左手）在表面织成 X。"""
    parts = []
    for rod in rods(R, seam):
        col = EMBER if ember_rod == rod["i"] + 1 else PAPER
        if solid:
            parts.append(rod_solid(rod, R, col))
            continue
        keep = (lambda p, rod=rod: (rod["i"] == 0 or inside(rod["cut0"], p, rod["mid"]))
                and (rod["i"] == 3 or inside(rod["cut1"], p, rod["mid"])))
        down = split_run(winding(rod, line, R, pitch, chaos, 3, +1.0, 0.0, samples_per_turn), keep)
        up = split_run(winding(rod, line, R, pitch, chaos, 11, -1.0, math.pi / 2, samples_per_turn), keep)

        # 交叉：按「回程线的自身弧长」排序后交替上下——这样回程线一路是
        # 压—被压—压—被压（平纹编织），而不是按全局序号乱压。
        crosses = []
        for ui, uu in enumerate(up):
            sl_u = arclen(uu)
            runs_d = []
            for di, d in enumerate(down):
                sl_d = arclen(d)
                for a in range(len(d) - 1):
                    for b in range(len(uu) - 1):
                        hit = seg_cross(d[a], d[a + 1], uu[b], uu[b + 1])
                        if hit:
                            t, ss, ang = hit
                            runs_d.append((sl_u[b] + (sl_u[b + 1] - sl_u[b]) * ss, di,
                                           sl_d[a] + (sl_d[a + 1] - sl_d[a]) * t, ang))
            if not runs_d:
                continue
            runs_d.sort()
            for k, (s_u, di, s_d, ang) in enumerate(runs_d):
                crosses.append((k % 2 == 0, ui, s_u, di, s_d, ang))
        d_cuts = [(di, s_d, ang) for (cutup, ui, s_u, di, s_d, ang) in crosses if not cutup]
        u_cuts = [(ui, s_u, ang) for (cutup, ui, s_u, di, s_d, ang) in crosses if cutup]
        for run in cut_spans(down, d_cuts, line, gap):
            parts.append(poly(run, line, col))
        for run in cut_spans(up, u_cuts, line, gap):
            parts.append(poly(run, line, col))
    return "".join(parts)


CASES = [
    ("A 距12 线1.7 棒7.4 乱0.40", build(R=7.4, pitch=12.0, line=1.7, seam=1.6, gap=1.4, chaos=0.40)),
    ("B 距10 线1.6 棒7.6 乱0.50", build(R=7.6, pitch=10.0, line=1.6, seam=1.6, gap=1.5, chaos=0.50)),
    ("C 距14 线1.8 棒7.2 乱0.30", build(R=7.2, pitch=14.0, line=1.8, seam=1.7, gap=1.5, chaos=0.30)),
    ("D 距12 线1.4 棒7.8 乱0.45", build(R=7.8, pitch=12.0, line=1.4, seam=1.6, gap=1.4, chaos=0.45)),
    ("E 距9 线1.7 棒7.6 乱0.55", build(R=7.6, pitch=9.0, line=1.7, seam=1.6, gap=1.4, chaos=0.55)),
    ("F 距12 线1.7 棒7.4 乱0.40+琥珀棒2", build(R=7.4, pitch=12.0, line=1.7, seam=1.6, gap=1.4, chaos=0.40, ember_rod=2)),
    ("G 实心声（小尺寸档）", build(R=7.4, pitch=12.0, line=1.7, seam=1.6, gap=1.4, chaos=0.40, solid=True)),
    ("H 距11 线1.5 棒7.6 乱0.65", build(R=7.6, pitch=11.0, line=1.5, seam=1.6, gap=1.5, chaos=0.65)),
]

PAD, HEAD, CELL, COLS = 22, 78, 470, 3
ROWS = math.ceil(len(CASES) / COLS)
WID = PAD * 2 + COLS * CELL + PAD * (COLS - 1)
H = HEAD + ROWS * (CELL + 96)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WID}" height="{H}" viewBox="0 0 {WID} {H}">',
       f'<rect width="{WID}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — woven lattice W (mirror-handed passes)</text>',
       f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'4 rods · thin thread down (right-hand) + back (left-hand) · plain-weave over/under · sizes 96/64/48/32/24/16</text>']
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
    svg.append(f'<text x="{x0+8}" y="{y0+CELL+88}" font-family="Inter" font-size="12.5" fill="{DIM}">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
