#!/usr/bin/env python3
"""WasmNest 探索 4：交织 W 的「干净端」规则 + 双接点放大对照。

前两轮的结论（决定性）：
  * 下方股若在「上方股的挖缝带」内结束 → 端角被斜切出锯齿/尖刺（上一轮的 defects）。
    要让接点干净只有两条路：要么缩进到带外（≥17 单位，内笔太短，不成立），
    要么**穿过带出头**（≥14 单位）——这时可见端面正好是带的边界线，与上方股边缘
    平行，读作「这根枝条从下面穿过去，露出头」= 真正的交织。
  * 因此单绳交替（s1 压 s2 压 s3 压 s4）是最自然的构造：每段笔画在一端是上方股、
    另一端是下方股，整条 W 读作「一根巢材反复穿过自己」。

骨架：对称 W，宽 76 × 高 54（v1 的 53.6×34 同族比例，但把窄高的 80×80 修正过来）。
接点夹角 50°（v1 是 39°，太窄导致穿越区过长、显脏）。

用法：python3 weave3.py > weave3.svg && rsvg-convert -z 2 weave3.svg -o weave3.png
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

def ext(p, u, s):
    return (p[0] + u[0] * s, p[1] + u[1] * s)

def segments(ov, un):
    """四段笔画。ov = 作上方股时相对顶点（正=出头，负=缩进）；un = 作下方股时的伸出量。"""
    u1, u2, u3, u4 = unit(T1, V1), unit(V1, P), unit(P, V2), unit(V2, T2)
    return [
        dict(i=1, a=ext(T1, u1, 0), b=ext(V1, u1, ov), u=u1),
        dict(i=2, a=ext(V1, u2, un), b=ext(P, u2, ov), u=u2),
        dict(i=3, a=ext(P, u3, un), b=ext(V2, u3, ov), u=u3),
        dict(i=4, a=ext(V2, u4, un), b=ext(T2, u4, 0), u=u4),
    ]

def line(a, b, w, color, extra=""):
    return (f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" '
            f'stroke="{color}" stroke-width="{w:.2f}" stroke-linecap="butt" {extra}/>')

def build(w=15.0, ov=-3.0, un=16.0, gap=2.6, strands=1, slot=0.0,
          ember_seg=None, core=None, stagger=0.0):
    S = segments(ov, un)
    sw = (w - slot * (strands - 1)) / strands
    offs = [-(w / 2) + sw / 2 + k * (sw + slot) for k in range(strands)]
    items = []
    for s in S:
        for k, off in enumerate(offs):
            n = (-s["u"][1], s["u"][0])
            # stagger：同段各股沿方向错开，端部形成参差的枝条口（0 = 整齐平切）
            a = ext(s["a"] + (n[0] * off, n[1] * off), s["u"], k * stagger)
            b = ext(s["b"] + (n[0] * off, n[1] * off), s["u"], -k * stagger)
            items.append(dict(seg=s["i"], k=k, a=a, b=b, w=sw))

    def over(A, B):
        if A["seg"] != B["seg"]:
            return A["seg"] < B["seg"]
        return (A["k"] + B["k"]) % 2 == 0

    def intersects(A, B):
        if A["seg"] == B["seg"]:
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
            if i != j and intersects(A, B) and not over(A, B):
                gaps[i].append((B["a"], B["b"], B["w"] + 2 * gap))

    defs, out = [], []
    for i, it in enumerate(items):
        col = EMBER if (ember_seg and it["seg"] == ember_seg) else PAPER
        m = ""
        if gaps[i]:
            mid = f"k{i}"
            defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="-60" y="-60" width="220" height="220">'
                        f'<rect x="-60" y="-60" width="220" height="220" fill="#fff"/>'
                        + "".join(line(a, b, wd, "#000") for a, b, wd in gaps[i]) + '</mask>')
            m = f'mask="url(#{mid})"'
        out.append(line(it["a"], it["b"], it["w"], col, m))
    tail = ""
    if core:
        cx, cy, h = core
        tail = f'<path d="M{cx},{cy-h} L{cx+h},{cy} L{cx},{cy+h} L{cx-h},{cy} Z" fill="{EMBER}"/>'
    return (f"<defs>{''.join(defs)}</defs>" if defs else "") + "".join(out) + tail

CASES = [
    ("S1 单股 w15 · ov-3 un16 · gap2.6", build()),
    ("S2 单股 w15 · ov0 un19 · gap3.2", build(ov=0, un=19, gap=3.2)),
    ("S3 单股 w16 · ov6 un20 · gap2.6（枝头外露）", build(w=16, ov=6, un=20, gap=2.6)),
    ("S4 双股 w20（7/6/7）· ov-3 un17", build(w=20, strands=2, slot=6, ov=-3, un=17)),
    ("S5 双股 w20 · 参差 4 · ov-3 un17", build(w=20, strands=2, slot=6, ov=-3, un=17, stagger=4)),
    ("S6 双股 w20 · 左谷股染琥珀", build(w=20, strands=2, slot=6, ov=-3, un=17, ember_seg=2)),
    ("S7 单股 w15 + 中心琥珀菱形", build(core=(50, 43, 8))),
    ("S8 双股 w24（8.5/7/8.5）· ov-4 un20", build(w=24, strands=2, slot=7, ov=-4, un=20)),
]

ZOOM_V = (0.0, 46.0, 58.0)   # 左谷接点窗口 (x, y, size)
ZOOM_P = (24.0, 14.0, 58.0)  # 中央峰接点窗口
ZW = 214
WW = 248
PAD = 20
ROW = 268
HEAD = 76
COLS = 3
ROWS = math.ceil(len(CASES) / COLS)
W = PAD * 2 + COLS * (ZW * 2 + 16 + WW + 30)
H = HEAD + ROWS * (ROW + 34)

def zoom_cell(x, y, art, win):
    zx, zy, zs = win
    return (f'<rect x="{x}" y="{y}" width="{ZW}" height="{ZW}" rx="12" fill="{INK_2}"/>'
            f'<g transform="translate({x},{y}) scale({ZW/zs}) translate({-zx},{-zy})">{art}</g>')

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       f'<rect width="{W}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="38" font-family="Inter Display" font-size="25" font-weight="700" fill="{PAPER}">'
       f'WasmNest — weave &amp; clean-end study</text>',
       f'<text x="{PAD}" y="60" font-family="JetBrains Mono" font-size="11" fill="#7C848F">'
       f'each row: left-valley zoom · center-peak zoom · whole W · 100 grid, W = 76 × 54</text>']
for i, (label, art) in enumerate(CASES):
    r, c = divmod(i, COLS)
    x0 = PAD + c * (ZW * 2 + 16 + WW + 30)
    y0 = HEAD + r * (ROW + 34)
    svg.append(zoom_cell(x0, y0, art, ZOOM_V))
    svg.append(zoom_cell(x0 + ZW + 16, y0, art, ZOOM_P))
    svg.append(f'<rect x="{x0+ZW*2+32}" y="{y0}" width="{WW}" height="{WW}" rx="12" fill="{INK_2}"/>')
    svg.append(f'<g transform="translate({x0+ZW*2+32},{y0}) scale({WW/100})">{art}</g>')
    svg.append(f'<text x="{x0}" y="{y0+ROW+16}" font-family="Inter" font-size="12" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
