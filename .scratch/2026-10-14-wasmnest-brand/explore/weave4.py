#!/usr/bin/env python3
"""WasmNest 探索 5：线团 W（每条笔画 = 一条闭合的绳环，循环缠绕交织）。

方向（用户裁定）：不用直笔画，用「互相缠绕交织的线团曲线」形成 W。
构造：把 W 的四段笔画各画成**一条闭合的绳环**（两股绳 + 两端圆帽），
  1) 闭合 → 读作「一圈绳子」而不是「两条平行线」；
  2) 环内留空 → 镂空；
  3) 环在接点处互相穿过（上方环的股在下方环上挖缝）= 真正的缠绕交织；
  4) 环可微弓（bow），读作柔软的绳而不是刚性胶囊。
语义：宿主与实例都是「一圈圈的巢材」，彼此穿绕而不融合；琥珀环 = 正在运行的那一环。

用法：python3 weave4.py > weave4.svg && rsvg-convert -z 2 weave4.svg -o weave4.png
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

def unit(a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    L = math.hypot(dx, dy)
    return (dx / L, dy / L)

def add(p, v, s):
    return (p[0] + v[0] * s, p[1] + v[1] * s)

def ext(p, u, s):
    return add(p, u, s)

def norm(u):
    return (-u[1], u[0])

def segs(ov, un):
    """四段笔画中心线；ov = 作上方环时相对顶点（正=出头/负=缩进），un = 作下方环时的伸出量。"""
    u1, u2, u3, u4 = unit(T1, V1), unit(V1, P), unit(P, V2), unit(V2, T2)
    return [
        dict(i=1, a=ext(T1, u1, 0), b=ext(V1, u1, ov), u=u1),
        dict(i=2, a=ext(V1, u2, un), b=ext(P, u2, ov), u=u2),
        dict(i=3, a=ext(P, u3, un), b=ext(V2, u3, ov), u=u3),
        dict(i=4, a=ext(V2, u4, un), b=ext(T2, u4, 0), u=u4),
    ]

def rail_path(p0, p1, u, off, bow, d):
    """一条股（含两端圆帽）的闭合路径：从 p0 侧到 p1 侧，再绕过端帽回来。"""
    n = norm(u)
    a0 = add(p0, n, off)
    a1 = add(p1, n, off)
    b0 = add(p0, n, -off)
    b1 = add(p1, n, -off)
    k = 4.0 / 3.0 * d  # 半圆的三次贝塞尔近似系数
    if bow:
        ca = add(((a0[0] + a1[0]) / 2, (a0[1] + a1[1]) / 2), n, bow)
        cb = add(((b0[0] + b1[0]) / 2, (b0[1] + b1[1]) / 2), n, bow)
        ab0, ab1 = add(a0, n, bow * 0.5), add(a1, n, bow * 0.5)
        bb0, bb1 = add(b0, n, bow * 0.5), add(b1, n, bow * 0.5)
    else:
        ca = ((a0[0] + a1[0]) / 2, (a0[1] + a1[1]) / 2)
        cb = ((b0[0] + b1[0]) / 2, (b0[1] + b1[1]) / 2)
        ab0, ab1, bb0, bb1 = a0, a1, b0, b1
    f = lambda p: f"{p[0]:.2f},{p[1]:.2f}"
    return (f"M{f(a0)} Q{f(ca)} {f(a1)} "
            f"C{f(add(a1, u, k))} {f(add(b1, u, k))} {f(b1)} "
            f"Q{f(cb)} {f(b0)} "
            f"C{f(add(b0, u, -k))} {f(add(a0, u, -k))} {f(a0)} Z")

def build(tw=5.5, sep=4.0, ov=-3.0, un=17.0, gap=2.2, ring_bow=0.0, ember_ring=None,
          extra=(), bow_sign=(1, 1, 1, 1)):
    """tw 绳宽 / sep 环内空 / ring_bow 环弓（沿法线，正负由 bow_sign 逐段决定）/ extra 额外缠绕环。"""
    S = segs(ov, un)
    d = (sep + tw) / 2
    items, rings = [], []
    for s in S:
        n = norm(s["u"])
        bow = ring_bow * bow_sign[s["i"] - 1]
        ri = len(rings)
        rings.append(dict(seg=s["i"], bow=bow))
        for k in (1, -1):
            off = d * k
            items.append(dict(ring=ri, k=k, path=rail_path(s["a"], s["b"], s["u"], off, bow, d),
                              a=add(s["a"], n, off), b=add(s["b"], n, off), w=tw,
                              color=(EMBER if ember_ring == s["i"] and False else PAPER)))
    # 额外缠绕环（不参与 W 骨架，只做线团密度）
    for (ca, cb, tw2, extra_bow) in extra:
        ue = unit(ca, cb)
        ne = norm(ue)
        d2 = (sep * 0.7 + tw2) / 2
        ri = len(rings)
        rings.append(dict(seg=80 + ri, bow=extra_bow))
        for k in (1, -1):
            off = d2 * k
            items.append(dict(ring=ri, k=k, path=rail_path(ca, cb, ue, off, extra_bow, d2),
                              a=add(ca, ne, off), b=add(cb, ne, off), w=tw2, color=PAPER))
    # 琥珀：整环染色
    for it in items:
        if ember_ring is not None and rings[it["ring"]]["seg"] == ember_ring:
            it["color"] = EMBER

    def over(A, B):
        return rings[A["ring"]]["seg"] < rings[B["ring"]]["seg"]

    def crosses(A, B):
        if A["ring"] == B["ring"]:
            return False
        (ax1, ay1), (ax2, ay2) = A["a"], A["b"]
        (bx1, by1), (bx2, by2) = B["a"], B["b"]
        if abs((ax2 - ax1) * (by2 - by1) - (ay2 - ay1) * (bx2 - bx1)) < 1e-6:
            return False
        x1, x2 = sorted((ax1, ax2)); y1, y2 = sorted((ay1, ay2))
        z1, z2 = sorted((bx1, bx2)); w1, w2 = sorted((by1, by2))
        return not (x2 < z1 or z2 < x1 or y2 < w1 or w2 < y1)

    gaps = {i: [] for i in range(len(items))}
    for i, A in enumerate(items):
        for j, B in enumerate(items):
            if i != j and crosses(A, B) and not over(A, B):
                gaps[i].append((B["a"], B["b"], B["w"] + 2 * gap))

    defs, out = [], []
    for i, it in enumerate(items):
        m = ""
        if gaps[i]:
            mid = f"r{i}"
            defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="-70" y="-70" width="240" height="240">'
                        f'<rect x="-70" y="-70" width="240" height="240" fill="#fff"/>'
                        + "".join(f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" '
                                  f'stroke="#000" stroke-width="{wd:.2f}" stroke-linecap="butt"/>'
                                  for a, b, wd in gaps[i]) + '</mask>')
            m = f'mask="url(#{mid})"'
        out.append(f'<path d="{it["path"]}" fill="none" stroke="{it["color"]}" '
                   f'stroke-width="{it["w"]:.2f}" {m}/>')
    return (f"<defs>{''.join(defs)}</defs>" if defs else "") + "".join(out)

CASES = [
    ("T1 直环 tw5.5 缝4 · ov-3 un17", build()),
    ("T2 微弓环 bow1.6 · 同向", build(ring_bow=1.6)),
    ("T3 交替弓 bow1.8 · ±±", build(ring_bow=1.8, bow_sign=(1, -1, 1, -1))),
    ("T4 粗绳 tw7 缝5", build(tw=7, sep=5, un=19)),
    ("T5 粗绳 + 中心缠环", build(tw=6.5, sep=4.6, un=19,
                                 extra=[((36, 58), (64, 58), 5.0, -9.0)])),
    ("T6 内左环染琥珀", build(tw=6.5, sep=4.6, un=19, ember_ring=2)),
    ("T7 细绳 tw4.6 缝3.6", build(tw=4.6, sep=3.6, un=15)),
    ("T8 微弓 + 缝加宽 gap3.4", build(tw=6.5, sep=4.6, un=19, gap=3.4, ring_bow=1.6)),
    ("T9 交替弓 + 琥珀环 2", build(tw=6.5, sep=4.6, un=19, ring_bow=1.8,
                                   bow_sign=(1, -1, 1, -1), ember_ring=2)),
]

ZW, WW, PAD, ROW, HEAD = 216, 268, 20, 320, 76
COLS = 3
ROWS = math.ceil(len(CASES) / COLS)
W = PAD * 2 + COLS * (ZW + 16 + WW + 34)
H = HEAD + ROWS * (ROW + 30)
ZOOM_V = (0.0, 42.0, 60.0)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       f'<rect width="{W}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="38" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — looped strand W</text>',
       f'<text x="{PAD}" y="60" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'each stroke = one closed loop of thread · interlaced over/under at the turns · left: valley zoom · right: full + 48/32/24/16</text>']
for i, (label, art) in enumerate(CASES):
    r, c = divmod(i, COLS)
    x0 = PAD + c * (ZW + 16 + WW + 34)
    y0 = HEAD + r * (ROW + 30)
    zx, zy, zs = ZOOM_V
    svg.append(f'<rect x="{x0}" y="{y0}" width="{ZW}" height="{ROW-30}" rx="12" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0},{y0}) scale({ZW/zs}) translate({-zx},{-zy})">{art}</g>')
    svg.append(f'<rect x="{x0+ZW+16}" y="{y0}" width="{WW}" height="{WW}" rx="12" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0+ZW+16},{y0}) scale({WW/100})">{art}</g>')
    sx = x0 + ZW + 16
    for s in (48, 32, 24, 16):
        svg.append(f'<g transform="translate({sx},{y0+WW+12}) scale({s/100})">{art}</g>')
        sx += s + 10
    svg.append(f'<text x="{x0}" y="{y0+ROW+2}" font-family="Inter" font-size="12" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
