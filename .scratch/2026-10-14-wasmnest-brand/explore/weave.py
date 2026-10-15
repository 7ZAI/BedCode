#!/usr/bin/env python3
"""WasmNest 标志探索 2：镂空 / 交织的 W（巢材编织）。

设计意图：v1 的 W 是「四段分离笔画」——接点处留缝，表达隔离。v2 反过来：
四段笔画在接点处**互相穿过并微微出头**，像巢里的枝条那样交织；笔画本身再拆成
多股细股，股间留空，得到镂空质感。语义：整体是编织物，但每根材料仍可被单独抽出
（插件可停用、可回收），单元之间只经编织点相交（互调 API / 总线），始终不融合。

交织的上下关系用 mask 实现：位于下方的股，在交点处被上方的股「挖掉」一条缝
（缝宽 = 上方股宽 + 2×gap），这才是真正的 weave，不是叠色。

用法：python3 weave.py > weave.svg && rsvg-convert -z 2 weave.svg -o weave.png
"""
import math

INK = "#08090B"
INK_2 = "#16181C"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"

# 100 网格上的 W 骨架（中心线顶点）。四段笔画：s1 外左↓ s2 内左↑ s3 内右↓ s4 外右↑
T1 = (10.0, 10.0)
V1 = (36.0, 90.0)
P = (50.0, 40.0)
V2 = (64.0, 90.0)
T2 = (90.0, 10.0)

def segs(pro):
    """四段笔画：每段在接点处向外多伸 pro 单位（出头 = 枝条交织的实感）。"""
    u = {}
    def unit(a, b):
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy)
        return (dx / L, dy / L)
    u12 = unit(T1, V1)
    u23 = unit(V1, P)
    u34 = unit(P, V2)
    u45 = unit(V2, T2)
    def ext(p, u, s):
        return (p[0] + u[0] * s, p[1] + u[1] * s)
    # s1 从头到尾并越过 V1；s2 越过 V1 与 P；s3 越过 P 与 V2；s4 越过 V2
    return [
        (ext(T1, u12, 0), ext(V1, u12, pro), u12),
        (ext(V1, u23, -pro), ext(P, u23, pro), u23),
        (ext(P, u34, -pro), ext(V2, u34, pro), u34),
        (ext(V2, u45, -pro), ext(T2, u45, 0), u45),
    ]

def normal(u):
    return (-u[1], u[0])

def strand(p0, p1, u, off, w):
    """把一段笔画按垂直偏移 off 平移后画成一条股。"""
    n = normal(u)
    a = (p0[0] + n[0] * off, p0[1] + n[1] * off)
    b = (p1[0] + n[0] * off, p1[1] + n[1] * off)
    return a, b, u

def band_line(a, b, w, color, extra=""):
    return (f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" '
            f'stroke="{color}" stroke-width="{w:.2f}" stroke-linecap="butt" {extra}/>')

def build(w=13.0, strands=1, slot=0.0, pro=6.0, gap=1.8, ember=None,
          pattern="rope", twigs=(), label=""):
    """生成一个 woven W。

    w       : 一段笔画的总宽度（多股时 = strands*股宽 + (strands-1)*slot）
    strands : 每段笔画拆成几条股
    slot    : 股间空隙（镂空）
    pro     : 接点出头长度
    ember   : 染色股，("seg", 段号 1..4, 股号) 或 None
    pattern : rope = 单绳交替（s1>s2>s3>s4 依次压在上方，镜像用 rope_rev）
    twigs   : 额外短枝条 [(中心点, 角度, 长度, 宽度)]，也参与交织
    """
    sw = (w - slot * (strands - 1)) / strands  # 单股宽
    offs = [-(w / 2) + sw / 2 + i * (sw + slot) for i in range(strands)]
    S = segs(pro)

    # 每段笔画的各股
    items = []  # (segidx, sidx, (a,b,u), is_twig)
    for i, (p0, p1, u) in enumerate(S, start=1):
        for j, off in enumerate(offs):
            a, b, _ = strand(p0, p1, u, off, sw)
            items.append({"seg": i, "idx": j, "a": a, "b": b, "u": u, "w": sw})
    # 额外枝条（横穿巢材，参与同样的交织规则）
    for k, (c, ang, L, wd) in enumerate(twigs):
        th = math.radians(ang)
        u = (math.cos(th), math.sin(th))
        a = (c[0] - u[0] * L / 2, c[1] - u[1] * L / 2)
        b = (c[0] + u[0] * L / 2, c[1] + u[1] * L / 2)
        items.append({"seg": 10 + k, "idx": 0, "a": a, "b": b, "u": u, "w": wd})

    # 上下关系：seg 越小越靠上（rope：s1 压 s2 压 s3 压 s4）；股号奇偶交替
    def over(A, B):
        """A 是否压在 B 上方。"""
        if A["seg"] != B["seg"]:
            if pattern == "rope":
                return A["seg"] < B["seg"]
            if pattern == "rope_rev":
                return A["seg"] > B["seg"]
        # 同段（平行）或需要交错时：股号奇偶决定
        return (A["idx"] + B["idx"]) % 2 == 0

    # 交点几何判定：两段中心线是否真的相交
    def crosses(A, B):
        if A["seg"] == B["seg"]:
            return False
        (x1, y1), (x2, y2) = A["a"], A["b"]
        (x3, y3), (x4, y4) = B["a"], B["b"]
        d = (x2 - x1) * (y4 - y3) - (y2 - y1) * (x4 - x3)
        if abs(d) < 1e-6:
            return False
        # 两段是否在彼此的纵向范围内（用无限直线求交，再用包围盒收边）
        bx1, bx2 = sorted((x1, x2)); by1, by2 = sorted((y1, y2))
        cx1, cx2 = sorted((x3, x4)); cy1, cy2 = sorted((y3, y4))
        return not (bx2 < cx1 or cx2 < bx1 or by2 < cy1 or cy2 < by1)

    masks, shapes = {}, []
    for i, it in enumerate(items):
        masks[it["seg"] * 100 + it["idx"]] = []
    for i, A in enumerate(items):
        for j, B in enumerate(items):
            if i == j or not crosses(A, B):
                continue
            # A 在下、B 在上 → A 被挖缝
            if B["seg"] < A["seg"] or (B["seg"] == A["seg"] and not over(A, B)):
                if A["seg"] != B["seg"]:
                    masks[A["seg"] * 100 + A["idx"]].append((B["a"], B["b"], B["w"] + 2 * gap))
    # 同段股之间不需要挖缝（平行）

    defs = []
    out = []
    for it in items:
        col = PAPER
        if ember and it["seg"] == ember[0] and it["idx"] == ember[1]:
            col = EMBER
        key = it["seg"] * 100 + it["idx"]
        gaps = masks[key]
        m = ""
        maskattr = ""
        if gaps:
            mid = f"m{key}"
            defs.append(f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="-40" y="-40" width="180" height="180">'
                        f'<rect x="-40" y="-40" width="180" height="180" fill="#fff"/>'
                        + "".join(band_line(a, b, wd, "#000") for a, b, wd in gaps)
                        + '</mask>')
            maskattr = f'mask="url(#{mid})"'
        out.append(band_line(it["a"], it["b"], it["w"], col, maskattr))
    svg = ""
    if defs:
        svg += "<defs>" + "".join(defs) + "</defs>"
    return svg + "".join(out)


