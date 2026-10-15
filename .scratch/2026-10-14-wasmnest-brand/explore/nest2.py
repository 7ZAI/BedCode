#!/usr/bin/env python3
"""WasmNest 标志探索：缠绕 W（四根棒料 + 细线双向缠绕 + 端头缠扎）。

在 nest.py（镜像双向交织）基础上修三处「像弹簧/像蕨叶」的问题：

  1. **端头缠扎（whipping）**：线在两端 1.5 圈内收紧节距（密集绕几圈），
     端头因此是「扎紧的绳头」而不是散开的蕨叶；中段保持疏绕 = 镂空。
     —— 这是船缆/藤编的真实做法，也是标志里最便宜的「工艺感」。
  2. **骨架放中间**：W 的中峰下移（43 → 50），内侧两根棒料的夹角更大，
     两团绕线不再在中脊挤成一坨；外侧两根长棒料的疏绕网格看得最清楚。
  3. **线加粗到 2.0、节距压到 7.5–8.5**：线只占棒面约 30%（两程 60%），
     棒料靠「包络」读成一根缠绕的杆，小尺寸不散架。

用法：python3 nest2.py > nest2.svg && rsvg-convert -z 1 nest2.svg -o nest2.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"
DIM = "#8B939E"

VERTS = [(12.0, 20.0), (30.0, 74.0), (50.0, 50.0), (70.0, 74.0), (88.0, 20.0)]


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
    den = u[0] * v[1] - u[1] * v[0]
    if abs(den) < 1e-9:
        return None
    t = ((q[0] - p[0]) * v[1] - (q[1] - p[1]) * v[0]) / den
    return (p[0] + u[0] * t, p[1] + u[1] * t)


def shift_cut(cut, away, d):
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    if dot(sub(away, p1), nn) > 0:
        nn = mul(nn, -1.0)
    return (add(p1, mul(nn, d)), add(p2, mul(nn, d)))


def rods(R, seam, verts=VERTS):
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
            p1, p2 = A, add(A, mul(n, 1.0))
        if i < 3:
            u_next = norm(sub(verts[i + 2], B))
            nn = perp(u_next)
            q1 = line_inter(add(B, mul(n, R)), u, add(B, mul(nn, R)), u_next)
            q2 = line_inter(add(B, mul(n, -R)), u, add(B, mul(nn, -R)), u_next)
            q1, q2 = shift_cut((q1, q2), mid, seam / 2)
        else:
            q1, q2 = B, add(B, mul(n, 1.0))
        out.append(dict(i=i, A=A, B=B, u=u, n=n, L=L, mid=mid, cut0=(p1, p2), cut1=(q1, q2)))
    return out


def inside(cut, p, ref):
    p1, p2 = cut
    nn = norm(perp(sub(p2, p1)))
    s_ref = dot(sub(ref, p1), nn)
    return dot(sub(p, p1), nn) * s_ref >= 0.0


class Rng:
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


def winding(rod, line, R, pitch, chaos, seed, dirn=1.0, phase=0.0, samples_per_turn=46,
            whip=1.5, whip_tight=0.42, drift=0.0):
    """一程缠绕。dirn=+1 右手螺旋（下行），−1 左手螺旋（回程）。

    端头缠扎：最外 whip 圈（不足则一半圈数）的节距乘 whip_tight（<1 = 更密），
    归一化后中段自然变疏 → 两端扎紧、中段镂空。

    drift：节距沿轴线单调漂移（−1..1）。手缠的节距不会是常数，
    单调漂移比逐圈随机更像「人手绕的」——逐圈随机会读成噪点。
    """
    A, u, n, L = rod["A"], rod["u"], rod["n"], rod["L"]
    rng = Rng(seed * 7919 + rod["i"] * 104729 + int(phase * 1000))
    n_turn = max(2, int(round(L / pitch)))
    k_whip = max(1, min(int(round(whip)), n_turn // 3))
    steps = []
    for k in range(n_turn + 2):
        w = whip_tight if (k < k_whip or k >= n_turn + 2 - k_whip) else 1.0
        d = 1.0 + drift * ((k + 0.5) / (n_turn + 2) - 0.5)
        steps.append(w * d * (1.0 + chaos * rng.rng(-0.26, 0.42)))
    tot = sum(steps)
    marks = [0.0]
    acc = 0.0
    for st in steps:
        acc += st
        marks.append(acc / tot)
    amp = [1.0 + chaos * rng.rng(-0.07, 0.07) for _ in range(n_turn + 3)]
    phj = [chaos * rng.rng(-0.26, 0.26) for _ in range(n_turn + 3)]
    wob = [chaos * rng.rng(-0.16, 0.16) for _ in range(n_turn + 3)]
    wph = [chaos * rng.rng(0, 2 * math.pi) for _ in range(n_turn + 3)]
    Reff = R - line / 2.0
    total = max(48, int(n_turn * samples_per_turn))

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
        # 端头轻微收圆（收到 0.86 而不是收到 0：端头扎紧处不该出现针尖）
        taper = min(1.0, 0.86 + 0.14 * min(1.0, s * n_turn / 0.8, (1.0 - s) * n_turn / 0.8))
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
    def endpt(base, sgn, cut):
        p = line_inter(add(base, mul(rod["n"], sgn * R)), rod["u"], cut[0], sub(cut[1], cut[0]))
        return p or add(base, mul(rod["n"], sgn * R))
    pts = [endpt(rod["A"], 1, rod["cut0"]), endpt(rod["B"], 1, rod["cut1"]),
           endpt(rod["B"], -1, rod["cut1"]), endpt(rod["A"], -1, rod["cut0"])]
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    return f'<polygon points="{p}" fill="{color}"/>'


def build(R=7.8, pitch=8.0, line=2.0, seam=1.6, gap=1.3, chaos=0.35,
          ember_rod=None, solid=False, samples_per_turn=46, whip=1.5, whip_tight=0.42,
          R2=None, verts=VERTS, drift=0.0):
    """R2：内侧两根棒料可用不同的半径（中脊面积小，稍细一点更透气）。"""
    parts = []
    for rod in rods(R, seam, verts):
        rr = R if (R2 is None or rod["i"] in (0, 3)) else R2
        col = EMBER if ember_rod == rod["i"] + 1 else PAPER
        if solid:
            parts.append(rod_solid(rod, rr, col))
            continue
        keep = (lambda p, rod=rod: (rod["i"] == 0 or inside(rod["cut0"], p, rod["mid"]))
                and (rod["i"] == 3 or inside(rod["cut1"], p, rod["mid"])))
        down = split_run(winding(rod, line, rr, pitch, chaos, 3, +1.0, 0.0,
                                 samples_per_turn, whip, whip_tight, drift), keep)
        up = split_run(winding(rod, line, rr, pitch, chaos, 11, -1.0, math.pi / 2,
                               samples_per_turn, whip, whip_tight, -drift), keep)
        crosses = []
        for ui, uu in enumerate(up):
            sl_u = arclen(uu)
            found = []
            for di, d in enumerate(down):
                sl_d = arclen(d)
                for a in range(len(d) - 1):
                    for b in range(len(uu) - 1):
                        hit = seg_cross(d[a], d[a + 1], uu[b], uu[b + 1])
                        if hit:
                            t, ss, ang = hit
                            found.append((sl_u[b] + (sl_u[b + 1] - sl_u[b]) * ss, di,
                                          sl_d[a] + (sl_d[a + 1] - sl_d[a]) * t, ang))
            found.sort()
            for k, (s_u, di, s_d, ang) in enumerate(found):
                crosses.append((k % 2 == 0, ui, s_u, di, s_d, ang))
        d_cuts = [(di, s_d, ang) for (cu, ui, s_u, di, s_d, ang) in crosses if not cu]
        u_cuts = [(ui, s_u, ang) for (cu, ui, s_u, di, s_d, ang) in crosses if cu]
        for run in cut_spans(down, d_cuts, line, gap):
            parts.append(poly(run, line, col))
        for run in cut_spans(up, u_cuts, line, gap):
            parts.append(poly(run, line, col))
    return "".join(parts)


PAD, HEAD, CELL, COLS = 22, 78, 470, 3


def sheet(cases, title, sub):
    """渲染对比表（一张 SVG 到 stdout）。"""
    rows = math.ceil(len(cases) / COLS)
    wid = PAD * 2 + COLS * CELL + PAD * (COLS - 1)
    h = HEAD + rows * (CELL + 96)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{wid}" height="{h}" viewBox="0 0 {wid} {h}">',
           f'<rect width="{wid}" height="{h}" fill="{INK}"/>',
           f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">{title}</text>',
           f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11" fill="#7C848F">{sub}</text>']
    for i, (label, art) in enumerate(cases):
        r, c = divmod(i, COLS)
        x0 = PAD + c * (CELL + PAD)
        y0 = HEAD + r * (CELL + 96)
        out.append(f'<rect x="{x0}" y="{y0}" width="{CELL}" height="{CELL}" rx="14" fill="{INK_2}"/>')
        out.append(f'<g transform="translate({x0},{y0}) scale({CELL/100})">{art}</g>')
        sx = x0 + 8
        for sz in (96, 64, 48, 32, 24, 16):
            out.append(f'<g transform="translate({sx},{y0+CELL+16}) scale({sz/100})">{art}</g>')
            sx += sz + 18
        out.append(f'<text x="{x0+8}" y="{y0+CELL+88}" font-family="Inter" font-size="12.5" fill="{DIM}">{label}</text>')
    out.append('</svg>')
    print("\n".join(out))


if __name__ == "__main__":
    sheet([
        ("A 距8.0 线2.0 棒7.8 乱0.35 扎1.5",
         build(R=7.8, pitch=8.0, line=2.0, seam=1.6, gap=1.3, chaos=0.35, whip=1.5)),
        ("B 距7.0 线2.0 棒8.0 乱0.30 扎2.0",
         build(R=8.0, pitch=7.0, line=2.0, seam=1.6, gap=1.3, chaos=0.30, whip=2.0)),
        ("C 距9.5 线2.2 棒8.0 乱0.45 扎1.5",
         build(R=8.0, pitch=9.5, line=2.2, seam=1.6, gap=1.4, chaos=0.45, whip=1.5)),
        ("D 距8.0 线2.0 棒7.8/7.0 乱0.35 扎2.0",
         build(R=7.8, R2=7.0, pitch=8.0, line=2.0, seam=1.6, gap=1.3, chaos=0.35, whip=2.0)),
        ("E 距6.2 线2.4 棒8.2 乱0.25 扎1.5",
         build(R=8.2, pitch=6.2, line=2.4, seam=1.7, gap=1.4, chaos=0.25, whip=1.5)),
        ("F 距8.0 线2.0 棒7.8 乱0.35 + 琥珀1",
         build(R=7.8, pitch=8.0, line=2.0, seam=1.6, gap=1.3, chaos=0.35, whip=1.5, ember_rod=1)),
        ("G 实心声（≤24px 档）",
         build(R=7.8, pitch=8.0, line=2.0, seam=1.6, chaos=0.35, solid=True)),
        ("H 距10.5 线1.9 棒8.4 乱0.55 扎1.5",
         build(R=8.4, pitch=10.5, line=1.9, seam=1.7, gap=1.4, chaos=0.55, whip=1.5)),
    ], "WasmNest — wound rods with whipped ends",
       "4 rods · thin thread down (right-hand) + back (left-hand) · tightened turns at the tips · open middle · 96/64/48/32/24/16")
