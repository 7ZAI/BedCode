#!/usr/bin/env python3
"""WasmNest 品牌规范板（3×3）：brandkit 主产物，纯矢量代码渲染。

为什么不用图像模型：v1 的板子因为缺 API key 一直没出；更要紧的是板上的标志、字标、
色值必须是**真源资产本身**（改标志时板子跟着变），而不是模型重画一遍。
本文件只做排版：所有图形都从 assets/ 的真源 SVG（由 build-assets.py 生成）取。

坐标约定：每个面板的内容函数拿到自己的左上角 (px, py)，直接算绝对坐标——
SVG 属性不认算术表达式（`x="64 + 46"` 会被渲染器当 0），所以这里一律算好数字再写。

用法：python3 brand-board.py > board.svg && rsvg-convert -w 2400 board.svg -o board.png
"""
import importlib.util
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("build_assets", HERE / "build-assets.py")
A = importlib.util.module_from_spec(_spec)      # 真源模块（有 __main__ 守卫，不会跑 main）
_spec.loader.exec_module(A)

INK, INK_2 = A.INK, A.INK_2
INK_3 = "#101216"          # 终端块底色（面板内自用，未进品牌色阶）
SURFACE, BORDER = A.SURFACE, A.BORDER
EMBER, EMBER_700, PAPER, FOG, CANVAS = A.EMBER, A.EMBER_700, A.PAPER, A.FOG, A.CANVAS

W = H = 2400
M, GUT = 64, 32
PW = PH = (W - 2 * M - 2 * GUT) / 3
DISP, SANS, MONO = "Inter Display", "Inter", "JetBrains Mono"

_bb = A.mark_bbox()
MB = dict(x0=_bb[0], y0=_bb[1], w=_bb[2] - _bb[0], h=_bb[3] - _bb[1])
panels = []


def txt(x, y, s, size=15, color=PAPER, font=SANS, weight=400, anchor="start", ls=None):
    lsp = f' letter-spacing="{ls}"' if ls else ""
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="{font}" font-size="{size:g}" '
            f'font-weight="{weight}" fill="{color}" text-anchor="{anchor}"{lsp}>{s}</text>')


def rect(x, y, w, h, fill, rx=0, extra=""):
    return f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" fill="{fill}"{extra}/>'


def line(x1, y1, x2, y2, color=BORDER, w=1):
    return f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" stroke="{color}" stroke-width="{w}"/>'


def place(name, x, y, w, h=None):
    """把 assets/<name> 以嵌套 <svg> 放进面板（真源资产直接引用，不重画）。"""
    txt_svg = (HERE / "assets" / name).read_text().strip()
    vb = re.search(r'viewBox="([^"]+)"', txt_svg).group(1)
    body = re.sub(r"^<\?xml[^>]*\?>", "", txt_svg)
    body = re.sub(r"^<svg[^>]*>", "", body)
    body = re.sub(r"</svg>\s*$", "", body)
    return (f'<svg x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{(h or w):.1f}" '
            f'viewBox="{vb}" preserveAspectRatio="xMidYMid meet">{body}</svg>')


def mark(x, y, w, solid=False):
    return place("wasmnest-mark-solid.svg" if solid else "wasmnest-mark.svg",
                 x, y, w, w * MB["h"] / MB["w"])


def panel(col, row, label, idx, build):
    x, y = M + col * (PW + GUT), M + row * (PH + GUT)
    body = "".join([
        rect(x, y, PW, PH, INK_2, 18, f' stroke="{BORDER}" stroke-width="1"'),
        txt(x + 28, y + 40, label.upper(), 13, FOG, MONO, 500, ls="0.10em"),
        txt(x + PW - 30, y + PH - 26, str(idx), 13, "#5C646E", MONO, 400, "end"),
        build(x, y)])
    panels.append(body)


