#!/usr/bin/env python3
"""WasmNest 探索 6：细线缠绕 W（一条细线沿 W 路径不断绕圈，圈与圈交错缠绕）。

用户裁定（决定性）：
  * 是**细线**，不是粗环/香肠；
  * 是**不断缠绕**——一条连续不断的线沿 W 的骨架反复绕圈（线团），不是四个独立图形；
  * W 的形状由这条线的**包络**读出来（近看是线团，远看是 W）。

构造：W 骨架（含圆角，读作柔软的线）采样成稠密点列 → 沿法线做**trochoid 绕圈**
（x = aφ − b·sinφ, y = −b·cosφ；b > a 时会出现回退＝真正的圈），
每个整圈切成一个 item，item 之间按 z 序挖缝 → 圈与圈的交叉有上下关系（真交织）。

用法：python3 weave5.py > weave5.svg && rsvg-convert -z 2 weave5.svg -o weave5.png
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
    """W 骨架 → 稠密点列 [(p, u)]，顶点处用半径 corner_r 的圆角过渡。"""
    verts = [T1, V1, P, V2, T2]
    pts = []
    def straight(a, b):
        L = dist(a, b)
        n = max(2, int(L / step))
        for k in range(n + 1):
            t = k / n
            pts.append(((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t), norm(sub(b, a))))
    for i, v in enumerate(verts):
        if i == 0:
            continue
        d_in = norm(sub(v, verts[i - 1]))       # 进入 v 的方向
        if i < len(verts) - 1:
            d_out = norm(sub(verts[i + 1], v))  # 离开 v 的方向
            cosang = max(-1.0, min(1.0, d_in[0] * d_out[0] + d_in[1] * d_out[1]))
            acute = math.acos(cosang)           # 两方向的夹角（≈130°）
            half = (math.pi - acute) / 2.0      # 半张角（≈25°）
            t = corner_r / math.tan(half)
            bis = norm(sub(d_out, d_in))
            c = add(v, mul(bis, corner_r / math.sin(half)))
            pa = sub(v, mul(d_in, t))           # 圆角起点
            pb = add(v, mul(d_out, t))          # 圆角终点
            straight(pts[-1][0] if pts else verts[i - 1], pa)
            # 圆弧采样
            a0 = math.atan2(pa[1] - c[1], pa[0] - c[0])
            a1 = math.atan2(pb[1] - c[1], pb[0] - c[0])
            da = (a1 - a0) % (2 * math.pi)
            if da > math.pi:
                da -= 2 * math.pi
            n = max(4, int(abs(da) * corner_r / step))
            for k in range(1, n + 1):
                ang = a0 + da * k / n
                p = (c[0] + corner_r * math.cos(ang), c[1] + corner_r * math.sin(ang))
                pts.append((p, norm((-math.sin(ang) * (1 if da > 0 else -1),
                                     math.cos(ang) * (1 if da > 0 else -1)))))
        else:
            straight(pts[-1][0], v)
    return pts

def arclen(pts):
    s, out = 0.0, [0.0]
    for i in range(1, len(pts)):
        s += dist(pts[i][0], pts[i - 1][0])
        out.append(s)
    return out

def coil_items(pts, b, pitch, flip=False, per_item=14):
    """沿路径做 trochoid 缠绕，切成 items（每条 item = 半圈，交叉配对干净且数量可控）。"""
    sl = arclen(pts)
    L = sl[-1]
    a_nom = pitch / (2 * math.pi)
    n_half = max(4, round(L / (a_nom * math.pi)))  # 半圈数；相位收在 π 的整数倍 → 端点落在中心线上
    phi_end = n_half * math.pi
    a = L / phi_end
    sgn = -1.0 if flip else 1.0

    def at(phi):
        adv = a * phi - b * math.sin(phi)
        lo, hi = 0, len(sl) - 1
        while lo < hi - 1:
            mid = (lo + hi) // 2
            if sl[mid] <= adv:
                lo = mid
            else:
                hi = mid
        seg = (sl[hi] - sl[lo]) or 1.0
        t = (adv - sl[lo]) / seg
        p0, u0 = pts[lo]
        p1, u1 = pts[hi]
        bp = (p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t)
        uu = norm((u0[0] + (u1[0] - u0[0]) * t, u0[1] + (u1[1] - u0[1]) * t))
        nn = (-uu[1], uu[0])
        off = sgn * -b * math.cos(phi)
        return (bp[0] + nn[0] * off, bp[1] + nn[1] * off)

    items = []
    for i in range(n_half):
        cur = [at(phi_end * (i * per_item + k) / (n_half * per_item)) for k in range(per_item + 1)]
        items.append(cur)
    return items

def poly(points, w, color, mask=""):
    p = " ".join(f"{x:.2f},{y:.2f}" for x, y in points)
    return (f'<polyline points="{p}" fill="none" stroke="{color}" stroke-width="{w:.2f}" '
            f'stroke-linecap="butt" stroke-linejoin="round" {mask}/>')

def bbox(points, pad=0.0):
    xs = [p[0] for p in points]; ys = [p[1] for p in points]
    return (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)

def build(line=2.6, b=5.0, pitch=8.0, corner_r=8.0, gap=1.1, pattern="later",
          flip=False, ember_turns=None):
    pts = fillet_path(corner_r)
    items = coil_items(pts, b, pitch, flip=flip)
    boxes = [bbox(it) for it in items]
    def hits(i, j):
        a, bb = boxes[i], boxes[j]
        return not (a[2] < bb[0] or bb[2] < a[0] or a[3] < bb[1] or bb[3] < a[1])
    def over(i, j):
        if pattern == "later":
            return i > j
        return (i + j) % 2 == 0   # alternate
        
    masks = {i: [] for i in range(len(items))}
    for i in range(len(items)):
        for j in range(len(items)):
            if i != j and hits(i, j) and not over(i, j):
                masks[i].append(j)
    defs, out = [], []
    for i, it in enumerate(items):
        col = PAPER
        if ember_turns and i in ember_turns:
            col = EMBER
        m = ""
        if masks[i]:
            mid = f"c{i}"
            body = "".join(poly(items[j], line + 2 * gap, "#000") for j in masks[i])
            defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="-70" y="-70" width="240" height="240">'
                        f'<rect x="-70" y="-70" width="240" height="240" fill="#fff"/>{body}</mask>')
            m = f'mask="url(#{mid})"'
        out.append(poly(it, line, col, m))
    return (f"<defs>{''.join(defs)}</defs>" if defs else "") + "".join(out)

CASES = [
    ("X1 线2.6 圈5.0 距8.0 r8", build()),
    ("X2 线2.6 圈5.0 距6.0 r8（更密）", build(pitch=6.0)),
    ("X3 线3.2 圈6.0 距7.0 r10", build(line=3.2, b=6.0, pitch=7.0, corner_r=10)),
    ("X4 线2.0 圈4.2 距9.0 r6（最细）", build(line=2.0, b=4.2, pitch=9.0, corner_r=6)),
    ("X5 线3.0 圈5.5 距8.0 r14（软 W）", build(line=3.0, b=5.5, pitch=8.0, corner_r=14)),
    ("X6 交替上下（交织感强）", build(pattern="alternate")),
    ("X7 反向缠绕", build(flip=True)),
    ("X8 线2.6 圈6.5 距5.0（紧绕）", build(b=6.5, pitch=5.0)),
    ("X9 首尾各两圈染琥珀", build(ember_turns=set(list(range(2)) + list(range(-2, 0))))),
]

ZW, WW, PAD = 214, 300, 20
ROW, HEAD = 372, 76
COLS = 3
ROWS = math.ceil(len(CASES) / COLS)
WID = PAD * 2 + COLS * (ZW + 14 + WW + 30)
H = HEAD + ROWS * (ROW + 26)
ZOOM_V = (8.0, 54.0, 46.0)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WID}" height="{H}" viewBox="0 0 {WID} {H}">',
       f'<rect width="{WID}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="38" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — thin thread, wound W</text>',
       f'<text x="{PAD}" y="60" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'one continuous thread wound along the W skeleton · loop crossings woven over/under · left valley zoom · 48/32/24/16</text>']
for i, (label, art) in enumerate(CASES):
    r, c = divmod(i, COLS)
    x0 = PAD + c * (ZW + 14 + WW + 30)
    y0 = HEAD + r * (ROW + 26)
    zx, zy, zs = ZOOM_V
    svg.append(f'<rect x="{x0}" y="{y0}" width="{ZW}" height="{ROW-40}" rx="12" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0},{y0}) scale({ZW/zs}) translate({-zx},{-zy})">{art}</g>')
    svg.append(f'<rect x="{x0+ZW+14}" y="{y0}" width="{WW}" height="{WW}" rx="12" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0+ZW+14},{y0}) scale({WW/100})">{art}</g>')
    sx = x0 + ZW + 14
    for s in (48, 32, 24, 16):
        svg.append(f'<g transform="translate({sx},{y0+WW+10}) scale({s/100})">{art}</g>')
        sx += s + 10
    svg.append(f'<text x="{x0}" y="{y0+ROW-2}" font-family="Inter" font-size="12" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
