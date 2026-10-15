#!/usr/bin/env python3
"""WasmNest 标志概念探索表（一次性工具，产出概念对照图供人眼裁决）。

所有候选都画在统一的 100×100 网格上，同一描边宽度，便于横向比较轮廓与密度。
探针目的只有三个：
  1. 「巢」读不读得出来（单一容器 vs 多股编织 vs 多单元）
  2. 「隔离单元 + 一个活跃核心」这条品牌论点能不能同时成立
  3. 16px 下还剩什么

用法：python3 explore.py > explore.svg && rsvg-convert -z 2 explore.svg -o explore.png
"""
import math

INK = "#08090B"
PAPER = "#F5F7F9"
EMBER = "#FF9E2C"
INK_2 = "#16181C"

def bowl(x1, x2, rim_top, wall_drop, ry, sw, color=PAPER):
    """U 形巢：两段竖壁 + 半椭圆底，端面水平平切。坐标为中心线。"""
    rx = (x2 - x1) / 2
    sh = rim_top + wall_drop
    d = f"M{x1},{rim_top} L{x1},{sh} A{rx},{ry} 0 0 0 {x2},{sh} L{x2},{rim_top}"
    return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{sw}" stroke-linecap="butt"/>'

def arc_dn(x1, x2, y, ry, sw, color=PAPER):
    """单条向下弧（无竖壁）：从 (x1,y) 到 (x2,y)，最低点 y+ry。"""
    rx = (x2 - x1) / 2
    return (f'<path d="M{x1},{y} A{rx},{ry} 0 0 0 {x2},{y}" fill="none" '
            f'stroke="{color}" stroke-width="{sw}" stroke-linecap="butt"/>')

def diamond(cx, cy, h, color=EMBER):
    return f'<path d="M{cx},{cy-h} L{cx+h},{cy} L{cx},{cy+h} L{cx-h},{cy} Z" fill="{color}"/>'

def rsq(cx, cy, h, r, color=EMBER):
    return (f'<rect x="{cx-h}" y="{cy-h}" width="{2*h}" height="{2*h}" rx="{r}" '
            f'fill="none" stroke="{color}" stroke-width="2.2"/>')

def rrect(cx, cy, hx, hy, r, sw=11, color=PAPER):
    return (f'<rect x="{cx-hx}" y="{cy-hy}" width="{2*hx}" height="{2*hy}" rx="{r}" '
            f'fill="none" stroke="{color}" stroke-width="{sw}"/>')

def v1_w(sw=13):
    segs = [(23.2, 33, 36.8, 67), (39.3, 67.1, 48.7, 46.9),
            (51.3, 46.9, 60.7, 67.1), (63.2, 67, 76.8, 33)]
    ls = "".join(f'<line x1="{a}" y1="{b}" x2="{c}" y2="{d}"/>' for a, b, c, d in segs)
    return (f'<g stroke="{PAPER}" stroke-width="{sw}" stroke-linecap="butt" fill="none">{ls}</g>'
            + diamond(50, 43, 6.5))

# ---------------------------------------------------------------------------
# 候选定义：每个返回一段 SVG（画在 0..100 的坐标系里）
# ---------------------------------------------------------------------------
SW = 11

def cand_A():
    """单槽三子：一个宽口 U 巢 + 三个横向并置的菱形单元（中者为活跃）。"""
    s = bowl(12, 88, 28, 14, 32, SW)
    s += diamond(30, 52, 7.5, PAPER) + diamond(50, 57, 7.5, EMBER) + diamond(70, 52, 7.5, PAPER)
    return s

def cand_B():
    """三子曲列：单元沿巢内壁的曲线坐（两侧高、中间低），更贴「巢」。"""
    s = bowl(12, 88, 26, 12, 34, SW)
    s += diamond(28, 48, 7.5, PAPER) + diamond(50, 58, 7.5, EMBER) + diamond(72, 48, 7.5, PAPER)
    return s

def cand_C():
    """双股巢：外巢 + 内圈短弧（编织感），三单元坐内弧上。"""
    s = bowl(12, 88, 26, 12, 34, SW)
    s += arc_dn(30, 70, 44, 16, 9)
    s += diamond(50, 56, 7, EMBER)
    return s

def cand_C2():
    """双股巢（三单元版）：内弧抬高，单元嵌在内外两股之间。"""
    s = bowl(12, 88, 26, 10, 36, SW)
    s += arc_dn(28, 72, 40, 14, 9)
    s += diamond(32, 50, 7, PAPER) + diamond(50, 55, 7, EMBER) + diamond(68, 50, 7, PAPER)
    return s

def cand_D():
    """三股巢 + 单核：三级嵌套边界，核心在最内层——「嵌套」的字面表达。"""
    s = bowl(10, 90, 22, 12, 38, SW)
    s += bowl(24, 76, 32, 10, 26, 9)
    s += arc_dn(38, 62, 44, 14, 8)
    s += diamond(50, 58, 6.5, EMBER)
    return s