# ============================================================================
# 1 封面
# ============================================================================
def p_cover(px, py):
    mw = 456.0
    mh = mw * MB["h"] / MB["w"]
    top = (PH - mh - 260) / 2
    return "".join([
        mark(px + (PW - mw) / 2, py + top, mw),
        place("wasmnest-lockup.svg", px + (PW - 520) / 2, py + top + mh + 54, 520, 156),
        txt(px + PW / 2, py + top + mh + 300, "One nest. Many threads.", 26, FOG, SANS, 400, "middle"),
        txt(px + PW / 2, py + top + mh + 338,
            "wasmtime 48 component host · sandboxed apps · capability gates",
            13.5, "#6B7280", MONO, 400, "middle"),
        txt(px + PW / 2, py + PH - 52, "WASMNEST", 13, "#4A525C", MONO, 500, "middle", ls="0.42em"),
    ])


# ============================================================================
# 2 构造
# ============================================================================
def p_construction(px, py):
    tw = 372.0
    s = tw / MB["w"]
    body = [f'<g transform="translate({px + 44:.1f},{py + 108:.1f}) scale({s:.5f}) '
            f'translate({-MB["x0"]:.2f},{-MB["y0"]:.2f})">',
            f'<g opacity="0.34">{A.mark_svg(PAPER, EMBER)}</g>',
            '<polyline points="' + " ".join(f"{vx:.1f},{vy:.1f}" for vx, vy in A.VERTS) + '" '
            f'fill="none" stroke="{EMBER}" stroke-width="{2.2 / s:.2f}" '
            f'stroke-dasharray="{7 / s:.1f} {5 / s:.1f}"/>']
    for vx, vy in A.VERTS:
        body.append(f'<circle cx="{vx:.1f}" cy="{vy:.1f}" r="{4.2 / s:.2f}" fill="{INK}" '
                    f'stroke="{EMBER}" stroke-width="{2 / s:.2f}"/>')
    body.append('</g>')
    notes = [("① 骨架", "W 的四个顶点：段长与夹角由此固定"),
             ("② 四根棒料", "沿骨架构件化；斜接缝 2.2 = 隔离边界"),
             ("③ 股线", "每根棒料 3 右手 + 3 左手螺旋（节距 26）"),
             ("④ 压 / 被压", "交叉处按 3D 深度 sinθ 判定，后股被压断"),
             ("⑤ 端头缠扎", "两端 2 圈收紧 + 收束到中轴 = 扎紧的绳头")]
    for i, (k, v) in enumerate(notes):
        yy = py + 300 + i * 54
        body.append(txt(px + 392, yy, k, 16, PAPER, SANS, 600))
        body.append(txt(px + 486, yy, v, 13.5, FOG))
    body.append(line(px + 44, py + 542, px + PW - 44, py + 542))
    left = 46.0
    for size in (96, 64, 48, 32, 24, 16):
        mh = size * MB["h"] / MB["w"]
        body.append(mark(px + left, py + 566 + (58 - mh) / 2, size, solid=size <= 32))
        body.append(txt(px + left, py + 648, str(size), 12, "#5C646E", MONO))
        left += size + 26
    body.append(txt(px + 44, py + 682, "≥48px 编织态　·　≤32px 实心态（轮廓相同，只是纹理并实）", 13.5, FOG))
    return "".join(body)


