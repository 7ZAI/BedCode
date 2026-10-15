#!/usr/bin/env python3
"""WasmNest 探索 3：接点研究（junction study）。

上一轮暴露两个问题：
  1) 骨架太窄太高（80×80）→ 四笔都接近 70°，读作「几根斜杠」而不是 W。改为
     宽大于高的 W（宽 76 / 高 52，与 v1 的 53.6×34 同族比例），笔画间夹角从
     39° 放宽到 49°，穿越区更短更干净。
  2) 接点被 mask「啃出锯齿」：上方股的挖缝带是沿其中心线的无限长条，下方股若只
     微微出头（pro 处于 4..10 的中间区），端角会一半在带外、一半在带内 → 缺口呈
     锯齿。要么不出头（端面平切，被上方股压住 = 绑扎态），要么出头足够远
     （≥12，端部完全越过带，成为真正的 X 交叉 = 枝条出头态）。本次把两端做成
     可分别控制的 stub（上方股出头）/ short（下方股缩进），扫一遍看哪种接点最干净。

用法：python3 weave2.py > weave2.svg && rsvg-convert -z 2 weave2.svg -o weave2.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"

# 宽大于高的 W 骨架（中心线顶点），比例贴近 v1：宽 76 / 高 52
T1 = (12.0, 24.0)
V1 = (35.0, 76.0)
P = (50.0, 44.0)
V2 = (65.0, 76.0)
T2 = (88.0, 24.0)

def unit(a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    L = math.hypot(dx, dy)
    return (dx / L, dy / L)

def ext(p, u, s):
    return (p[0] + u[0] * s, p[1] + u[1] * s)

def segments(stub=0.0, short=0.0):
    """四段笔画。接点处上方股出头 stub，下方股缩进 short（相对顶点）。"""
    u1, u2, u3, u4 = unit(T1, V1), unit(V1, P), unit(P, V2), unit(V2, T2)
    # 沿自身方向的伸出量：起端 / 末端
    # s1: 起 T1，末 V1 —— 在 V1 处 s1 压 s2（rope），故 s1 出头 stub，s2 缩进 short
    return [
        dict(i=1, a=ext(T1, u1, 0), b=ext(V1, u1, stub), u=u1),
        dict(i=2, a=ext(V1, u2, -short), b=ext(P, u2, short), u=u2),
        dict(i=3, a=ext(P, u3, -short), b=ext(V2, u3, stub), u=u3),
        dict(i=4, a=ext(V2, u4, -short), b=ext(T2, u4, 0), u=u4),
    ]

def line(a, b, w, color, extra=""):
    return (f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" '
            f'stroke="{color}" stroke-width="{w:.2f}" stroke-linecap="butt" {extra}/>')

def build(w=15.0, stub=0.0, short=0.0, gap=2.5, ember_seg=None, ember_diamond=None,
          strands=1, slot=0.0, twigs=()):
    S = segments(stub, short)
    sw = (w - slot * (strands - 1)) / strands
    offs = [-(w / 2) + sw / 2 + k * (sw + slot) for k in range(strands)]
    items = []
    for s in S:
        for k, off in enumerate(offs):
            n = (-s["u"][1], s["u"][0])
            a = (s["a"][0] + n[0] * off, s["a"][1] + n[1] * off)
            b = (s["b"][0] + n[0] * off, s["b"][1] + n[1] * off)
            items.append(dict(seg=s["i"], k=k, a=a, b=b, w=sw))
    for t, (c, ang, L, wd) in enumerate(twigs):
        th = math.radians(ang)
        u = (math.cos(th), math.sin(th))
        items.append(dict(seg=90 + t, k=0, a=ext(c, u, -L / 2), b=ext(c, u, L / 2), w=wd))

    # 上下关系：段号小者在上（绳的交替感），同段内按股号奇偶
    def over(A, B):
        if A["seg"] != B["seg"]:
            return A["seg"] < B["seg"]
        return (A["k"] + B["k"]) % 2 == 0

    def intersects(A, B):
        if A["seg"] == B["seg"]:
            return False
        ax1, ay1, ax2, ay2 = *A["a"], *A["b"]
        bx1, by1, bx2, by2 = *B["a"], *B["b"]
        d = (ax2 - ax1) * (by2 - by1) - (ay2 - ay1) * (bx2 - bx1)
        if abs(d) < 1e-6:
            return False
        bx1_, bx2_ = sorted((ax1, ax2)); by1_, by2_ = sorted((ay1, ay2))
        cx1_, cx2_ = sorted((bx1, bx2)); cy1_, cy2_ = sorted((by1, by2))
        return not (bx2_ < cx1_ or cx2_ < bx1_ or by2_ < cy1_ or cy2_ < by1_)

    gaps = {i: [] for i in range(len(items))}
    for i, A in enumerate(items):
        for j, B in enumerate(items):
            if i == j or not intersects(A, B):
                continue
            if not over(A, B):
                gaps[i].append((B["a"], B["b"], B["w"] + 2 * gap))

    defs, out = [], []
    for i, it in enumerate(items):
        col = EMBER if (ember_seg and it["seg"] == ember_seg) else PAPER
        m = ""
        if gaps[i]:
            mid = f"mm{i}"
            defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="-60" y="-60" width="220" height="220">'
                        f'<rect x="-60" y="-60" width="220" height="220" fill="#fff"/>'
                        + "".join(line(a, b, wd, "#000") for a, b, wd in gaps[i]) + '</mask>')
            m = f'mask="url(#{mid})"'
        out.append(line(it["a"], it["b"], it["w"], col, m))
    head = f"<defs>{''.join(defs)}</defs>" if defs else ""
    tail = ""
    if ember_diamond:
        cx, cy, h = ember_diamond
        tail = f'<path d="M{cx},{cy-h} L{cx+h},{cy} L{cx},{cy+h} L{cx-h},{cy} Z" fill="{EMBER}"/>'
    return head + "".join(out) + tail

# ---------------------------------------------------------------------------
# 一、接点研究：同一 W，扫描 stub / short / gap
# ---------------------------------------------------------------------------
JUNCTIONS = [
    ("A stub0 short0 gap2.5（平切绑扎）", build(stub=0, short=0, gap=2.5)),
    ("B stub0 short4 gap2.5（下方缩进）", build(stub=0, short=4, gap=2.5)),
    ("C stub3 short3 gap2.5", build(stub=3, short=3, gap=2.5)),
    ("D stub6 short6 gap3", build(stub=6, short=6, gap=3)),
    ("E stub12 short7 gap2.5（真交叉）", build(stub=12, short=7, gap=2.5)),
    ("F stub16 short9 gap2.5", build(stub=16, short=9, gap=2.5)),
    ("G 双股 w22 缝7 stub0 short4", build(w=22, strands=2, slot=7, stub=0, short=4, gap=3)),
    ("H 双股 w22 缝7 stub8 short6", build(w=22, strands=2, slot=7, stub=8, short=6, gap=3)),
    ("I 单股 w15 stub0 short0 gap4（宽缝）", build(stub=0, short=0, gap=4)),
]

# 放大区：左谷接点附近
ZOOM = "20 30 60 70"

CELL = 330
ZD = 260
PAD = 24
COLS = 3
ROWS = math.ceil(len(JUNCTIONS) / COLS)
HEAD = 78
W = PAD * 2 + COLS * CELL + (COLS - 1) * PAD
H = HEAD + ROWS * (ZD + CELL + 40)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       f'<rect width="{W}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="26" font-weight="700" fill="{PAPER}">'
       f'WasmNest — junction study</text>',
       f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11.5" fill="#7C848F">'
       f'top: whole W (330px) · bottom: 3.6× zoom on the left valley junction · over/under gap</text>']

for i, (label, art) in enumerate(JUNCTIONS):
    r, c = divmod(i, COLS)
    x = PAD + c * (CELL + PAD)
    y = HEAD + r * (ZD + CELL + 40)
    # 放大格
    svg.append(f'<rect x="{x}" y="{y}" width="{ZD}" height="{ZD}" rx="12" fill="{INK_2}"/>')
    zx, zy, zw, zh = [float(v) for v in ZOOM.split()]
    sx, sy = ZD / zw, ZD / zh
    svg.append(f'<g transform="translate({x},{y}) scale({sx},{sy}) translate({-zx},{-zy})">{art}</g>')
    # 整标格
    bx = x + ZD + 22
    svg.append(f'<rect x="{bx}" y="{y}" width="{CELL}" height="{CELL}" rx="12" fill="{INK_2}"/>')
    sc = 300 / 100
    svg.append(f'<g transform="translate({bx + (CELL-300)/2},{y + (CELL-300)/2}) scale({sc})">{art}</g>')
    svg.append(f'<text x="{x+10}" y="{y+CELL+22}" font-family="Inter" font-size="12.5" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