def glyph(art, size, x, y, bg=None):
    s = size / 100
    g = f'<g transform="translate({x},{y}) scale({s})">{art}</g>'
    if bg:
        g = f'<rect x="{x}" y="{y}" width="{size}" height="{size}" rx="{size*0.055}" fill="{bg}"/>' + g
    return g

# ---------------------------------------------------------------------------
# 对照表
# ---------------------------------------------------------------------------
CASES = [
    ("W1 单股 w13 出头6 · 交替编织", build(w=13, pro=6)),
    ("W2 单股 w13 出头11 · 交替编织", build(w=13, pro=11)),
    ("W3 双股 w15 缝4 出头9", build(w=15, strands=2, slot=4.0, pro=9)),
    ("W4 双股 w17 缝5 出头11", build(w=17, strands=2, slot=5.0, pro=11)),
    ("W5 三股 w18 缝2.6 出头11", build(w=18, strands=3, slot=2.6, pro=11)),
    ("W6 单股 w13 出头6 · 反向交替", build(w=13, pro=6, pattern="rope_rev")),
    ("W7 双股 + 横枝两根", build(w=15, strands=2, slot=4.0, pro=9,
                                twigs=[((50, 66), 8, 58, 5.0), ((50, 30), -8, 40, 4.4)])),
    ("W8 双股 · 一股染琥珀", build(w=15, strands=2, slot=4.0, pro=9, ember=(4, 1))),
    ("W9 单股 w16 出头9 · 交替", build(w=16, pro=9)),
]

CELL = 236
PAD = 22
COLS = 3
ROWS = math.ceil(len(CASES) / COLS)
HEAD = 74
W = PAD * 2 + COLS * CELL + (COLS - 1) * PAD
H = HEAD + 30 + ROWS * (CELL + 30)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       f'<rect width="{W}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="40" font-family="Inter Display" font-size="26" font-weight="700" fill="{PAPER}">'
       f'WasmNest — woven W</text>',
       f'<text x="{PAD}" y="62" font-family="JetBrains Mono" font-size="11.5" fill="#7C848F">'
       f'interlaced strands · over/under gaps · 100 grid · small 48 / 32 / 20 / 16</text>']

for i, (label, art) in enumerate(CASES):
    r, c = divmod(i, COLS)
    x = PAD + c * (CELL + PAD)
    y = HEAD + 30 + r * (CELL + 30)
    svg.append(f'<rect x="{x}" y="{y}" width="{CELL}" height="{CELL}" rx="14" fill="{INK_2}"/>')
    svg.append(glyph(art, 152, x + (CELL - 152) / 2, y + 16))
    sx = x + 14
    for s in (48, 32, 20, 16):
        svg.append(glyph(art, s, sx, y + CELL - s - 14))
        sx += s + 9
    svg.append(f'<text x="{x+12}" y="{y+CELL+19}" font-family="Inter" font-size="12.5" fill="#8B939E">{label}</text>')
svg.append('</svg>')
print("\n".join(svg))