# ============================================================================
# 3 标志变体 + 留白
# ============================================================================
def p_variants(px, py):
    body = []
    items = [("wasmnest-mark.svg", "primary", "深底主用（琥珀棒 = 活跃实例）", False),
             ("wasmnest-mark-mono.svg", "mono", "单色纸白；深底 / 琥珀底上用", False),
             ("wasmnest-mark-ink.svg", "ink", "浅底反相；琥珀转 ember-700", True),
             ("wasmnest-mark-solid.svg", "solid", "≤32px 降级态", False)]
    for i, (f, name, note, light) in enumerate(items):
        cx = px + 40 + (i % 2) * 336
        cy = py + 90 + (i // 2) * 244
        body += [rect(cx, cy, 300, 170, CANVAS if light else INK, 12, f' stroke="{BORDER}"'),
                 place(f, cx, cy, 300, 170),
                 txt(cx, cy + 196, name.upper(), 13.5, PAPER, MONO, 500, ls="0.08em"),
                 txt(cx, cy + 218, note, 13.5, FOG)]
    mw = 190.0
    mh = mw * MB["h"] / MB["w"]
    cx = px + 44
    cy = py + 520
    body += [rect(cx - 26, cy - 20, mw + 52, mh + 40, INK, 10, f' stroke="{BORDER}"'),
             f'<rect x="{cx:.1f}" y="{cy:.1f}" width="{mw:.1f}" height="{mh:.1f}" '
             f'fill="none" stroke="{EMBER}" stroke-width="1" stroke-dasharray="5 4"/>',
             mark(cx, cy, mw),
             txt(cx + mw + 40, cy + 84, "留白 ≥ 棒料半径 R（7.8 单位）", 13.5, PAPER, SANS, 600),
             txt(cx + mw + 40, cy + 106, "标距 ≥ ½ R；锁定组合里标距 = ½ 标识高。", 13.5, FOG),
             txt(cx - 26, py + PH - 18, "四态共用同一骨架：换档只改纹理，不改轮廓。", 12.5, "#6B7280")]
    return "".join(body)


# ============================================================================
# 4 色彩
# ============================================================================
SW = [(INK, "ink", "#08090B", "主深底"), (INK_2, "ink-2", "#16181C", "面板 / 渐变上端"),
      (SURFACE, "surface", "#121418", "卡片面"), (BORDER, "border", "#242930", "分隔线"),
      (EMBER, "ember", "#FF9E2C", "品牌琥珀 · 唯一强调色"),
      (EMBER_700, "ember-700", "#9F621B", "浅底上的琥珀"),
      (PAPER, "paper", "#F5F7F9", "主前景 / 股线"), (FOG, "fog", "#9AA3AE", "次级文字"),
      (CANVAS, "canvas", "#F4F6F8", "浅色底")]


def p_palette(px, py):
    body = []
    for i, (hexv, name, hexs, note) in enumerate(SW):
        col, row = i % 3, i // 3
        cx, cy = px + 40 + col * 226, py + 96 + row * 180
        fg = INK if hexv in (EMBER, EMBER_700, PAPER, FOG, CANVAS) else PAPER
        body += [rect(cx, cy, 200, 96, hexv, 10, f' stroke="{BORDER}"'),
                 txt(cx + 14, cy + 82, hexs, 14, fg, MONO, 500),
                 txt(cx, cy + 122, name, 14.5, PAPER, MONO, 600),
                 txt(cx, cy + 144, note, 13, FOG)]
    body += [line(px + 40, py + 660, px + PW - 40, py + 660),
             txt(px + 40, py + 690, "配比纪律：一档深底 + 一个强调色；琥珀全系统只允许一处。", 13.5, FOG)]
    return "".join(body)


# ============================================================================
# 5 字体
# ============================================================================
def p_type(px, py):
    return "".join([
        txt(px + 40, py + 122, "Inter Display", 32, PAPER, DISP, 700),
        txt(px + 40, py + 150, "字标 / 标题 · SemiBold–Bold · 字距 −0.03em", 13.5, FOG),
        txt(px + 40, py + 228, "Inter", 26, PAPER, SANS, 600),
        txt(px + 40, py + 254, "界面正文 · Regular 400 / Medium 500", 13.5, FOG),
        txt(px + 40, py + 332, "JetBrains Mono", 23, PAPER, MONO, 700),
        txt(px + 40, py + 358, "capability / ABI / 组件 ID / 终端", 13.5, FOG),
        line(px + 40, py + 392, px + PW - 40, py + 392),
        txt(px + 40, py + 440, "wasm:capability/pty:spawn", 17, EMBER, MONO),
        txt(px + 40, py + 474, "ABI v19 · wasmtime 48 · wasm32-wasip3", 17, PAPER, MONO),
        txt(px + 40, py + 508, "$ wasmnest run --plugin terminal-session", 17, FOG, MONO),
        line(px + 40, py + 552, px + PW - 40, py + 552),
        txt(px + 40, py + 596, "三级分工：Display 命题 · Sans 解释 · Mono 精确。", 14.5, FOG),
        txt(px + 40, py + 622, "不用衬线体与手写体；字标不做字距扩张。", 14.5, FOG),
    ])


# ============================================================================
# 6 应用：图标 + favicon
# ============================================================================
def p_icons(px, py):
    body = [place(f, px + 40 + i * 168, py + 90, 148, 148) for i, f in
            enumerate(["wasmnest-icon.svg", "wasmnest-icon-light.svg",
                       "wasmnest-icon-mono.svg", "wasmnest-icon-simple.svg"])]
    body += [rect(px + 40, py + 268, PW - 80, 392, INK, 12, f' stroke="{BORDER}"'),
             rect(px + 40, py + 268, PW - 80, 44, INK_2, 12)]
    body += [f'<circle cx="{px + 66 + i * 18:.1f}" cy="{py + 290:.1f}" r="5" fill="#3A4048"/>'
             for i in range(3)]
    body += [place("wasmnest-favicon.svg", px + 122, py + 279, 22, 22),
             rect(px + 156, py + 277, PW - 236, 26, SURFACE, 13),
             txt(px + 172, py + 295, "wasmnest.dev", 14, FOG, MONO),
             mark(px + (PW - 300) / 2, py + 348, 300),
             txt(px + 40, py + 688, "16px favicon 与 ≤64px 图标用实心态；≥128px 用编织态。", 13.5, FOG),
             txt(px + 40, py + 712,
                 "圆角 22.5%；单色版 = 琥珀底 + ink 标志；另交付 Android 三层与 .ico / .icns。",
                 13.5, "#6B7280")]
    return "".join(body)


# ============================================================================
# 7 应用：宿主窗口
# ============================================================================
def p_ui(px, py):
    body = [rect(px + 40, py + 90, PW - 80, 404, INK, 14, f' stroke="{BORDER}"'),
            rect(px + 40, py + 90, PW - 80, 46, INK_2, 14),
            mark(px + 58, py + 100, 34),
            txt(px + 108, py + 121, "WasmNest", 16, PAPER, DISP, 700),
            rect(px + 40, py + 136, 186, 358, SURFACE)]
    for i, (name, active) in enumerate([("terminal-session", True), ("file-transfer", False),
                                        ("agent-hub", False), ("ai-chatbox", False)]):
        ry = py + 160 + i * 46
        if active:
            body += [rect(px + 52, ry - 16, 162, 34, "rgba(255,158,44,0.12)", 7),
                     rect(px + 52, ry - 16, 2, 34, EMBER)]
        body.append(txt(px + 66, ry + 6, name, 13, EMBER if active else FOG, MONO))
    for i, (k, v) in enumerate([("host", "wasmtime 48 · 4 instances"),
                                ("pty:spawn", "granted → terminal-session"),
                                ("net:http", "denied → ai-chatbox"),
                                ("bus", "12 msg/s · 0 dropped")]):
        yy = py + 192 + i * 34
        body += [txt(px + 250, yy, k, 14, PAPER, MONO, 500),
                 txt(px + 420, yy, v, 14, FOG, MONO)]
    body.append(rect(px + 238, py + 332, PW - 296, 148, INK_3, 10, f' stroke="{BORDER}"'))
    for i, s in enumerate(["$ wasmnest status", "nest   4 apps · abi v19",
                           "ember  1 active instance"]):
        body.append(txt(px + 256, py + 368 + i * 30, s, 13.5, PAPER if i == 0 else FOG, MONO))
    body += [txt(px + 40, py + 540, "界面里琥珀只出现在一处：当前选中项。", 13.5, FOG),
             txt(px + 40, py + 564, "标题栏用 96px 宽的小尺寸标志（实心态），不做图标化改造。", 13.5, FOG)]
    return "".join(body)


# ============================================================================
# 8 应用：移动端 + 底纹
# ============================================================================
def p_mobile(px, py):
    body = [rect(px + 40, py + 100, 228, 430, INK, 30, f' stroke="{BORDER}"'),
            place("wasmnest-pattern.svg", px + 56, py + 116, 196, 398),
            rect(px + 56, py + 116, 196, 398, INK, 18, ' opacity="0.6"')]
    for i in range(3):
        for j in range(2):
            body.append(place("wasmnest-icon-simple.svg" if j else "wasmnest-icon.svg",
                              px + 74 + i * 62, py + 168 + j * 76, 44, 44))
    body += [place("wasmnest-icon.svg", px + 124, py + 326, 60, 60),
             rect(px + 300, py + 100, PW - 340, 196, INK, 12),
             place("wasmnest-pattern.svg", px + 300, py + 100, PW - 340, 196),
             rect(px + 300, py + 318, PW - 340, 92, CANVAS, 12),
             place("wasmnest-lockup-ink.svg", px + 318, py + 334, PW - 376, 60),
             txt(px + 300, py + 438, "底纹 = 平纹编织（与标志同一套压 / 被压语言），可平铺。", 13.5, FOG),
             txt(px + 300, py + 462, "浅色场景用 ink 反相标志，不要给纸白标志加描边。", 13.5, FOG),
             txt(px + 300, py + 494, "双端同品牌：同一套资产与色板。", 13.5, "#6B7280"),
             txt(px + 40, py + 578, "自适应图标：前景 / 背景 / 单色三层，", 13.5, FOG),
             txt(px + 40, py + 602, "安全区 = 中心 66dp（108dp 画布）。", 13.5, FOG)]
    return "".join(body)


# ============================================================================
# 9 规范
# ============================================================================
def p_rules(px, py):
    body = []
    for i, (s, ok) in enumerate([("深底用纸白股线，琥珀只留一根棒料", True),
                                 ("小尺寸换实心态，轮廓与编织态完全一致", True),
                                 ("重新着色 / 多色渐变 / 外描边", False),
                                 ("拉伸、旋转、投影、外发光", False)]):
        yy = py + 116 + i * 68
        body += [f'<circle cx="{px + 56:.1f}" cy="{yy - 5:.1f}" r="9" '
                 f'fill="{EMBER if ok else "none"}" stroke="{EMBER if ok else "#5C646E"}" '
                 f'stroke-width="1.5"/>',
                 txt(px + 84, yy, s, 15.5, PAPER if ok else FOG)]
    body += [line(px + 40, py + 392, px + PW - 40, py + 392),
             txt(px + 40, py + 380, "反例（禁止）", 13, "#5C646E", MONO),
             f'<g transform="translate({px + 210:.1f},{py + 470:.1f}) scale(1.75,0.62) '
             f'translate({-MB["x0"] - MB["w"] / 2:.2f},{-(MB["y0"] + MB["h"] / 2):.2f})">'
             f'{A.mark_svg(PAPER, EMBER)}</g>',
             txt(px + 40, py + 582, "拉伸：棒料宽度与角度全变", 13, FOG),
             f'<g transform="translate({px + 470:.1f},{py + 470:.1f}) rotate(-22) '
             f'translate({-(MB["x0"] + MB["w"] / 2):.2f},{-(MB["y0"] + MB["h"] / 2):.2f})">'
             f'{A.mark_svg(PAPER, EMBER)}</g>',
             txt(px + 400, py + 582, "旋转：斜接缝与重力方向失效", 13, FOG),
             line(px + 40, py + 620, px + PW - 40, py + 620),
             txt(px + 40, py + 648, "命名：WasmNest（一个词，驼峰）", 15, PAPER, MONO, 500),
             txt(px + 40, py + 676, "不改 com.bedcode.app / bedcode.keystore / SDK 包名 / SDK 标识符。",
                 13, FOG),
             txt(px + 40, py + 700, "落地顺序：视觉资产 → 双端图标 / 闪屏 / 标题栏 / 关于 → 文档。",
                 13, "#6B7280")]
    return "".join(body)


def main():
    for col, row, label, idx, fn in [
            (0, 0, "cover", 1, p_cover),
            (1, 0, "construction", 2, p_construction),
            (2, 0, "mark variants · clear space", 3, p_variants),
            (0, 1, "colour", 4, p_palette),
            (1, 1, "typography", 5, p_type),
            (2, 1, "app icon · favicon", 6, p_icons),
            (0, 2, "application · host window", 7, p_ui),
            (1, 2, "application · mobile · pattern", 8, p_mobile),
            (2, 2, "rules · naming", 9, p_rules)]:
        panel(col, row, label, idx, fn)
    print("\n".join([f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
                     f'viewBox="0 0 {W} {H}">', rect(0, 0, W, H, INK)] + panels + ['</svg>']))


if __name__ == "__main__":
    main()