def cand_E():
    """双柱托巢：两根竖柱（N 的两笔）+ 其间悬一枚巢弧，核心居中。"""
    s = f'<g stroke="{PAPER}" stroke-width="{SW}" stroke-linecap="butt" fill="none">'
    s += '<line x1="20" y1="24" x2="20" y2="76"/><line x1="80" y1="24" x2="80" y2="76"/>'
    s += '</g>'
    s += arc_dn(31, 69, 52, 20, 10)
    s += diamond(50, 52, 7, EMBER)
    return s

def cand_F():
    """W 巢：接续 v1 的四段 W（Wasm），其下承一枚巢弧与三个单元。"""
    segs = [(22.6, 26, 32.4, 52), (34.8, 52, 41.2, 38.6),
            (43.8, 38.6, 50.2, 52), (52.6, 52, 62.4, 26)]
    ls = "".join(f'<line x1="{a}" y1="{b}" x2="{c}" y2="{d}"/>' for a, b, c, d in segs)
    s = f'<g stroke="{PAPER}" stroke-width="9" stroke-linecap="butt" fill="none">{ls}</g>'
    s += arc_dn(18, 82, 62, 22, 9)
    s += diamond(50, 41, 4.5, EMBER)
    return s

def cand_G():
    """框巢：两层圆角方框（各留闸门缺口）+ 中心核心——嵌套边界的极简版。"""
    s = f'<g fill="none" stroke="{PAPER}" stroke-linecap="butt">'
    s += f'<rect x="12" y="12" width="76" height="76" rx="20" stroke-width="{SW}"/>'
    s += '</g>'
    # 内层：旋转 45° 的方框（嵌套 + 方向差）
    s += f'<rect x="31" y="31" width="38" height="38" rx="6" stroke="{PAPER}" stroke-width="9" fill="none" transform="rotate(45 50 50)"/>'
    s += diamond(50, 50, 7, EMBER)
    return s

def cand_H():
    """二子：巢 + 两枚单元（一活跃一静默），最大留白。"""
    s = bowl(14, 86, 30, 14, 30, SW)
    s += diamond(40, 54, 8.5, PAPER) + diamond(62, 54, 8.5, EMBER)
    return s

def cand_I():
    """深槽独核：只有一枚巢弧与一枚核心——最简，检验「容器 + 内容」能否独立成标。"""
    s = bowl(14, 86, 24, 16, 32, SW)
    s += diamond(50, 56, 9, EMBER)
    return s

CANDS = [
    ("V1 旧标基线", v1_w()),
    ("A 单槽三子", cand_A()),
    ("B 三子曲列", cand_B()),
    ("C2 双股 · 三子", cand_C2()),
    ("H 巢 + 二子", cand_H()),
    ("I 深槽独核", cand_I()),
    ("D 三股巢 · 独核", cand_D()),
    ("E 双柱托巢", cand_E()),
    ("G 框巢", cand_G()),
    ("F W 巢", cand_F()),
    ("C 双股 · 独核", cand_C()),
]

CELL = 236
COLS = 4
ROWS = math.ceil(len(CANDS) / COLS)
PAD = 22
HEAD = 76
W = PAD * 2 + COLS * CELL + (COLS - 1) * PAD
H = HEAD + PAD + ROWS * (CELL + 26)

def cell(x, y, label, art, big=150, small=(34, 20, 16)):
    """一格：大图 + 三档小尺寸（检验降级）+ 标签。"""
    out = [f'<g transform="translate({x},{y})">',
           f'<rect x="0" y="0" width="{CELL}" height="{CELL}" rx="14" fill="{INK_2}"/>']
    out.append(f'<g transform="translate({(CELL-big)/2},{(CELL-big)/2 - 4}) scale({big/100})">{art}</g>')
    sx = 14
    for s in small:
        out.append(f'<g transform="translate({sx},{CELL-s-14}) scale({s/100})">{art}</g>')
        sx += s + 8
    out.append(f'<text x="12" y="{CELL+18}" font-family="Inter" font-size="12.5" '
               f'fill="#8B939E">{label}</text>')
    out.append('</g>')
    return "".join(out)

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       f'<rect width="{W}" height="{H}" fill="{INK}"/>',
       f'<text x="{PAD}" y="42" font-family="Inter Display" font-size="26" font-weight="700" '
       f'fill="{PAPER}">WasmNest — mark exploration</text>',
       f'<text x="{PAD}" y="64" font-family="JetBrains Mono" font-size="11.5" '
       f'fill="#7C848F">100 grid · stroke 11 · paper #F5F7F9 · ember #FF9E2C · small sizes 34/20/16</text>']
for i, (label, art) in enumerate(CANDS):
    r, c = divmod(i, COLS)
    svg.append(cell(PAD + c * (CELL + PAD), HEAD + r * (CELL + 26), label, art))
svg.append('</svg>')
print("\n".join(svg))
